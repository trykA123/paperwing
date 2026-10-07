use super::linux::Trash;
use crate::linux_guard::{
    folders::{Directory, FileMove},
    Root,
};
use std::path::Path;

pub(super) fn recycle_file(
    root: &Root,
    file: &str,
    bytes: &[u8],
    data: &Path,
) -> Result<(), String> {
    crate::paths::relative(file)?;
    let parent = root
        .parent(file, false)
        .map_err(|error| error.to_string())?;
    let expected = parent.snapshot().map_err(|error| error.to_string())?;
    if expected.bytes() != Some(bytes) {
        return Err("File changed since the diff was read; file retained".into());
    }
    let path = root
        .value()
        .map_err(|error| error.to_string())?
        .path
        .join(file);
    let source = Directory::open(path.parent().ok_or("Missing trash source parent")?)
        .map_err(|error| error.to_string())?;
    let leaf = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Unsupported trash filename")?;
    let trash = Trash::for_source(&source, data)?;
    trash.recycle_entry(&source, root, &path, |target, name, renamed| {
        source.move_file_to_tracked(
            FileMove {
                leaf,
                target,
                name,
                parent: &parent,
                expected: &expected,
            },
            renamed,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untracked_crlf_file_moves_with_trash_info_and_exact_bytes() {
        let fixture = crate::linux_guard::folders::Fixture::new("discard-trash");
        let repo = fixture.repository("repo");
        let root = Root::open(&repo, &[repo.join(".git")]).unwrap();
        let bytes = b"discard\r\nlast";
        std::fs::write(repo.join("with spaces.txt"), bytes).unwrap();
        let data = fixture.0.join("data");
        recycle_file(&root, "with spaces.txt", bytes, &data).unwrap();
        assert!(!repo.join("with spaces.txt").exists());
        let moved = std::fs::read_dir(data.join("Trash/files"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(std::fs::read(moved.path()).unwrap(), bytes);
        let info = std::fs::read_dir(data.join("Trash/info"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert!(std::fs::read_to_string(info.path())
            .unwrap()
            .contains("with%20spaces.txt"));
    }

    #[test]
    fn unavailable_trash_and_stale_bytes_retain_the_file() {
        let fixture = crate::linux_guard::folders::Fixture::new("discard-trash-refuse");
        let repo = fixture.repository("repo");
        let root = Root::open(&repo, &[repo.join(".git")]).unwrap();
        std::fs::write(repo.join("file.txt"), b"current").unwrap();
        let data = fixture.0.join("not-a-directory");
        std::fs::write(&data, b"blocked").unwrap();
        assert!(recycle_file(&root, "file.txt", b"stale", &data)
            .unwrap_err()
            .contains("changed"));
        assert!(recycle_file(&root, "file.txt", b"current", &data).is_err());
        assert_eq!(std::fs::read(repo.join("file.txt")).unwrap(), b"current");
    }
}
