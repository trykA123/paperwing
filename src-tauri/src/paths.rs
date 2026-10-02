use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const CONTENT_LIMIT: usize = 2 * 1024 * 1024;

#[derive(Default)]
pub struct ReadCache {
    #[cfg(windows)]
    names: std::collections::HashMap<PathBuf, std::collections::HashSet<std::ffi::OsString>>,
}

#[derive(Clone, Debug)]
pub struct ReadRoot {
    pub path: PathBuf,
    metadata: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct FileBytes {
    pub bytes: Vec<u8>,
    pub modified_ms: Option<u128>,
    pub symlink: bool,
}

pub fn relative(path: &str) -> Result<(), String> {
    if path.is_empty() || path.contains('\\') || path.starts_with('/') {
        return Err("Unsupported relative path".into());
    }
    for part in path.split('/') {
        let stem = part.split('.').next().unwrap_or("").to_uppercase();
        if part.is_empty()
            || part == "."
            || part == ".."
            || part.ends_with(['.', ' '])
            || part
                .chars()
                .any(|character| character.is_control() || ":*?\"<>|".contains(character))
            || part.eq_ignore_ascii_case(".git")
            || part.to_ascii_lowercase().starts_with("git~")
            || ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.chars().count() == 4
                && "0123456789\u{b9}\u{b2}\u{b3}".contains(stem.chars().last().unwrap()))
        {
            return Err("Unsafe or aliased relative path".into());
        }
    }
    Ok(())
}

