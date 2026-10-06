use super::{repository_tree, TEST_RUNNER_LOCK};
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

fn git(dir: &std::path::Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn gitlinks(root: &std::path::Path, paths: &[String]) {
    let sha = "1111111111111111111111111111111111111111";
    let mut child = Command::new("git")
        .current_dir(root)
        .args(["update-index", "-z", "--index-info"])
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = child.stdin.take().unwrap();
        for path in paths {
            write!(input, "160000 {sha} 0\t{path}\0").unwrap();
        }
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn big_repo(root: &std::path::Path, files: usize, remote_branches: usize) {
    std::fs::create_dir_all(root).unwrap();
    git(root, &["init", "-q", "-b", "main"]);
    let mut child = Command::new("git")
        .current_dir(root)
        .args(["fast-import", "--quiet"])
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut input = std::io::BufWriter::new(child.stdin.take().unwrap());
        write!(
            input,
            "commit refs/heads/main\ncommitter a <a@b> 1 +0000\ndata 1\nx\n"
        )
        .unwrap();
        for index in 0..files {
            write!(
                input,
                "M 100644 inline src/module{:04}/component/file{index:06}.rs\ndata 2\nx\n",
                index % 1000
            )
            .unwrap();
        }
        for index in 0..remote_branches {
            write!(
                input,
                "\nreset refs/remotes/origin/feature/branch-{index:06}\nfrom refs/heads/main\n"
            )
            .unwrap();
        }
    }
    assert!(child.wait().unwrap().success());
    git(
        root,
        &[
            "remote",
            "add",
            "origin",
            "https://example.invalid/org/repo.git",
        ],
    );
    git(root, &["reset", "-q", "--mixed", "HEAD"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn measure_repository_tree_on_large_repo() {
    let _runner = TEST_RUNNER_LOCK.lock().await;
    let files: usize = std::env::var("TREE_FILES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120_000);
    let branches: usize = std::env::var("TREE_BRANCHES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2_000);
    let root = std::path::PathBuf::from(std::env::var("SKEIN_TEST_TMP").unwrap())
        .join(format!("big-tree-{files}-{branches}"));
    if !root.join(".git").exists() {
        big_repo(&root, files, branches);
    }
    let start = Instant::now();
    let result = repository_tree(root.to_string_lossy().into_owned()).await;
    match &result {
        Ok(tree) => println!(
            "files={files} branches={branches}: ok, {} remote refs, {:?}",
            tree.remotes.iter().map(|r| r.refs.len()).sum::<usize>(),
            start.elapsed()
        ),
        Err(error) => println!(
            "files={files} branches={branches}: ERR {error:?} after {:?}",
            start.elapsed()
        ),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tree_with_many_tracked_files_still_lists_remote_branches() {
    let _runner = TEST_RUNNER_LOCK.lock().await;
    let root = std::path::PathBuf::from(std::env::var("SKEIN_TEST_TMP").unwrap())
        .join("big-tree-test-150000");
    let _ = std::fs::remove_dir_all(&root);
    big_repo(&root, 150_000, 3);
    let tree = repository_tree(root.to_string_lossy().into_owned())
        .await
        .expect("repository_tree must not fail for a large index");
    assert_eq!(tree.remotes[0].refs.len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tree_lists_gitlinks_declared_in_gitmodules_with_urls() {
    let _runner = TEST_RUNNER_LOCK.lock().await;
    let root =
        std::path::PathBuf::from(std::env::var("SKEIN_TEST_TMP").unwrap()).join("tree-gitlinks");
    let _ = std::fs::remove_dir_all(&root);
    big_repo(&root, 5, 1);
    let sha = "1111111111111111111111111111111111111111";
    git(
        &root,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{sha},libs/dep"),
        ],
    );
    std::fs::write(
        root.join(".gitmodules"),
        "[submodule \"dep\"]\n\tpath = libs/dep\n\turl = https://example.invalid/dep.git\n",
    )
    .unwrap();
    let tree = repository_tree(root.to_string_lossy().into_owned())
        .await
        .unwrap();
    assert_eq!(tree.submodules.len(), 1);
    assert_eq!(tree.submodules[0].path, "libs/dep");
    assert_eq!(tree.submodules[0].sha, sha);
    assert_eq!(
        tree.submodules[0].url.as_deref(),
        Some("https://example.invalid/dep.git")
    );
    assert!(tree.warning.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gitmodules_glob_paths_match_only_the_literal_index_path() {
    let _runner = TEST_RUNNER_LOCK.lock().await;
    let root = std::path::PathBuf::from(std::env::var("SKEIN_TEST_TMP").unwrap())
        .join("tree-literal-path");
    let _ = std::fs::remove_dir_all(&root);
    big_repo(&root, 5, 1);
    let paths = vec!["*".to_string(), "modules/other".to_string()];
    gitlinks(&root, &paths);
    std::fs::write(
        root.join(".gitmodules"),
        "[submodule \"glob\"]\n\tpath = *\n\turl = x\n",
    )
    .unwrap();

    let tree = repository_tree(root.to_string_lossy().into_owned())
        .await
        .unwrap();

    assert_eq!(
        tree.submodules
            .iter()
            .map(|submodule| submodule.path.as_str())
            .collect::<Vec<_>>(),
        ["*"]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn long_gitmodules_path_lists_are_chunked_and_merged() {
    let _runner = TEST_RUNNER_LOCK.lock().await;
    let root =
        std::path::PathBuf::from(std::env::var("SKEIN_TEST_TMP").unwrap()).join("tree-path-chunks");
    let _ = std::fs::remove_dir_all(&root);
    big_repo(&root, 0, 1);
    let paths: Vec<String> = (0..300)
        .map(|index| format!("modules/{index:04}/{}", "component".repeat(11)))
        .collect();
    assert!(
        super::repository_tree::submodule_path_chunks(&root.to_string_lossy(), &paths)
            .unwrap()
            .len()
            > 1
    );
    gitlinks(&root, &paths);
    let modules = paths
        .iter()
        .enumerate()
        .map(|(index, path)| format!("[submodule \"dep{index:04}\"]\n\tpath = {path}\n\turl = x\n"))
        .collect::<String>();
    std::fs::write(root.join(".gitmodules"), modules).unwrap();

    let tree = repository_tree(root.to_string_lossy().into_owned())
        .await
        .unwrap();

    assert_eq!(tree.submodules.len(), paths.len());
    assert_eq!(tree.submodules.first().unwrap().path, paths[0]);
    assert_eq!(tree.submodules.last().unwrap().path, paths[paths.len() - 1]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn broken_optional_parts_keep_branches_and_report_a_warning() {
    let _runner = TEST_RUNNER_LOCK.lock().await;
    let root =
        std::path::PathBuf::from(std::env::var("SKEIN_TEST_TMP").unwrap()).join("tree-warning");
    let _ = std::fs::remove_dir_all(&root);
    big_repo(&root, 5, 2);
    std::fs::write(root.join(".gitmodules"), vec![b'#'; 300 * 1024]).unwrap();
    let tree = repository_tree(root.to_string_lossy().into_owned())
        .await
        .unwrap();
    assert_eq!(tree.remotes[0].refs.len(), 2);
    assert_eq!(tree.branches.len(), 1);
    assert!(tree
        .warning
        .as_deref()
        .unwrap()
        .contains("Submodules unavailable"));
}
