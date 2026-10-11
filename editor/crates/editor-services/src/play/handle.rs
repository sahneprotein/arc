use super::*;
use scene_data::Diagnostic;
use std::{
    cell::RefCell,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
        Arc, Mutex, RwLock,
    },
};
const CAPACITY: usize = 32;
pub(crate) struct Shared {
    pub status: RwLock<Arc<PlayStatus>>,
    pub wake: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    pub pending: AtomicUsize,
}
impl Shared {
    pub fn wake(&self) {
        let callback = self.wake.lock().unwrap().clone();
        if let Some(callback) = callback {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| callback()));
        }
    }
    pub fn publish(&self, status: &mut PlayStatus) {
        status.revision += 1;
        *self.status.write().unwrap() = Arc::new(status.clone());
        self.wake();
    }
}
/// UI-thread handle. Snapshot reads never wait: contention returns its last observed
/// revision. No filesystem, process, authoring worker or callback runs under a state lock.
pub struct PlayHandle {
    requests: SyncSender<PlayRequest>,
    events: Receiver<PlayEvent>,
    pub(crate) shared: Arc<Shared>,
    cached: RefCell<Arc<PlayStatus>>,
}
impl PlayHandle {
    pub fn start(config: PlayConfig) -> Result<Self, Diagnostic> {
        Self::with_backend(config, Arc::new(super::process::ExternalBackend::default()))
    }
    pub fn with_backend(
        config: PlayConfig,
        backend: Arc<dyn PlayBackend>,
    ) -> Result<Self, Diagnostic> {
        let initial = Arc::new(PlayStatus::default());
        let shared = Arc::new(Shared {
            status: RwLock::new(initial.clone()),
            wake: Mutex::new(None),
            pending: AtomicUsize::new(0),
        });
        let (tx, rx) = mpsc::sync_channel(CAPACITY);
        // Each admitted start can generate at most an answer and one terminal event.
        let (events, results) = mpsc::sync_channel(CAPACITY * 2 + 1);
        let actor = shared.clone();
        std::thread::Builder::new()
            .name("external-play".into())
            .spawn(move || super::supervisor::run(config, backend, rx, events, actor))
            .map_err(|e| Diagnostic::new("play.supervisor_start_failed").with("error", e))?;
        Ok(Self {
            requests: tx,
            events: results,
            shared,
            cached: RefCell::new(initial),
        })
    }
    pub fn submit(&self, request: PlayRequest) -> Result<(), Diagnostic> {
        if self
            .shared
            .pending
            .try_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < CAPACITY).then_some(n + 1)
            })
            .is_err()
        {
            return Err(Diagnostic::new("play.queue_full"));
        }
        match self.requests.try_send(request) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.shared.pending.fetch_sub(1, Ordering::AcqRel);
                Err(Diagnostic::new(match error {
                    TrySendError::Full(_) => "play.queue_full",
                    TrySendError::Disconnected(_) => "play.coordinator_closed",
                }))
            }
        }
    }
    pub fn poll(&mut self, max: usize) -> Vec<PlayEvent> {
        let events: Vec<_> = self.events.try_iter().take(max).collect();
        for event in &events {
            if matches!(event, PlayEvent::Answer(_)) {
                self.shared.pending.fetch_sub(1, Ordering::AcqRel);
            }
        }
        events
    }
    pub fn snapshot(&self) -> Arc<PlayStatus> {
        if let Ok(status) = self.shared.status.try_read() {
            *self.cached.borrow_mut() = status.clone();
        }
        self.cached.borrow().clone()
    }
    pub fn play_snapshot(&self) -> Arc<PlayStatus> {
        self.snapshot()
    }
    pub fn set_waker(&self, wake: Box<dyn Fn() + Send + Sync + 'static>) {
        *self.shared.wake.lock().unwrap() = Some(Arc::from(wake));
        self.shared.wake();
    }
}
