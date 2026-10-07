use crate::git::repository_tree;
use std::path::Path;
use std::process::Command;

fn run(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn fixture() -> std::path::PathBuf {
    let dir = crate::test_support::tmp_root()
        .join("remote-list")
        .join("parity");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    run(&dir, &["init", "-q", "-b", "main"]);
    run(
        &dir,
        &["remote", "add", "origin", "https://a.example/x.git"],
    );
    run(
        &dir,
        &[
            "config",
            "--add",
            "remote.origin.url",
            "https://b.example/y.git",
        ],
    );
    run(
        &dir,
        &["remote", "add", "mirror", "https://old.example/m.git"],
    );
    run(
        &dir,
        &[
            "config",
            "url.https://new.example/.insteadOf",
            "https://old.example/",
        ],
    );
    run(
        &dir,
        &["remote", "add", "pushed", "https://c.example/p.git"],
    );
    run(
        &dir,
        &[
            "config",
            "remote.pushed.pushurl",
            "https://push.example/p.git",
        ],
    );
    run(&dir, &["remote", "add", "plain", "ssh://h.example/o.git"]);
    run(
        &dir,
        &["remote", "add", "hidden", "https://a.example/h.git"],
    );
    run(
        &dir,
        &[
            "config",
            "--add",
            "remote.hidden.url",
            "https://b.example/h2.git",
        ],
    );
    run(
        &dir,
        &[
            "config",
            "remote.hidden.pushurl",
            "https://mirror.example/h.git",
        ],
    );
    run(
        &dir,
        &["remote", "add", "samepush", "https://a.example/s.git"],
    );
    run(
        &dir,
        &[
            "config",
            "--add",
            "remote.samepush.url",
            "https://b.example/s2.git",
        ],
    );
    run(
        &dir,
        &[
            "config",
            "remote.samepush.pushurl",
            "https://a.example/s.git",
        ],
    );
    dir
}

#[tokio::test]
async fn remote_urls_match_get_url_all_for_every_remote_shape() {
    let _serial = crate::test_support::serial().await;
    let dir = fixture();
    let path = dir.to_str().unwrap();
    let tree = repository_tree(path.into()).await.unwrap();
    assert_eq!(tree.remotes.len(), 6);
    for remote in &tree.remotes {
        let expected: Vec<String> = run(&dir, &["remote", "get-url", "--all", &remote.name])
            .lines()
            .map(str::to_string)
            .collect();
        assert_eq!(remote.urls, expected, "remote {}", remote.name);
    }
}

#[tokio::test]
async fn plain_remotes_cost_one_remote_listing_and_no_get_url() {
    let _serial = crate::test_support::serial().await;
    let dir = crate::test_support::tmp_root()
        .join("remote-list")
        .join("plain");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    run(&dir, &["init", "-q", "-b", "main"]);
    for name in ["a", "b", "c"] {
        run(
            &dir,
            &[
                "remote",
                "add",
                name,
                &format!("https://{name}.example/r.git"),
            ],
        );
    }
    let path = dir.to_str().unwrap();
    let tree = repository_tree(path.into()).await.unwrap();
    assert_eq!(tree.remotes.len(), 3);
    let context = format!("Tree: {path}");
    let calls: Vec<_> = crate::git::activity_snapshot()
        .into_iter()
        .filter(|entry| entry.context == context)
        .collect();
    let used = |word: &str| {
        calls
            .iter()
            .filter(|entry| entry.argv.iter().any(|arg| arg == word))
            .count()
    };
    assert_eq!(used("remote"), 1);
    assert_eq!(used("get-url"), 0);
}
