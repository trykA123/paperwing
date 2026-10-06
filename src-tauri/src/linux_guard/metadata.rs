use super::{Error, ErrorKind, Handle};
use rustix::fs::{FileType, Mode, XattrFlags};
use std::collections::BTreeMap;
use std::os::fd::AsRawFd;

const NAMES_LIMIT: usize = 64 * 1024;
const ATTR_LIMIT: usize = 256 * 1024;
const FLAGS_UNSUPPORTED: libc::c_long = 0x10 | 0x20 | 0x4000_0000;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Security { pub mode: u32, pub uid: u32, pub gid: u32, pub attributes: BTreeMap<String, Vec<u8>> }

fn owner(handle: &Handle) -> Result<rustix::fs::Stat, Error> {
    let stat = rustix::fs::fstat(handle).map_err(Error::io)?;
    if stat.st_uid != unsafe { libc::geteuid() } || stat.st_gid != unsafe { libc::getegid() } {
        return Err(Error::unsupported("Foreign ownership or group is unsupported for Linux writes"));
    }
    if stat.st_mode & 0o7000 != 0 { return Err(Error::unsupported("Special permission bits are unsupported for Linux writes")); }
    let mut flags: libc::c_long = 0;
    if unsafe { libc::ioctl(handle.file.as_raw_fd(), libc::FS_IOC_GETFLAGS, &mut flags) } < 0 {
        return Err(Error::unsupported("Linux inode flags could not be verified"));
    }
    if flags & FLAGS_UNSUPPORTED != 0 { return Err(Error::unsupported("Immutable, append-only or casefold objects are unsupported")); }
    Ok(stat)
}

fn attributes(handle: &Handle, directory: bool) -> Result<BTreeMap<String, Vec<u8>>, Error> {
    let mut names = vec![0; NAMES_LIMIT];
    let count = rustix::fs::flistxattr(handle, names.as_mut_slice()).map_err(Error::io)?;
    names.truncate(count);
    let mut attributes = BTreeMap::new();
    let mut total = 0;
    for name in names.split(|byte| *byte == 0).filter(|name| !name.is_empty()) {
        let name = std::str::from_utf8(name).map_err(|_| Error::unsupported("Unsupported metadata encoding"))?;
        if !(name.starts_with("user.") || (!directory && name == "system.posix_acl_access")) {
            return Err(Error::unsupported("Unsupported security metadata or inherited ACL"));
        }
        if attributes.len() >= 64 { return Err(Error::new(ErrorKind::Limit, "Too many file metadata attributes")); }
        let mut value = vec![0; NAMES_LIMIT];
        let length = rustix::fs::fgetxattr(handle, name, value.as_mut_slice()).map_err(Error::io)?;
        value.truncate(length); total += length;
        if total > ATTR_LIMIT { return Err(Error::new(ErrorKind::Limit, "File metadata exceeds the limit")); }
        attributes.insert(name.to_string(), value);
    }
    Ok(attributes)
}

pub(super) fn directory(handle: &Handle) -> Result<(), Error> {
    let stat = owner(handle)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::Directory { return Err(Error::unsupported("An ordinary directory is required")); }
    attributes(handle, true)?;
    Ok(())
}

pub(super) fn snapshot(handle: &Handle) -> Result<Security, Error> {
    let stat = owner(handle)?;
    if FileType::from_raw_mode(stat.st_mode) != FileType::RegularFile || stat.st_nlink != 1 {
        return Err(Error::unsupported("Linux writes require an ordinary file with one link"));
    }
    Ok(Security { mode: stat.st_mode & 0o777, uid: stat.st_uid, gid: stat.st_gid, attributes: attributes(handle, false)? })
}

impl Security {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.mode & !0o777 != 0 || self.uid != unsafe { libc::geteuid() } || self.gid != unsafe { libc::getegid() } {
            return Err(Error::unsupported("Unsupported persisted file ownership or permissions"));
        }
        if self.attributes.len() > 64 { return Err(Error::new(ErrorKind::Limit, "Too many file metadata attributes")); }
        let mut names = 0usize;
        let mut total = 0usize;
        for (name, value) in &self.attributes {
            if name.as_bytes().contains(&0) || !(name.starts_with("user.") && name.len() > 5 || name == "system.posix_acl_access") {
                return Err(Error::unsupported("Unsupported persisted file metadata name"));
            }
            names = names.checked_add(name.len() + 1).ok_or_else(|| Error::new(ErrorKind::Limit, "Metadata name limit reached"))?;
            total = total.checked_add(value.len()).ok_or_else(|| Error::new(ErrorKind::Limit, "Metadata value limit reached"))?;
            if names > NAMES_LIMIT || value.len() > NAMES_LIMIT || total > ATTR_LIMIT {
                return Err(Error::new(ErrorKind::Limit, "Persisted file metadata exceeds the limit"));
            }
        }
        Ok(())
    }

    pub(crate) fn new_file() -> Self {
        Self { mode: 0o644, uid: unsafe { libc::geteuid() }, gid: unsafe { libc::getegid() }, attributes: BTreeMap::new() }
    }
    pub(super) fn apply(&self, handle: &Handle) -> Result<(), Error> {
        self.validate()?;
        let actual = snapshot(handle)?;
        if actual.uid != self.uid || actual.gid != self.gid { return Err(Error::unsupported("Staged file ownership differs")); }
        for name in actual.attributes.keys() {
            if !self.attributes.contains_key(name) { rustix::fs::fremovexattr(handle, name.as_str()).map_err(Error::io)?; }
        }
        rustix::fs::fchmod(handle, Mode::from_raw_mode(self.mode)).map_err(Error::io)?;
        for (name, value) in &self.attributes { rustix::fs::fsetxattr(handle, name.as_str(), value, XattrFlags::empty()).map_err(Error::io)?; }
        if &snapshot(handle)? != self { return Err(Error::conflict("Staged file metadata differs")); }
        rustix::fs::fsync(handle).map_err(Error::io)
    }
}
