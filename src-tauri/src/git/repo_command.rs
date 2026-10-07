const HARDENING: [&str; 4] = ["-c", "core.fsmonitor=false", "-c", "core.quotepath=false"];

#[derive(Clone)]
pub(crate) struct RepoGit<'a> {
    path: &'a str,
    optional_locks: bool,
    literal_pathspecs: bool,
    config: Vec<&'a str>,
}

impl<'a> RepoGit<'a> {
    pub(crate) fn at(path: &'a str) -> Self {
        Self {
            path,
            optional_locks: true,
            literal_pathspecs: false,
            config: Vec::new(),
        }
    }

    pub(crate) fn no_optional_locks(mut self) -> Self {
        self.optional_locks = false;
        self
    }

    pub(crate) fn literal_pathspecs(mut self) -> Self {
        self.literal_pathspecs = true;
        self
    }

    pub(crate) fn config(mut self, setting: &'a str) -> Self {
        self.config.push(setting);
        self
    }

    pub(crate) fn argv(&self, args: &[&'a str]) -> Vec<&'a str> {
        let mut argv = Vec::with_capacity(args.len() + 12);
        if !self.optional_locks {
            argv.push("--no-optional-locks");
        }
        argv.extend(["-C", self.path]);
        argv.extend(HARDENING);
        for setting in &self.config {
            argv.extend(["-c", setting]);
        }
        if self.literal_pathspecs {
            argv.push("--literal-pathspecs");
        }
        argv.extend_from_slice(args);
        argv
    }
}

pub(crate) fn harden<'a>(args: &[&'a str]) -> Vec<&'a str> {
    match args {
        ["-C", path, rest @ ..] => RepoGit::at(path).argv(rest),
        _ => args.to_vec(),
    }
}

#[cfg(test)]
pub(crate) fn assert_hardened(argv: &[String], path: &str) {
    let start = argv
        .iter()
        .position(|arg| arg == "-C")
        .expect("argv has -C");
    assert_eq!(argv[start + 1], path);
    assert_eq!(argv[start + 2..start + 6], HARDENING);
}

#[cfg(test)]
pub(crate) fn argv_of(context: &str) -> Vec<String> {
    let activity = super::activity_snapshot()
        .into_iter()
        .rev()
        .find(|entry| entry.context == context);
    activity.expect("activity for context").argv
}

#[cfg(test)]
#[path = "repo_command_tests.rs"]
mod wrapper_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argv_always_carries_the_hardening_flags() {
        assert_eq!(
            RepoGit::at("/r").argv(&["status"]),
            [
                "-C",
                "/r",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.quotepath=false",
                "status"
            ]
        );
    }

    #[test]
    fn options_keep_their_order() {
        let argv = RepoGit::at("/r")
            .no_optional_locks()
            .config("log.showSignature=false")
            .literal_pathspecs()
            .argv(&["log"]);
        assert_eq!(
            argv,
            [
                "--no-optional-locks",
                "-C",
                "/r",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.quotepath=false",
                "-c",
                "log.showSignature=false",
                "--literal-pathspecs",
                "log"
            ]
        );
    }

    #[test]
    fn harden_rewrites_only_arguments_that_start_with_a_directory() {
        assert_eq!(
            harden(&["-C", "/r", "merge", "--ff-only"]),
            [
                "-C",
                "/r",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.quotepath=false",
                "merge",
                "--ff-only"
            ]
        );
        assert_eq!(
            harden(&["clone", "--", "u", "d"]),
            ["clone", "--", "u", "d"]
        );
    }
}
