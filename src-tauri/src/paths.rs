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
    #[cfg(target_os = "linux")]
    native: crate::linux_guard::Root,
    pub path: PathBuf,
    metadata: Vec<PathBuf>,
    root_identity: String,
    metadata_identities: Vec<Option<String>>,
}

#[derive(Debug)]
pub struct FileBytes {
    pub bytes: Vec<u8>,
    pub modified_ms: Option<u128>,
    pub symlink: bool,
}

pub(crate) fn filename_component(part: &str, windows: bool) -> Result<(), String> {
    if part.is_empty() || part == "." || part == ".." || part.contains(['/', '\0']) {
        return Err("Unsafe filename component".into());
    }
    if windows {
        let stem = part.split('.').next().unwrap_or("").to_uppercase();
        if part.contains('\\')
            || part.ends_with(['.', ' '])
            || part.chars().any(|character| character.is_control() || ":*?\"<>|".contains(character))
            || ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
            || ((stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.chars().count() == 4
                && "0123456789\u{b9}\u{b2}\u{b3}".contains(stem.chars().last().unwrap()))
        {
            return Err("Unsafe or aliased filename component".into());
        }
    }
    Ok(())
}

pub(crate) fn relative_with_policy(path: &str, windows: bool) -> Result<(), String> {
    if path.is_empty() || path.starts_with('/') || (windows && path.contains('\\')) {
        return Err("Unsupported relative path".into());
    }
    for part in path.split('/') {
        filename_component(part, windows)?;
        if part.eq_ignore_ascii_case(".git") || (windows && part.to_ascii_lowercase().starts_with("git~")) {
            return Err("Repository metadata is protected".into());
        }
    }
    Ok(())
}

pub fn relative(path: &str) -> Result<(), String> {
    relative_with_policy(path, cfg!(windows))
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
    #[cfg(target_os = "linux")]
    pub(crate) fn linux_value(&self) -> Result<crate::linux_guard::root::RootValue, String> {
        self.revalidate()?;
        self.native.value().map_err(|error| error.to_string())
    }

    pub fn new(path: &Path, metadata: Vec<PathBuf>) -> Result<Self, String> {
        crate::git::valid_path(path.to_str().ok_or("Unsupported root encoding")?, true)?;
        if !path.is_dir() {
            return Err("Repository root is not a directory".into());
        }
        let path = crate::platform::canonical_path(path)?;
        let mut protected = Vec::new();
        for directory in metadata {
            crate::git::valid_path(
                directory.to_str().ok_or("Unsupported metadata encoding")?,
                true,
            )?;
            protected
                .push(crate::platform::canonical_path(&directory)?);
        }
        protected.push(path.join(".git"));
        let root_identity = crate::platform::physical_identity(&path)?;
        let metadata_identities = protected.iter().map(|directory| {
            match fs::symlink_metadata(directory) {
                Ok(_) => crate::platform::physical_identity(directory).map(Some),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(_) => Err("Repository metadata unavailable".into()),
            }
        }).collect::<Result<_, String>>()?;
        #[cfg(target_os = "linux")]
        let native = crate::linux_guard::Root::open(&path, &protected).map_err(|error| error.to_string())?;
        Ok(Self {
            #[cfg(target_os = "linux")]
            native,
            path,
            metadata: protected,
            root_identity,
            metadata_identities,
        })
    }

    fn revalidate(&self) -> Result<(), String> {
        #[cfg(target_os = "linux")]
        self.native.revalidate().map_err(|error| error.to_string())?;
        if crate::platform::physical_identity(&self.path)? != self.root_identity {
            return Err("Repository root identity changed".into());
        }
        for (directory, expected) in self.metadata.iter().zip(&self.metadata_identities) {
            crate::git::valid_path(directory.to_str().ok_or("Unsupported metadata encoding")?, false)?;
            let identity = match fs::symlink_metadata(directory) {
                Ok(_) => Some(crate::platform::physical_identity(directory)?),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => return Err("Repository metadata unavailable".into()),
            };
            if &identity != expected {
                return Err("Repository metadata identity changed".into());
            }
        }
        Ok(())
    }

    fn protected(&self, path: &Path) -> Result<bool, String> {
        if self.metadata.iter().any(|directory| path.starts_with(directory)) {
            return Ok(true);
        }
        match fs::symlink_metadata(path) {
            Ok(metadata) if !linked(&metadata) => {
                let identity = crate::platform::physical_identity(path)?;
                Ok(self.metadata_identities.iter().any(|expected| expected.as_ref() == Some(&identity)))
            }
            Ok(_) => Ok(false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(_) => Err("Unreadable path".into()),
        }
    }

    pub fn resolve_cached(
        &self,
        relative_path: &str,
        allow_leaf_link: bool,
        _cache: &mut ReadCache,
    ) -> Result<PathBuf, String> {
        relative(relative_path)?;
        self.revalidate()?;
        let mut path = self.path.clone();
        if linked(&fs::symlink_metadata(&path).map_err(|_| "Root unavailable")?) {
            return Err("Linked root is unsupported".into());
        }
        let parts: Vec<_> = relative_path.split('/').collect();
        for (index, part) in parts.iter().enumerate() {
            path.push(part);
            if self.protected(&path)? {
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
        #[cfg(target_os = "linux")]
        if !symlink {
            let file = self.native.read(relative_path, CONTENT_LIMIT).map_err(|error| error.to_string())?;
            self.resolve_cached(relative_path, true, cache)?;
            return Ok(file.map(|file| FileBytes { bytes: file.bytes, modified_ms: file.modified_ms, symlink: false }));
        }
        let bytes = if symlink {
            let target = fs::read_link(&path).map_err(|_| "Unreadable link")?;
            let resolved = crate::platform::canonical_path(&path.parent().ok_or("Missing parent")?.join(&target))
                .map_err(|_| "Dangling or unreadable link")?;
            if !resolved.starts_with(&self.path) || self.protected(&resolved)? {
                return Err("External or metadata link is unsupported".into());
            }
            let within = resolved
                .strip_prefix(&self.path)
                .map_err(|_| "External link")?;
            let within = within.to_str().ok_or("Unsupported link encoding")?;
            #[cfg(windows)]
            let normalized = within.replace('\\', "/");
            #[cfg(windows)]
            let within = normalized.as_str();
            self.resolve_cached(within, false, cache)?;
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
            "//server/share",
            ".GIT/config",
            "a/../file",
            "private-metadata/object",
        ] {
            assert!(root.read(path).is_err(), "{path}");
        }
        for path in ["C:/file", "\\\\?\\C:\\file", "file:stream", "a./file", "NUL.txt", "COM\u{b9}.txt", "git~1/config"] {
            assert!(relative_with_policy(path, true).is_err(), "{path}");
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
            let status = std::process::Command::new("bun")
                .args(["-e", "import {mkdirSync,writeFileSync,symlinkSync} from 'node:fs'; const p=process.env.PAPERWING_JUNCTION_ROOT; mkdirSync(p+'/outside-dir'); writeFileSync(p+'/outside-dir/sentinel','junction sentinel'); symlinkSync(p+'/outside-dir',p+'/repo/junction','junction');"])
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
    #[cfg(target_os = "linux")]
    #[test]
    fn native_components_preserve_literal_characters_and_refuse_traversal() {
        let fixture = crate::platform::Fixture::new("names");
        let repo = fixture.0.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        let root = ReadRoot::new(&repo, vec![]).unwrap();
        for name in ["literal\\backslash", "file:stream", "NUL.txt", "COM¹.txt", "trailing.", "trailing ", "git~1", "λ folder"] {
            fs::write(repo.join(name), name).unwrap();
            assert_eq!(root.read(name).unwrap().unwrap().bytes, name.as_bytes(), "{name}");
        }
        for name in ["../escape", "a/../escape", "/absolute", "a//b", "a/./b", "a\0b", ".git/config", ".GIT/config"] {
            assert!(root.read(name).is_err(), "{name:?}");
        }
        fs::write(repo.join("literal\\target"), b"target").unwrap();
        std::os::unix::fs::symlink("literal\\target", repo.join("link")).unwrap();
        assert_eq!(root.read("link").unwrap().unwrap().bytes, b"literal\\target");
    }

    #[test]
    fn root_and_protected_metadata_replacement_invalidate_read_authority() {
        let fixture = crate::platform::Fixture::new("replacement");
        let repo = fixture.0.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::create_dir_all(repo.join("metadata")).unwrap();
        fs::write(repo.join("file"), b"before").unwrap();
        let root = ReadRoot::new(&repo, vec![repo.join("metadata")]).unwrap();
        fs::rename(repo.join("metadata"), repo.join("old-metadata")).unwrap();
        fs::create_dir(repo.join("metadata")).unwrap();
        assert!(root.read("file").is_err());
        let root = ReadRoot::new(&repo, vec![]).unwrap();
        fs::rename(repo.join(".git"), repo.join("old-git")).unwrap();
        fs::create_dir(repo.join(".git")).unwrap();
        assert!(root.read("file").is_err());
        let root = ReadRoot::new(&repo, vec![]).unwrap();
        fs::rename(&repo, fixture.0.join("old-repo")).unwrap();
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::write(repo.join("file"), b"replacement").unwrap();
        assert!(root.read("file").is_err());
        assert_eq!(fs::read(fixture.0.join("old-repo/file")).unwrap(), b"before");
    }

    #[test]
    fn protected_metadata_file_aliases_are_refused_by_physical_identity() {
        let fixture = crate::platform::Fixture::new("metadata-alias");
        let repo = fixture.0.join("repo");
        fs::create_dir_all(repo.join(".git")).unwrap();
        fs::write(repo.join("metadata-control"), b"protected bytes").unwrap();
        fs::hard_link(repo.join("metadata-control"), repo.join("ordinary-alias")).unwrap();
        let root = ReadRoot::new(&repo, vec![repo.join("metadata-control")]).unwrap();
        assert!(root.read("ordinary-alias").is_err());
        assert_eq!(fs::read(repo.join("metadata-control")).unwrap(), b"protected bytes");
    }

}
