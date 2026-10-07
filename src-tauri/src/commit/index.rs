use std::process::Command;

#[derive(PartialEq, Eq)]
struct Entry {
    stages: Vec<u8>,
    intent_to_add: bool,
}

pub(crate) struct IndexState {
    root: String,
    file: String,
    entry: Entry,
}

impl IndexState {
    fn query(&self, args: &[&str]) -> Result<Vec<u8>, String> {
        let mut command = Command::new("git");
        command
            .args([
                "-C",
                &self.root,
                "-c",
                "core.fsmonitor=false",
                "--literal-pathspecs",
            ])
            .args(args)
            .args(["--", &self.file]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        let output = command
            .output()
            .map_err(|error| format!("Could not verify the index entry: {error}"))?;
        if !output.status.success() {
            return Err("Could not verify the index entry; refresh before applying".into());
        }
        Ok(output.stdout)
    }

    fn current(&self) -> Result<Entry, String> {
        let stages = self.query(&["ls-files", "--stage", "-z"])?;
        let intent_to_add = if stages.is_empty() {
            false
        } else {
            let mut args = vec![
                "diff",
                "--cached",
                "--name-only",
                "-z",
                "--no-renames",
                "--no-ext-diff",
                "--no-textconv",
                "--ita-visible-in-index",
            ];
            let visible = self.query(&args)?;
            args.pop();
            args.push("--ita-invisible-in-index");
            visible != self.query(&args)?
        };
        Ok(Entry {
            stages,
            intent_to_add,
        })
    }

    pub(super) fn stages(&self) -> &[u8] {
        &self.entry.stages
    }

    pub(super) fn intent_to_add(&self) -> bool {
        self.entry.intent_to_add
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.current()? != self.entry {
            return Err("Index changed since the diff was read; refresh before applying".into());
        }
        Ok(())
    }

    pub(super) async fn read(root: &str, file: &str) -> Result<Self, String> {
        let state = Self {
            root: root.into(),
            file: file.into(),
            entry: Entry {
                stages: Vec::new(),
                intent_to_add: false,
            },
        };
        tauri::async_runtime::spawn_blocking(move || {
            let entry = state.current()?;
            Ok(Self { entry, ..state })
        })
        .await
        .map_err(|_| "Could not verify the index entry")?
    }
}
