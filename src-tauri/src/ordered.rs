use std::future::Future;
use std::sync::Arc;
use tauri::async_runtime::{spawn, JoinHandle};
use tokio::sync::Semaphore;

pub(crate) async fn join_ordered<T>(
    handles: Vec<JoinHandle<T>>,
    failed: impl Fn(usize) -> T,
) -> Vec<T> {
    let mut out = Vec::with_capacity(handles.len());
    for (index, handle) in handles.into_iter().enumerate() {
        out.push(match handle.await {
            Ok(value) => value,
            Err(_) => failed(index),
        });
    }
    out
}

pub(crate) async fn map_bounded<I, T, F, Fut>(
    items: Vec<I>,
    limit: usize,
    work: F,
    failed: impl Fn(usize) -> T,
) -> Vec<T>
where
    I: Send + 'static,
    T: Send + 'static,
    F: Fn(I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    let sem = Arc::new(Semaphore::new(limit));
    let work = Arc::new(work);
    let handles = items
        .into_iter()
        .map(|item| {
            let (sem, work) = (sem.clone(), work.clone());
            spawn(async move {
                let _permit = sem.acquire_owned().await;
                work(item).await
            })
        })
        .collect();
    join_ordered(handles, failed).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_panicked_task_keeps_its_slot_and_the_others_keep_their_order() {
        let items: Vec<u32> = (0..6).collect();
        let out = map_bounded(
            items,
            2,
            |n| async move {
                if n == 2 {
                    panic!("synthetic task failure");
                }
                tokio::time::sleep(std::time::Duration::from_millis(u64::from(6 - n))).await;
                Ok(n)
            },
            Err,
        )
        .await;
        assert_eq!(out, vec![Ok(0), Ok(1), Err(2), Ok(3), Ok(4), Ok(5)]);
    }
}
