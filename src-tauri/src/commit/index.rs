use std::time::Duration;

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
    async fn query(&self, args: &[&str]) -> Result<Vec<u8>, String> {
        let mut command = crate::git::hygienic_git();
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
        let output = tokio::time::timeout(Duration::from_secs(45), command.output())
            .await
            .map_err(|_| "Could not verify the index entry in time; retry".to_string())?
            .map_err(|error| format!("Could not verify the index entry: {error}"))?;
        if !output.status.success() {
            return Err("Could not verify the index entry; refresh before applying".into());
        }
        Ok(output.stdout)
    }

    async fn current(&self) -> Result<Entry, String> {
        let stages = self.query(&["ls-files", "--stage", "-z"]).await?;
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
            let visible = self.query(&args).await?;
            args.pop();
            args.push("--ita-invisible-in-index");
            visible != self.query(&args).await?
        };
        Ok(Entry {
            stages,
            intent_to_add,
        })
    }

    pub(super) fn stages(&self) -> &[u8] {
        &self.entry.stages
    }

    pub(crate) fn intent_to_add(&self) -> bool {
        self.entry.intent_to_add
    }

    pub(crate) async fn validate(&self) -> Result<(), String> {
        if self.current().await? != self.entry {
            return Err("Index changed since the diff was read; refresh before applying".into());
        }
        Ok(())
    }

    pub(crate) async fn read(root: &str, file: &str) -> Result<Self, String> {
        let state = Self {
            root: root.into(),
            file: file.into(),
            entry: Entry {
                stages: Vec::new(),
                intent_to_add: false,
            },
        };
        let entry = state.current().await?;
        Ok(Self { entry, ..state })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn index_state_ignores_process_git_index_file() {
        let _serial = crate::test_support::serial().await;
        let fixture = crate::commit::test_fixture::Fixture::new();
        fixture.commit("file.txt", b"tracked\n");
        let non_existent = fixture.root.join(".git/non_existent_index");
        std::env::set_var("GIT_INDEX_FILE", &non_existent);
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                std::env::remove_var("GIT_INDEX_FILE");
            }
        }
        let _reset = Reset;
        let state = IndexState::read(&fixture.path(), "file.txt").await.unwrap();
        assert!(!state.stages().is_empty());
    }
}
