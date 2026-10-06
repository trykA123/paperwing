use super::*;
use crate::platform::Fixture;
use std::sync::atomic::AtomicBool;

fn repo(root: &Path, relative: &str) {
    let git = root.join(relative).join(".git");
    fs::create_dir_all(&git).unwrap();
    fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
}

fn run(root: &Path, limits: Limits) -> (Vec<FoundRepo>, Summary) {
    let cancel = AtomicBool::new(false);
    let mut found = Vec::new();
    let summary = scan(root, limits, &cancel, &mut |event| {
        if let Event::Repo(item) = event {
            found.push(item);
        }
    });
    (found, summary)
}

fn names(found: &[FoundRepo]) -> Vec<String> {
    let mut names: Vec<_> = found.iter().map(|item| item.name.clone()).collect();
    names.sort();
    names
}

#[test]
fn finds_nested_repositories_and_stops_inside_them() {
    let fixture = Fixture::new("discover-nested");
    repo(&fixture.0, "a");
    repo(&fixture.0, "group/b");
    repo(&fixture.0, "a/inner");
    let (found, summary) = run(&fixture.0, Limits::default());
    assert_eq!(names(&found), ["a", "b"]);
    assert_eq!(found[0].branch.as_deref(), Some("main"));
    assert_eq!(summary.repositories, 2);
    assert_eq!(summary.capped, None);
}

#[test]
fn a_repository_root_is_reported_as_the_only_result() {
    let fixture = Fixture::new("discover-root");
    repo(&fixture.0, "");
    repo(&fixture.0, "child");
    let (found, _) = run(&fixture.0, Limits::default());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].path, fixture.0.to_str().unwrap());
}

#[test]
fn detached_heads_report_no_branch() {
    let fixture = Fixture::new("discover-detached");
    repo(&fixture.0, "a");
    fs::write(
        fixture.0.join("a/.git/HEAD"),
        "0123456789012345678901234567890123456789\n",
    )
    .unwrap();
    let (found, _) = run(&fixture.0, Limits::default());
    assert!(found[0].detached);
    assert_eq!(found[0].branch, None);
}

#[test]
fn recognises_worktrees_and_reads_their_branch() {
    let fixture = Fixture::new("discover-worktree");
    let admin = fixture.0.join("main/.git/worktrees/feature");
    repo(&fixture.0, "main");
    fs::create_dir_all(&admin).unwrap();
    fs::write(admin.join("HEAD"), "ref: refs/heads/feature\n").unwrap();
    fs::create_dir_all(fixture.0.join("elsewhere/feature")).unwrap();
    fs::write(
        fixture.0.join("elsewhere/feature/.git"),
        format!("gitdir: {}\n", admin.display()),
    )
    .unwrap();
    let (found, _) = run(&fixture.0, Limits::default());
    let worktree = found.iter().find(|item| item.name == "feature").unwrap();
    assert_eq!(worktree.kind, RepoKind::Worktree);
    assert_eq!(worktree.branch.as_deref(), Some("feature"));
}

#[test]
fn reports_initialised_submodules_as_children_of_their_repository() {
    let fixture = Fixture::new("discover-submodule");
    repo(&fixture.0, "super");
    fs::write(fixture.0.join("super/.gitmodules"), "[submodule \"lib\"]\n\tpath = libs/lib\n\turl = x\n[submodule \"gone\"]\n\tpath = gone\n[submodule \"bad\"]\n\tpath = ../escape\n").unwrap();
    let modules = fixture.0.join("super/.git/modules/lib");
    fs::create_dir_all(&modules).unwrap();
    fs::write(modules.join("HEAD"), "ref: refs/heads/dev\n").unwrap();
    fs::create_dir_all(fixture.0.join("super/libs/lib")).unwrap();
    fs::write(
        fixture.0.join("super/libs/lib/.git"),
        "gitdir: ../../.git/modules/lib\n",
    )
    .unwrap();
    let (found, _) = run(&fixture.0, Limits::default());
    assert_eq!(names(&found), ["lib", "super"]);
    let child = found.iter().find(|item| item.name == "lib").unwrap();
    assert_eq!(child.kind, RepoKind::Submodule);
    assert_eq!(child.branch.as_deref(), Some("dev"));
    assert_eq!(child.parent.as_deref(), fixture.0.join("super").to_str());
}

