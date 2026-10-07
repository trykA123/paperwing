use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

static READ_FAILURES: OnceLock<Mutex<HashMap<PathBuf, usize>>> = OnceLock::new();

pub(crate) struct ReadFailure(PathBuf);

impl ReadFailure {
    pub(crate) fn new(path: &Path, count: usize) -> Self {
        READ_FAILURES
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), count);
        Self(path.to_path_buf())
    }
}

impl Drop for ReadFailure {
    fn drop(&mut self) {
        READ_FAILURES.get().unwrap().lock().unwrap().remove(&self.0);
    }
}

pub(super) fn check_read(path: &Path) -> std::io::Result<()> {
    let mut failures = READ_FAILURES.get_or_init(Default::default).lock().unwrap();
    if let Some(remaining) = failures.get_mut(path).filter(|remaining| **remaining > 0) {
        *remaining -= 1;
        return Err(Error::new(
            ErrorKind::PermissionDenied,
            "injected settings read failure",
        ));
    }
    Ok(())
}
