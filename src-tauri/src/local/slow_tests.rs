use super::*;
use std::process::Command;
use std::time::Instant;

fn sh(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success());
}

fn fixture(root: &Path, count: usize) -> Vec<String> {
    (0..count)
        .map(|index| {
            let dir = root.join(format!("repo{index:02}"));
            std::fs::create_dir_all(&dir).unwrap();
            sh(&dir, &["init", "-q", "-b", "main"]);
            sh(
                &dir,
                &[
                    "-c",
                    "user.name=a",
                    "-c",
                    "user.email=a@b",
                    "commit",
                    "-q",
                    "--allow-empty",
                    "-m",
                    "x",
                ],
            );
            dir.to_string_lossy().into_owned()
        })
        .collect()
}

fn shim(root: &Path, millis: u64) -> std::path::PathBuf {
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let log = root.join("spawns.log");
    let script = format!(
        "#!/bin/sh\necho \"$(date +%s%N) $*\" >> {}\nsleep {}\nexec /usr/bin/git \"$@\"\n",
        log.display(),
        millis as f64 / 1000.0
    );
    std::fs::write(bin.join("git"), script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(bin.join("git"), std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn measure_slow_git_local_status_and_refs() {
    let root =
        std::path::PathBuf::from(std::env::var("SKEIN_TEST_TMP").unwrap()).join("slow-fixture");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let count: usize = std::env::var("SLOW_COUNT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60);
    let paths = fixture(&root, count);
    let bin = shim(&root, 150);
    std::env::set_var(
        "PATH",
        format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
    );
    let log = root.join("spawns.log");
    let spawns = || {
        std::fs::read_to_string(&log)
            .map(|text| text.lines().count())
            .unwrap_or(0)
    };

    let start = Instant::now();
    let rows = local_status(paths.clone()).await;
    println!(
        "local_status: {} rows, {} spawns, all-rows and first-row both {:?}",
        rows.len(),
        spawns(),
        start.elapsed()
    );

    let before = spawns();
    let start = Instant::now();
    let rows = crate::git::get_refs_many(paths.clone()).await;
    println!(
        "get_refs_many one call (8 per call): {} rows, {} spawns, {:?}",
        rows.len(),
        spawns() - before,
        start.elapsed()
    );

    let before = spawns();
    let start = Instant::now();
    let handles: Vec<_> = paths
        .iter()
        .cloned()
        .map(|path| {
            tauri::async_runtime::spawn(async move { crate::git::get_refs_many(vec![path]).await })
        })
        .collect();
    let mut first = None;
    for handle in handles {
        handle.await.unwrap();
        first.get_or_insert(start.elapsed());
    }
    println!(
        "get_refs_many one url per call (frontend pattern): first {:?}, all {:?}, {} spawns",
        first.unwrap(),
        start.elapsed(),
        spawns() - before
    );
}
