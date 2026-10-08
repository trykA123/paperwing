use super::*;
use std::sync::Mutex;

pub(super) const COMPARISON_BYTES: usize = 32 * 1024 * 1024;
pub(super) const GLOBAL_BYTES: usize = 64 * 1024 * 1024;
pub(super) const ROW_BYTES: usize = 24 * 1024 * 1024;
const INTERACTIVE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Default)]
struct Counts {
    bytes: usize,
    enrichment: usize,
    waiting: usize,
    comparisons: HashMap<usize, usize>,
}

static COUNTS: std::sync::LazyLock<Mutex<Counts>> =
    std::sync::LazyLock::new(|| Mutex::new(Counts::default()));
static CHANGED: tokio::sync::Notify = tokio::sync::Notify::const_new();

fn counts() -> std::sync::MutexGuard<'static, Counts> {
    COUNTS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) struct Reservation {
    comparison: Arc<AtomicBool>,
    bytes: usize,
    enrichment: bool,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut counts = counts();
        counts.bytes -= self.bytes;
        counts.enrichment -= usize::from(self.enrichment) * self.bytes;
        if let Some(bytes) = counts
            .comparisons
            .get_mut(&(Arc::as_ptr(&self.comparison) as usize))
        {
            *bytes -= self.bytes;
            if *bytes == 0 {
                counts
                    .comparisons
                    .remove(&(Arc::as_ptr(&self.comparison) as usize));
            }
        }
        drop(counts);
        CHANGED.notify_waiters();
    }
}

struct Waiting(bool);

impl Drop for Waiting {
    fn drop(&mut self) {
        if self.0 {
            counts().waiting -= 1;
            CHANGED.notify_waiters();
        }
    }
}

pub(super) async fn acquire(
    job: &Job,
    enrichment: bool,
    bytes: usize,
) -> Result<Reservation, Problem> {
    if bytes > COMPARISON_BYTES {
        return Err(Problem::new(
            "internal",
            "Content window exceeds comparison limit",
        ));
    }

    if !enrichment {
        counts().waiting += 1;
    }
    let _waiting = Waiting(!enrichment);
    loop {
        job.check()?;
        let changed = CHANGED.notified();
        tokio::pin!(changed);
        changed.as_mut().enable();
        if let Some(reservation) = try_acquire(&job.cancel, enrichment, bytes) {
            return Ok(reservation);
        }
        tokio::select! { _ = &mut changed => {}, _ = tokio::time::sleep(Duration::from_millis(25)) => {} }
    }
}

fn try_acquire(cancel: &Arc<AtomicBool>, enrichment: bool, bytes: usize) -> Option<Reservation> {
    let comparison = Arc::as_ptr(cancel) as usize;
    let mut counts = counts();
    if counts.bytes + bytes > GLOBAL_BYTES
        || counts.comparisons.get(&comparison).copied().unwrap_or(0) + bytes > COMPARISON_BYTES
        || (enrichment
            && (counts.waiting > 0 || counts.enrichment + bytes > GLOBAL_BYTES - INTERACTIVE_BYTES))
    {
        return None;
    }
    counts.bytes += bytes;
    counts.enrichment += usize::from(enrichment) * bytes;
    *counts.comparisons.entry(comparison).or_default() += bytes;
    Some(Reservation {
        comparison: cancel.clone(),
        enrichment,
        bytes,
    })
}

#[cfg(test)]
pub(super) fn idle() -> bool {
    let counts = counts();
    counts.bytes == 0 && counts.waiting == 0 && counts.comparisons.is_empty()
}