fn linked(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

fn link_text(target: &Path) -> Result<Vec<u8>, String> {
    let text = target.to_str().ok_or("Unsupported link encoding")?;
    #[cfg(windows)]
    let text = text.replace('\\', "/");
    Ok(text.as_bytes().to_vec())
}

impl ReadRoot {
    pub(crate) fn metadata_locations(&self) -> &[PathBuf] { &self.metadata }

    pub fn new(path: &Path, metadata: Vec<PathBuf>) -> Result<Self, String> {
        crate::git::valid_path(path.to_str().ok_or("Unsupported root encoding")?, true)?;
        if !path.is_dir() {
            return Err("Repository root is not a directory".into());
        }
        let path = fs::canonicalize(path).map_err(|_| "Repository root is unavailable")?;
        let mut protected = Vec::new();
        for directory in metadata {
            crate::git::valid_path(
                directory.to_str().ok_or("Unsupported metadata encoding")?,
                true,
            )?;
            protected
                .push(fs::canonicalize(directory).map_err(|_| "Repository metadata unavailable")?);
        }
        protected.push(path.join(".git"));
        Ok(Self {
            path,
            metadata: protected,
        })
    }

    fn protected(&self, path: &Path) -> bool {
        self.metadata
            .iter()
            .any(|directory| path.starts_with(directory))
    }

    pub fn resolve_cached(
        &self,
        relative_path: &str,
        allow_leaf_link: bool,
        _cache: &mut ReadCache,
    ) -> Result<PathBuf, String> {
        relative(relative_path)?;
        let mut path = self.path.clone();
        if linked(&fs::symlink_metadata(&path).map_err(|_| "Root unavailable")?) {
            return Err("Linked root is unsupported".into());
        }
        let parts: Vec<_> = relative_path.split('/').collect();
        for (index, part) in parts.iter().enumerate() {
            path.push(part);
            if self.protected(&path) {
                return Err("Repository metadata is protected".into());
            }
            match fs::symlink_metadata(&path) {
                Ok(metadata) => {
                    #[cfg(windows)]
                    {
                        let parent = path.parent().ok_or("Missing ancestor")?;
                        if !_cache.names.contains_key(parent) {
                            let names = fs::read_dir(parent)
                                .map_err(|_| "Unreadable ancestor")?
                                .map(|entry| entry.map(|entry| entry.file_name()))
                                .collect::<Result<_, _>>()
                                .map_err(|_| "Unreadable ancestor")?;
                            _cache.names.insert(parent.to_path_buf(), names);
                        }
                        if !_cache.names[parent].contains(std::ffi::OsStr::new(part)) {
                            return Err("Case or short-name alias is unsupported".into());
                        }
                    }
                    if linked(&metadata)
                        && !(allow_leaf_link
                            && index + 1 == parts.len()
                            && metadata.file_type().is_symlink())
                    {
                        return Err("Linked or reparse ancestor is unsupported".into());
                    }
                    if index + 1 != parts.len() && !metadata.is_dir() {
                        return Err("Non-directory ancestor".into());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("Unreadable path".into()),
            }
        }
        Ok(path)
    }

    pub fn read(&self, relative_path: &str) -> Result<Option<FileBytes>, String> {
        self.read_cached(relative_path, &mut ReadCache::default())
    }

    pub fn read_cached(
        &self,
        relative_path: &str,
        cache: &mut ReadCache,
    ) -> Result<Option<FileBytes>, String> {
        let path = self.resolve_cached(relative_path, true, cache)?;
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Unreadable file".into()),
        };
        let symlink = metadata.file_type().is_symlink();
        let bytes = if symlink {
            let target = fs::read_link(&path).map_err(|_| "Unreadable link")?;
            let resolved = fs::canonicalize(path.parent().ok_or("Missing parent")?.join(&target))
                .map_err(|_| "Dangling or unreadable link")?;
            if !resolved.starts_with(&self.path) || self.protected(&resolved) {
                return Err("External or metadata link is unsupported".into());
            }
            let within = resolved
                .strip_prefix(&self.path)
                .map_err(|_| "External link")?;
            self.resolve_cached(&within.to_string_lossy().replace('\\', "/"), false, cache)?;
            link_text(&target)?
        } else {
            if linked(&metadata) || !metadata.is_file() {
                return Err("Unsupported file type".into());
            }
            if metadata.len() > CONTENT_LIMIT as u64 {
                return Err("Content exceeds read limit".into());
            }
            let file = fs::File::open(&path).map_err(|_| "Unreadable file")?;
            let mut bytes = Vec::new();
            file.take(CONTENT_LIMIT as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Unreadable file")?;
            if bytes.len() > CONTENT_LIMIT {
                return Err("Content exceeds read limit".into());
            }
            bytes
        };
        self.resolve_cached(relative_path, true, cache)?;
        Ok(Some(FileBytes {
            bytes,
            symlink,
            modified_ms: metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis()),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_link_text_uses_git_separator_semantics_without_touching_files() {
        assert_eq!(
            link_text(Path::new("folder/child")).unwrap(),
            b"folder/child"
        );
        #[cfg(windows)]
        {
            assert_eq!(
                link_text(Path::new("folder\\child")).unwrap(),
                b"folder/child"
            );
            assert_eq!(
                link_text(Path::new("..\\folder/child")).unwrap(),
                b"../folder/child"
            );
        }
        #[cfg(unix)]
        assert_eq!(
            link_text(Path::new("folder\\child")).unwrap(),
            b"folder\\child"
        );
    }

    #[test]
    fn reads_validate_ancestors_metadata_and_missing_leaves() {
        let directory = std::env::temp_dir().join(format!("paperwing-paths-{}", std::process::id()));
        fs::create_dir_all(directory.join("repo/.git")).unwrap();
        fs::create_dir_all(directory.join("repo/private-metadata")).unwrap();
        fs::write(directory.join("repo/file.txt"), b"unchanged\r\n").unwrap();
        let root = ReadRoot::new(
            &directory.join("repo"),
            vec![directory.join("repo/private-metadata")],
        )
        .unwrap();
        assert_eq!(
            root.read("file.txt").unwrap().unwrap().bytes,
            b"unchanged\r\n"
        );
        assert!(root.read("new/leaf.txt").unwrap().is_none());
        for path in [
            "../escape",
            "a/../../escape",
            "C:/file",
            "//server/share",
            "\\\\?\\C:\\file",
            "file:stream",
            ".GIT/config",
            "a/../file",
            "a./file",
            "NUL.txt",
            "COM\u{b9}.txt",
            "git~1/config",
            "private-metadata/object",
        ] {
            assert!(root.read(path).is_err(), "{path}");
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn links_are_inert_or_refused_and_read_limits_are_enforced() {
        let directory =
            std::env::temp_dir().join(format!("paperwing-path-links-{}", std::process::id()));
        fs::create_dir_all(directory.join("repo/.git")).unwrap();
        fs::write(directory.join("outside"), b"outside sentinel").unwrap();
        fs::write(directory.join("repo/inside"), b"inside").unwrap();
        let root = ReadRoot::new(&directory.join("repo"), vec![]).unwrap();
        fs::File::create(directory.join("repo/large"))
            .unwrap()
            .set_len(CONTENT_LIMIT as u64 + 1)
            .unwrap();
        assert!(root.read("large").unwrap_err().contains("limit"));
        #[cfg(windows)]
        {
            assert!(root.read("INSIDE").is_err());
            assert!(root.read("inside.").is_err());
        }
        #[cfg(unix)]
        let links = std::os::unix::fs::symlink("inside", directory.join("repo/link"));
        #[cfg(windows)]
        let links = std::os::windows::fs::symlink_file("inside", directory.join("repo/link"));
        match links {
            Ok(()) => {
                let link = root.read("link").unwrap().unwrap();
                assert!(link.symlink);
                assert_eq!(link.bytes, b"inside");
                #[cfg(unix)]
                std::os::unix::fs::symlink("../outside", directory.join("repo/external")).unwrap();
                #[cfg(windows)]
                std::os::windows::fs::symlink_file("../outside", directory.join("repo/external"))
                    .unwrap();
                assert!(root.read("external").is_err());
                assert!(root.read("external/missing").is_err());
            }
            Err(error) => println!("Physical symlink fixture unavailable: {error}"),
        }
        assert_eq!(
            fs::read(directory.join("outside")).unwrap(),
            b"outside sentinel"
        );
        #[cfg(windows)]
        {
            let status = std::process::Command::new("rtk")
                .args(["bun", "-e", "import {mkdirSync,writeFileSync,symlinkSync} from 'node:fs'; const p=process.env.PAPERWING_JUNCTION_ROOT; mkdirSync(p+'/outside-dir'); writeFileSync(p+'/outside-dir/sentinel','junction sentinel'); symlinkSync(p+'/outside-dir',p+'/repo/junction','junction');"])
                .env("PAPERWING_JUNCTION_ROOT", &directory).status().unwrap();
            assert!(status.success());
            assert!(root.read("junction/sentinel").is_err());
            assert!(root.read("junction/nonexistent/leaf").is_err());
            assert_eq!(
                fs::read(directory.join("outside-dir/sentinel")).unwrap(),
                b"junction sentinel"
            );
            fs::remove_dir(directory.join("repo/junction")).unwrap();
            println!("Self-contained unprivileged junction rejected; outside sentinel unchanged");
        }
        if let Some(fixture) = std::env::var_os("PAPERWING_P3_LINK_FIXTURE") {
            let fixture = PathBuf::from(fixture);
            let root = ReadRoot::new(&fixture.join("repo"), vec![]).unwrap();
            assert!(root.read("junction/sentinel").is_err());
            assert!(root.read("junction/nonexistent/leaf").is_err());
            assert_eq!(
                fs::read(fixture.join("outside/sentinel")).unwrap(),
                b"junction sentinel"
            );
            println!("External Windows junction fixture rejected; outside sentinel unchanged");
        }
        fs::remove_dir_all(directory).unwrap();
    }
}
