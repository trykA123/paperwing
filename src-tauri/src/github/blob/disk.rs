use super::{cache_error, Blob, Error, MAX_BYTES};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn lock(root: &Path, check: &impl Fn() -> Result<(), Error>) -> Result<File, Error> {
    check()?;
    std::fs::create_dir_all(root).map_err(|_| cache_error())?;
    if std::fs::symlink_metadata(root)
        .map_err(|_| cache_error())?
        .file_type()
        .is_symlink()
    {
        return Err(cache_error());
    }
    let path = root.join("cache.lock");
    if std::fs::symlink_metadata(&path)
        .is_ok_and(|metadata| !metadata.is_file() || metadata.file_type().is_symlink())
    {
        return Err(cache_error());
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|_| cache_error())?;
    #[cfg(test)]
    super::locking_tests::waiting(root);
    file.lock().map_err(|_| cache_error())?;
    check()?;
    cleanup_pending(root)?;
    Ok(file)
}

pub(super) struct Request<'a> {
    pub root: &'a Path,
    pub key: &'a str,
    pub capacity: u64,
}

#[cfg(test)]
pub(super) fn read(root: &Path, key: &str, capacity: u64) -> Result<Option<Blob>, Error> {
    read_checked(
        Request {
            root,
            key,
            capacity,
        },
        || Ok(()),
    )
}

pub(super) fn read_checked(
    request: Request<'_>,
    check: impl Fn() -> Result<(), Error>,
) -> Result<Option<Blob>, Error> {
    let Request {
        root,
        key,
        capacity,
    } = request;
    check()?;
    if !root.exists() {
        return Ok(None);
    }
    let _lock = lock(root, &check)?;
    evict(root, capacity)?;
    let path = root.join(key);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(cache_error()),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_BYTES as u64 + 1024
    {
        return Err(cache_error());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|_| cache_error())?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(MAX_BYTES as u64 + 1025)
        .read_to_end(&mut bytes)
        .map_err(|_| cache_error())?;
    let blob = decode(&bytes)?;
    file.set_times(std::fs::FileTimes::new().set_modified(SystemTime::now()))
        .map_err(|_| cache_error())?;
    check()?;
    Ok(Some(blob))
}

fn decode(bytes: &[u8]) -> Result<Blob, Error> {
    match bytes.split_first() {
        Some((0, bytes)) if bytes.len() <= MAX_BYTES => Ok(Blob {
            size: Some(bytes.len() as u64),
            bytes: Some(bytes.to_vec()),
            binary: false,
        }),
        Some((1, bytes)) => serde_json::from_slice(bytes).map_err(|_| cache_error()),
        _ => Err(cache_error()),
    }
}

fn encode(blob: &Blob) -> Result<Vec<u8>, Error> {
    if let Some(bytes) = &blob.bytes {
        if bytes.len() > MAX_BYTES {
            return Err(cache_error());
        }
        let mut result = vec![0];
        result.extend_from_slice(bytes);
        return Ok(result);
    }
    let mut result = vec![1];
    result.extend(serde_json::to_vec(blob).map_err(|_| cache_error())?);
    Ok(result)
}

#[cfg(test)]
pub(super) fn write(root: &Path, key: &str, blob: &Blob, capacity: u64) -> Result<(), Error> {
    write_checked(
        Request {
            root,
            key,
            capacity,
        },
        blob,
        || Ok(()),
    )
}

pub(super) fn write_checked(
    request: Request<'_>,
    blob: &Blob,
    check: impl Fn() -> Result<(), Error>,
) -> Result<(), Error> {
    let Request {
        root,
        key,
        capacity,
    } = request;
    let _lock = lock(root, &check)?;
    let destination = root.join(key);
    if destination.exists() {
        evict(root, capacity)?;
        return check();
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let temporary = root.join(format!(
        "pending-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let bytes = encode(blob)?;
    let mut published = false;
    let result = (|| {
        stage(&temporary, &bytes)?;
        check()?;
        std::fs::rename(&temporary, &destination).map_err(|_| cache_error())?;
        published = true;
        evict(root, capacity)?;
        check()
    })();
    if temporary.exists() {
        std::fs::remove_file(temporary).map_err(|_| cache_error())?;
    }
    if result.is_err() && published && destination.exists() {
        std::fs::remove_file(destination).map_err(|_| cache_error())?;
    }
    result
}

fn stage(temporary: &Path, bytes: &[u8]) -> Result<(), Error> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(temporary).map_err(|_| cache_error())?;
    file.write_all(bytes).map_err(|_| cache_error())?;
    file.sync_all().map_err(|_| cache_error())?;
    drop(file);
    Ok(())
}

fn evict(root: &Path, capacity: u64) -> Result<(), Error> {
    let mut entries: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|_| cache_error())? {
        let entry = entry.map_err(|_| cache_error())?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if name.len() != 64 || !name.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            continue;
        }
        let metadata = entry.path().symlink_metadata().map_err(|_| cache_error())?;
        if metadata.is_file() && !metadata.file_type().is_symlink() {
            entries.push((
                metadata.modified().map_err(|_| cache_error())?,
                metadata.len(),
                entry.path(),
            ));
        }
    }
    entries.sort();
    let mut used: u64 = entries.iter().map(|(_, size, _)| size).sum();
    for (_, size, path) in entries {
        if used <= capacity {
            break;
        }
        std::fs::remove_file(path).map_err(|_| cache_error())?;
        used -= size;
    }
    Ok(())
}

fn cleanup_pending(root: &Path) -> Result<(), Error> {
    for entry in std::fs::read_dir(root).map_err(|_| cache_error())? {
        let entry = entry.map_err(|_| cache_error())?;
        let name = entry.file_name();
        let Some(name) = name.to_str().and_then(|name| name.strip_prefix("pending-")) else {
            continue;
        };
        let Some((pid, sequence)) = name.split_once('-') else {
            continue;
        };
        if pid.parse::<u32>().is_err() || sequence.parse::<u64>().is_err() {
            continue;
        }
        let metadata = entry.path().symlink_metadata().map_err(|_| cache_error())?;
        if metadata.is_file() && !metadata.file_type().is_symlink() {
            std::fs::remove_file(entry.path()).map_err(|_| cache_error())?;
        }
    }
    Ok(())
}
