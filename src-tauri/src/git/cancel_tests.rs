use super::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicUsize;
use std::task::{Context, Poll, Wake, Waker};

struct Counter(AtomicUsize);

impl Wake for Counter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

type Queued = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

fn queued(external: Option<Arc<AtomicBool>>, stop: Arc<Stop>) -> Queued {
    Box::pin(async move { admitted(std::future::pending::<()>(), &external, &stop).await })
}

fn poll(future: &mut Queued, counter: &Arc<Counter>) -> Poll<Result<(), String>> {
    let waker = Waker::from(counter.clone());
    future.as_mut().poll(&mut Context::from_waker(&waker))
}

#[tokio::test]
async fn idle_queued_jobs_receive_no_wakeups_until_one_is_cancelled() {
    let counter = Arc::new(Counter(AtomicUsize::new(0)));
    let stops: Vec<_> = (0..800).map(|_| Stop::new()).collect();
    let mut jobs: Vec<Queued> = stops
        .iter()
        .map(|stop| queued(None, stop.clone()))
        .collect();
    for job in &mut jobs {
        assert!(poll(job, &counter).is_pending());
    }
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert_eq!(counter.0.load(Ordering::SeqCst), 0);
    stops[7].request();
    assert_eq!(counter.0.load(Ordering::SeqCst), 1);
    assert_eq!(
        poll(&mut jobs[7], &counter),
        Poll::Ready(Err("Git command cancelled".into()))
    );
    assert!(poll(&mut jobs[8], &counter).is_pending());
}

#[tokio::test]
async fn an_external_flag_wakes_only_its_own_queued_job_through_the_shared_watcher() {
    let counter = Arc::new(Counter(AtomicUsize::new(0)));
    let flags: Vec<_> = (0..50).map(|_| Arc::new(AtomicBool::new(false))).collect();
    let stops: Vec<_> = flags.iter().map(|_| Stop::new()).collect();
    let guards: Vec<_> = flags
        .iter()
        .zip(&stops)
        .map(|(flag, stop)| watch(Some(flag), stop))
        .collect();
    let mut jobs: Vec<Queued> = flags
        .iter()
        .zip(&stops)
        .map(|(flag, stop)| queued(Some(flag.clone()), stop.clone()))
        .collect();
    for job in &mut jobs {
        assert!(poll(job, &counter).is_pending());
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(counter.0.load(Ordering::SeqCst), 0);
    flags[3].store(true, Ordering::Relaxed);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(counter.0.load(Ordering::SeqCst), 1);
    assert_eq!(
        poll(&mut jobs[3], &counter),
        Poll::Ready(Err("Git command cancelled".into()))
    );
    drop(guards);
    let watched = |flag: &Arc<AtomicBool>| {
        WATCHER
            .lock()
            .unwrap()
            .watches
            .iter()
            .any(|watch| Arc::ptr_eq(&watch.external, flag))
    };
    assert!(!flags.iter().any(watched));
}
