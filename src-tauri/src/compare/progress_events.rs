use super::*;
use std::collections::VecDeque;
use std::time::Instant;

pub(super) const INTERVAL: Duration = Duration::from_millis(100);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Notice {
    pub id: String,
    pub generation: u64,
    pub sequence: u64,
    pub state: State,
}

type Sink = Arc<dyn Fn(&Notice) + Send + Sync>;

pub(super) struct Events {
    queue: VecDeque<Notice>,
    sent: Option<Instant>,
    sink: Option<Sink>,
}

impl Events {
    pub fn new(app: Option<tauri::AppHandle>) -> Self {
        let sink = app.map(|app| {
            Arc::new(move |notice: &Notice| {
                let _ = crate::events::publish_payload(
                    &app,
                    crate::kernel::events::CoreEvent::CompareProgress,
                    notice,
                );
            }) as Sink
        });
        Self {
            queue: VecDeque::new(),
            sent: None,
            sink,
        }
    }

    pub fn queue(&mut self, notice: Notice) {
        if self.sink.is_none() {
            return;
        }
        if let Some(last) = self
            .queue
            .back_mut()
            .filter(|last| last.state == notice.state)
        {
            *last = notice;
        } else {
            self.queue.push_back(notice);
        }
    }

    fn flush(&mut self, now: Instant) -> Duration {
        let delay = self
            .sent
            .map(|sent| (sent + INTERVAL).saturating_duration_since(now))
            .unwrap_or_default();
        if !delay.is_zero() {
            return delay;
        }
        if let Some(notice) = self.queue.pop_front() {
            if let Some(sink) = &self.sink {
                sink(&notice);
            }
            self.sent = Some(now);
        }
        INTERVAL
    }
}

impl Service {
    pub(super) async fn send_events(&self, id: &str, generation: u64) {
        loop {
            let delay = {
                let mut sessions = self.sessions();
                let Some(session) = sessions.get_mut(id).filter(|session| {
                    session.generation == generation && !session.cancel.load(Ordering::Relaxed)
                }) else {
                    return;
                };
                let Some(progress) = &mut session.progress else {
                    return;
                };
                let delay = progress.events.flush(Instant::now());
                if progress.events.sink.is_none()
                    || (progress.events.queue.is_empty()
                        && matches!(progress.state, State::Complete | State::Failed))
                {
                    return;
                }
                delay.min(Duration::from_millis(25))
            };
            tokio::time::sleep(delay).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_preserve_state_changes_coalesce_rows_and_send_at_most_ten_per_second() {
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let sink = received.clone();
        let mut events = Events::new(None);
        events.sink = Some(Arc::new(move |notice| {
            sink.lock().unwrap().push(notice.clone())
        }));
        let start = Instant::now();
        for (state, sequence) in [
            (State::Resolving, 0),
            (State::Listing, 0),
            (State::Enriching, 20_000),
        ] {
            events.queue(Notice {
                id: "test".into(),
                generation: 1,
                sequence,
                state,
            });
        }
        for sequence in 20_001..=40_000 {
            events.queue(Notice {
                id: "test".into(),
                generation: 1,
                sequence,
                state: State::Enriching,
            });
        }
        events.queue(Notice {
            id: "test".into(),
            generation: 1,
            sequence: 40_000,
            state: State::Complete,
        });
        assert_eq!(events.queue.len(), 4);
        events.flush(start);
        for _ in 0..100 {
            events.flush(start + Duration::from_millis(99));
        }
        assert_eq!(received.lock().unwrap().len(), 1);
        for index in 1..=3 {
            events.flush(start + INTERVAL * index);
        }
        let notices = received.lock().unwrap();
        assert_eq!(notices.len(), 4);
        assert_eq!(notices[2].sequence, 40_000);
        assert_eq!(notices[3].state, State::Complete);
    }
}