#[test]
fn recognises_bare_repositories() {
    let fixture = Fixture::new("discover-bare");
    let bare = fixture.0.join("origin.git");
    fs::create_dir_all(bare.join("objects")).unwrap();
    fs::create_dir_all(bare.join("refs")).unwrap();
    fs::write(bare.join("HEAD"), "ref: refs/heads/trunk\n").unwrap();
    let (found, _) = run(&fixture.0, Limits::default());
    assert_eq!(found[0].kind, RepoKind::Bare);
    assert_eq!(found[0].branch.as_deref(), Some("trunk"));
}

#[test]
fn skips_dependency_build_and_hidden_directories() {
    let fixture = Fixture::new("discover-skip");
    for skipped in [
        "node_modules/x",
        "target/x",
        "dist/x",
        "build/x",
        ".venv/x",
        "venv/x",
        ".cache/x",
        "vendor/x",
        ".tox/x",
    ] {
        repo(&fixture.0, skipped);
    }
    repo(&fixture.0, "kept");
    let (found, _) = run(&fixture.0, Limits::default());
    assert_eq!(names(&found), ["kept"]);
}

#[cfg(unix)]
#[test]
fn never_follows_links_including_loops_and_escapes() {
    use std::os::unix::fs::symlink;
    let outside = Fixture::new("discover-outside");
    repo(&outside.0, "secret");
    let fixture = Fixture::new("discover-links");
    repo(&fixture.0, "real");
    symlink(&fixture.0, fixture.0.join("loop")).unwrap();
    symlink(&outside.0, fixture.0.join("escape")).unwrap();
    symlink(fixture.0.join("real"), fixture.0.join("alias")).unwrap();
    let (found, summary) = run(&fixture.0, Limits::default());
    assert_eq!(names(&found), ["real"]);
    assert_eq!(summary.links_skipped, 3);
}

#[test]
fn depth_limit_bounds_the_search() {
    let fixture = Fixture::new("discover-depth");
    repo(&fixture.0, "a/b/c/d");
    repo(&fixture.0, "a/b/c/d/e/f");
    repo(&fixture.0, "x/y/z/w/v");
    let (found, _) = run(&fixture.0, Limits::default());
    assert_eq!(names(&found), ["d"]);
    let (found, _) = run(
        &fixture.0,
        Limits {
            depth: 5,
            ..Limits::default()
        },
    );
    assert_eq!(names(&found), ["d", "v"]);
    let (found, _) = run(
        &fixture.0,
        Limits {
            depth: 1,
            ..Limits::default()
        },
    );
    assert!(found.is_empty());
}

#[test]
fn directory_cap_stops_the_scan_and_says_so() {
    let fixture = Fixture::new("discover-dirs");
    for index in 0..10_050 {
        fs::create_dir(fixture.0.join(format!("d{index:05}"))).unwrap();
    }
    repo(&fixture.0, "zzz");
    let (_, summary) = run(&fixture.0, Limits::default());
    assert_eq!(summary.directories, 10_000);
    assert_eq!(summary.capped, Some(Cap::Directories));
}

#[test]
fn repository_cap_stops_at_the_limit() {
    let fixture = Fixture::new("discover-repos");
    for index in 0..8 {
        repo(&fixture.0, &format!("r{index}"));
    }
    let (found, summary) = run(
        &fixture.0,
        Limits {
            repositories: 5,
            ..Limits::default()
        },
    );
    assert_eq!(found.len(), 5);
    assert_eq!(summary.capped, Some(Cap::Repositories));
    let (found, summary) = run(
        &fixture.0,
        Limits {
            repositories: 8,
            ..Limits::default()
        },
    );
    assert_eq!(found.len(), 8);
    assert_eq!(summary.capped, None);
}

