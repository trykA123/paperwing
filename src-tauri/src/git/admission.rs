use std::future::Future;
use std::sync::Mutex;
use tokio::sync::Notify;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Class {
    Interactive,
    Enrichment,
}

tokio::task_local! { static CLASS: Class; }

pub(super) fn current() -> Class {
    CLASS.try_with(|class| *class).unwrap_or(Class::Interactive)
}

pub(super) async fn scope<T>(class: Class, future: impl Future<Output = T>) -> T {
    CLASS.scope(class, future).await
}

pub(crate) async fn enrichment<T>(future: impl Future<Output = T>) -> T {
    scope(Class::Enrichment, future).await
}

#[derive(Default)]
struct Counts {
    active: usize,
    enrichment: usize,
    waiting: usize,
}

#[derive(Default)]
pub(super) struct Admission {
    counts: Mutex<Counts>,
    changed: Notify,
}

impl Admission {
    fn counts(&self) -> std::sync::MutexGuard<'_, Counts> {
        self.counts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) async fn acquire(&self, class: Class) -> Permit<'_> {
        let _waiting = Waiting::new(self, class);
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            {
                let mut counts = self.counts();
                let ready = match class {
                    Class::Interactive => counts.active < 32,
                    Class::Enrichment => {
                        counts.active < 31 && counts.enrichment < 16 && counts.waiting == 0
                    }
                };
                if ready {
                    counts.active += 1;
                    counts.enrichment += usize::from(class == Class::Enrichment);
                    return Permit {
                        admission: self,
                        class,
                    };
                }
            }
            changed.await;
        }
    }

    #[cfg(test)]
    pub(super) fn idle(&self) -> bool {
        let counts = self.counts();
        counts.active == 0 && counts.waiting == 0
    }
}

struct Waiting<'a> {
    admission: &'a Admission,
    class: Class,
}

impl<'a> Waiting<'a> {
    fn new(admission: &'a Admission, class: Class) -> Self {
        if class == Class::Interactive {
            admission.counts().waiting += 1;
        }
        Self { admission, class }
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        if self.class == Class::Interactive {
            self.admission.counts().waiting -= 1;
            self.admission.changed.notify_waiters();
        }
    }
}

pub(super) struct Permit<'a> {
    admission: &'a Admission,
    class: Class,
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut counts = self.admission.counts();
        counts.active -= 1;
        counts.enrichment -= usize::from(self.class == Class::Enrichment);
        drop(counts);
        self.admission.changed.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn enrichment_is_capped_and_leaves_the_last_slot_for_interactive_work() {
        let admission = Admission::default();
        let mut enrichment = Vec::new();
        for _ in 0..16 {
            enrichment.push(admission.acquire(Class::Enrichment).await);
        }
        let mut waiting = Box::pin(admission.acquire(Class::Enrichment));
        assert!(futures_util::FutureExt::now_or_never(&mut waiting).is_none());
        let mut interactive = Vec::new();
        for _ in 0..16 {
            interactive.push(admission.acquire(Class::Interactive).await);
        }
        assert_eq!(admission.counts().active, 32);
        drop(enrichment);
        drop(interactive);
        drop(waiting);
        assert!(admission.idle());
    }

    #[tokio::test]
    async fn enrichment_never_takes_the_last_free_runner_slot() {
        let admission = Admission::default();
        let mut held = Vec::new();
        for _ in 0..31 {
            held.push(admission.acquire(Class::Interactive).await);
        }
        let mut queued = Box::pin(admission.acquire(Class::Enrichment));
        assert!(futures_util::FutureExt::now_or_never(&mut queued).is_none());
        let last = admission.acquire(Class::Interactive).await;
        held.pop();
        assert!(futures_util::FutureExt::now_or_never(&mut queued).is_none());
        held.pop();
        let enrichment = queued.await;
        assert_eq!(admission.counts().active, 31);
        drop(enrichment);
        drop(last);
        drop(held);
        assert!(admission.idle());
    }

    #[tokio::test]
    async fn queued_interactive_work_passes_queued_enrichment_and_dropped_waiters_release() {
        let admission = Arc::new(Admission::default());
        let mut held = Vec::new();
        for _ in 0..32 {
            held.push(admission.acquire(Class::Interactive).await);
        }
        let enrichment = admission.acquire(Class::Enrichment);
        let interactive = admission.acquire(Class::Interactive);
        tokio::pin!(enrichment, interactive);
        assert!(futures_util::FutureExt::now_or_never(&mut enrichment).is_none());
        assert!(futures_util::FutureExt::now_or_never(&mut interactive).is_none());
        held.pop();
        assert!(futures_util::FutureExt::now_or_never(&mut enrichment).is_none());
        let permit = interactive.await;
        assert_eq!(admission.counts().active, 32);
        drop(permit);
        drop(held);
    }
}
