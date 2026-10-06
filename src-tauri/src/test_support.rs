use std::path::PathBuf;
use std::sync::{Condvar, Mutex as StdMutex, MutexGuard as StdGuard};
use tokio::sync::{Mutex, MutexGuard};

static SERIAL: Mutex<()> = Mutex::const_new(());
thread_local!(static OWNS_EXCLUSIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
static BUDGET: Budget = Budget {
    state: StdMutex::new(0),
    changed: Condvar::new(),
};

struct Budget {
    state: StdMutex<isize>,
    changed: Condvar,
}
impl Budget {
    fn lock(&self) -> StdGuard<'_, isize> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
    fn wait<'a>(
        &'a self,
        mut state: StdGuard<'a, isize>,
        blocked: fn(isize) -> bool,
    ) -> StdGuard<'a, isize> {
        while blocked(*state) {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
        state
    }
}

pub(crate) struct Shared(bool);
impl Shared {
    pub(crate) fn new() -> Self {
        if OWNS_EXCLUSIVE.get() {
            return Self(false);
        }
        let mut state = BUDGET.wait(BUDGET.lock(), |held| held < 0);
        *state += 1;
        Self(true)
    }
}
impl Drop for Shared {
    fn drop(&mut self) {
        if !self.0 {
            return;
        }
        *BUDGET.lock() -= 1;
        BUDGET.changed.notify_all();
    }
}

pub(crate) struct Exclusive;
impl Exclusive {
    pub(crate) fn new() -> Self {
        let mut state = BUDGET.wait(BUDGET.lock(), |held| held != 0);
        *state = -1;
        OWNS_EXCLUSIVE.set(true);
        Self
    }
}
impl Drop for Exclusive {
    fn drop(&mut self) {
        OWNS_EXCLUSIVE.set(false);
        *BUDGET.lock() = 0;
        BUDGET.changed.notify_all();
    }
}

pub(crate) struct Serial {
    _budget: Shared,
    _serial: MutexGuard<'static, ()>,
    _runner: MutexGuard<'static, ()>,
}

pub(crate) fn tmp_root() -> PathBuf {
    let root = std::env::var_os("PAPERWING_TEST_TMP")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-tmp"));
    let _ = std::fs::create_dir_all(&root);
    root
}

pub(crate) async fn serial() -> Serial {
    let budget = Shared::new();
    let serial = SERIAL.lock().await;
    let runner = crate::git::TEST_RUNNER_LOCK.lock().await;
    Serial {
        _budget: budget,
        _serial: serial,
        _runner: runner,
    }
}