#[test]
fn cancellation_mid_scan_returns_partial_results() {
    let fixture = Fixture::new("discover-cancel");
    for index in 0..6 {
        repo(&fixture.0, &format!("r{index}"));
    }
    let cancel = AtomicBool::new(false);
    let mut seen = 0;
    let summary = scan(&fixture.0, Limits::default(), &cancel, &mut |event| {
        if let Event::Repo(_) = event {
            seen += 1;
            if seen == 2 {
                cancel.store(true, Ordering::Relaxed);
            }
        }
    });
    assert!(summary.cancelled);
    assert_eq!(seen, 2);
}

#[cfg(target_os = "linux")]
#[test]
fn fifo_and_linked_metadata_files_never_block_or_escape() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new("discover-fifo");
    repo(&fixture.0, "fifo");
    let head = fixture.0.join("fifo/.git/HEAD");
    fs::remove_file(&head).unwrap();
    let name = CString::new(head.as_os_str().as_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    repo(&fixture.0, "linked");
    let secret = fixture.0.join("secret");
    fs::write(&secret, "ref: refs/heads/leaked\n").unwrap();
    let head = fixture.0.join("linked/.git/HEAD");
    fs::remove_file(&head).unwrap();
    symlink(&secret, &head).unwrap();
    fs::create_dir_all(fixture.0.join("marker")).unwrap();
    symlink(fixture.0.join("linked/.git"), fixture.0.join("marker/.git")).unwrap();
    let (found, _) = run(&fixture.0, Limits::default());
    assert_eq!(names(&found), ["fifo", "linked"]);
    assert!(found.iter().all(|item| item.branch.is_none()));
}

#[test]
fn gitdir_targets_outside_the_folder_or_unc_style_report_no_branch() {
    let outside = Fixture::new("discover-gitdir-outside");
    fs::write(outside.0.join("HEAD"), "ref: refs/heads/leaked\n").unwrap();
    let fixture = Fixture::new("discover-gitdir");
    for (name, text) in [
        ("away", format!("gitdir: {}\n", outside.0.display())),
        ("unc", "gitdir: \\\\server\\share\\repo\n".to_string()),
        ("up", "gitdir: ../../../../../../../etc\n".to_string()),
    ] {
        fs::create_dir_all(fixture.0.join(name)).unwrap();
        fs::write(fixture.0.join(name).join(".git"), text).unwrap();
    }
    let (found, _) = run(&fixture.0, Limits::default());
    assert_eq!(names(&found), ["away", "unc", "up"]);
    assert!(found
        .iter()
        .all(|item| item.branch.is_none() && !item.detached));
}

#[test]
fn repositories_named_like_skipped_directories_are_still_found() {
    let fixture = Fixture::new("discover-named");
    for name in ["build", "dist", "target", "vendor", "node_modules"] {
        repo(&fixture.0, name);
    }
    repo(&fixture.0, "vendor2/x");
    repo(&fixture.0, ".config/tool");
    repo(&fixture.0, ".idea/x");
    let (found, _) = run(&fixture.0, Limits::default());
    assert_eq!(
        names(&found),
        [
            "build",
            "dist",
            "node_modules",
            "target",
            "tool",
            "vendor",
            "x"
        ]
    );
}

#[test]
fn a_scan_cancelled_before_listing_visits_nothing() {
    let fixture = Fixture::new("discover-cancel-listing");
    for index in 0..50 {
        fs::create_dir(fixture.0.join(format!("d{index}"))).unwrap();
    }
    let cancel = AtomicBool::new(true);
    let summary = scan(&fixture.0, Limits::default(), &cancel, &mut |_| {});
    assert!(summary.cancelled);
    assert_eq!(summary.directories, 0);
}
