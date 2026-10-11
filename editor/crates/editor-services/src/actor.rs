use crate::{
    package::cooked_package_identity,
    persistence::{check_cancel, elapsed},
    *,
};
use editor_core::*;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError},
        Arc,
    },
    thread,
    time::Instant,
};

pub use editor_core::{JobFact, JobKind, JobState};
#[derive(Clone, Debug)]
pub enum JobOutput {
    Geometry {
        binding: GeometryBinding,
        simulation: Arc<scene_data::SimulationPayload>,
        visual: Arc<scene_data::VisualPayload>,
        timings: editor_cook::CookTimings,
    },
    Opened {
        timings: OpenTimings,
        disk_hash: ContentHash,
        source_hash: ContentHash,
    },
    Saved(SaveReceipt),
    Cooked {
        receipt: VerifiedPackageReceipt,
        path: PathBuf,
        source_hash: ContentHash,
        simulation_hash: ContentHash,
        timings: editor_cook::CookTimings,
        publication_us: u64,
    },
    PackageVerified {
        receipt: VerifiedPackageReceipt,
        verification_us: u64,
    },
}
#[derive(Clone, Debug)]
pub enum ServiceEvent {
    Command(CommandResult),
    JobStarted(JobFact),
    JobFinished {
        fact: JobFact,
        result: Result<JobOutput, Diagnostic>,
    },
    Snapshot(Arc<ViewSnapshot>),
}
pub enum ServiceRequest {
    Command(CommandRequest),
    Job {
        id: u64,
        session: u64,
        revision: u64,
        operation: JobOperation,
        cancel: Arc<AtomicBool>,
    },
}
pub enum JobOperation {
    Geometry,
    Open(PathBuf),
    Save {
        path: PathBuf,
        expected: DiskVersion,
    },
    Cook {
        package_root: PathBuf,
        /// The game's folder, which the emitters a level places are read from. A level
        /// that places one is not cooked without it.
        game: Option<PathBuf>,
    },
    VerifyPackage {
        path: PathBuf,
        expected: Option<PackageIdentity>,
    },
}
impl JobOperation {
    fn kind(&self) -> JobKind {
        match self {
            Self::Geometry => JobKind::Geometry,
            Self::Open(_) => JobKind::Open,
            Self::Save { .. } => JobKind::Save,
            Self::Cook { .. } => JobKind::Cook,
            Self::VerifyPackage { .. } => JobKind::VerifyPackage,
        }
    }
}
#[derive(Clone)]
pub struct JobTicket {
    pub id: u64,
    cancelled: Arc<AtomicBool>,
}
impl JobTicket {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

/// UI-thread endpoint. `snapshot` clones cached data; `submit` and `poll` never wait.
pub struct EditorHandle {
    sender: Option<SyncSender<ServiceRequest>>,
    receiver: Receiver<ServiceEvent>,
    cached: Arc<ViewSnapshot>,
    next_job: u64,
    jobs: VecDeque<JobFact>,
    cancellation: Vec<(u64, Arc<AtomicBool>)>,
    latest_geometry: Option<Arc<AtomicBool>>,
}
impl EditorHandle {
    pub fn start(document: SceneDocument) -> Result<Self, Diagnostic> {
        let session = EditorSession::new(document)?;
        let cached = session.snapshot();
        let (sender, requests) = mpsc::sync_channel(64);
        let (events, receiver) = mpsc::sync_channel(128);
        thread::Builder::new()
            .name("arc-editor-core".into())
            .spawn(move || worker(session, requests, events))
            .map_err(|e| Diagnostic::new("worker.start").with("detail", e))?;
        Ok(Self {
            sender: Some(sender),
            receiver,
            cached,
            next_job: 1,
            jobs: VecDeque::new(),
            cancellation: Vec::new(),
            latest_geometry: None,
        })
    }
    pub fn snapshot(&self) -> Arc<ViewSnapshot> {
        self.cached.clone()
    }
    pub fn jobs(&self) -> &VecDeque<JobFact> {
        &self.jobs
    }
    pub fn submit(&self, request: CommandRequest) -> Result<(), Diagnostic> {
        self.send(ServiceRequest::Command(request))
    }
    pub fn job(&mut self, operation: JobOperation) -> Result<JobTicket, Diagnostic> {
        self.cancellation.retain(|(_, c)| Arc::strong_count(c) > 1);
        let id = self.next_job;
        self.next_job += 1;
        let cancel = Arc::new(AtomicBool::new(false));
        let geometry = matches!(operation, JobOperation::Geometry);
        self.send(ServiceRequest::Job {
            id,
            session: self.cached.session,
            revision: self.cached.document_revision,
            operation,
            cancel: cancel.clone(),
        })?;
        // Supersede ONLY after successful enqueue: queue.full must not cancel the
        // last usable request. Every accepted ticket still follows normal terminal events.
        if geometry && let Some(previous) = self.latest_geometry.replace(cancel.clone()) {
            previous.store(true, Ordering::Release);
        }
        self.cancellation.push((id, cancel.clone()));
        Ok(JobTicket {
            id,
            cancelled: cancel,
        })
    }
    fn send(&self, request: ServiceRequest) -> Result<(), Diagnostic> {
        self.sender
            .as_ref()
            .ok_or_else(|| Diagnostic::new("worker.stopped"))?
            .try_send(request)
            .map_err(|e| match e {
                TrySendError::Full(_) => Diagnostic::new("queue.full"),
                TrySendError::Disconnected(_) => Diagnostic::new("worker.stopped"),
            })
    }
    /// Bounded work per UI frame; command/job errors remain explicit events.
    pub fn poll(&mut self, max_events: usize) -> Vec<ServiceEvent> {
        let mut result = Vec::new();
        for _ in 0..max_events {
            let event = match self.receiver.try_recv() {
                Ok(e) => e,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            };
            match &event {
                ServiceEvent::Snapshot(snapshot) => self.cached = snapshot.clone(),
                ServiceEvent::JobStarted(fact) | ServiceEvent::JobFinished { fact, .. } => {
                    if matches!(event, ServiceEvent::JobFinished { .. }) {
                        self.cancellation.retain(|(id, _)| *id != fact.id);
                    }
                    self.jobs.retain(|j| j.id != fact.id);
                    self.jobs.push_back(fact.clone());
                    while self.jobs.len() > 32 {
                        self.jobs.pop_front();
                    }
                }
                _ => {}
            }
            result.push(event);
        }
        result
    }
}
impl Drop for EditorHandle {
    fn drop(&mut self) {
        for (_, c) in &self.cancellation {
            c.store(true, Ordering::Release);
        }
        self.sender.take();
    }
}
fn worker(
    mut session: EditorSession,
    requests: Receiver<ServiceRequest>,
    events: SyncSender<ServiceEvent>,
) {
    let mut geometry = editor_cook::GeometryCache::default();
    while let Ok(request) = requests.recv() {
        match request {
            ServiceRequest::Command(command) => {
                if events
                    .send(ServiceEvent::Command(session.submit(command)))
                    .is_err()
                {
                    return;
                }
            }
            ServiceRequest::Job {
                id,
                session: expected_session,
                revision,
                operation,
                cancel,
            } => {
                let kind = operation.kind();
                let start = Instant::now();
                let running = JobFact {
                    id,
                    kind,
                    state: JobState::Running,
                    elapsed_us: None,
                };
                session.set_job(running.clone());
                if events.send(ServiceEvent::JobStarted(running)).is_err()
                    || events
                        .send(ServiceEvent::Snapshot(session.snapshot()))
                        .is_err()
                {
                    return;
                }
                let view = session.snapshot();
                let result = if view.session != expected_session
                    || view.document_revision != revision
                {
                    Err(Diagnostic::new("job.stale"))
                } else if view.preview.is_some() && !matches!(operation, JobOperation::Geometry) {
                    Err(Diagnostic::new("transaction.busy"))
                } else {
                    run_job(&mut session, operation, &cancel, &mut geometry)
                };
                let state = match &result {
                    Ok(_) => JobState::Complete,
                    Err(e) if e.code == "job.cancelled" => JobState::Cancelled,
                    Err(e) => JobState::Failed(e.clone()),
                };
                let fact = JobFact {
                    id,
                    kind,
                    state,
                    elapsed_us: Some(elapsed(start)),
                };
                session.set_job(fact.clone());
                if events
                    .send(ServiceEvent::JobFinished { fact, result })
                    .is_err()
                {
                    return;
                }
            }
        }
        if events
            .send(ServiceEvent::Snapshot(session.snapshot()))
            .is_err()
        {
            return;
        }
    }
}
fn run_job(
    session: &mut EditorSession,
    operation: JobOperation,
    cancel: &AtomicBool,
    geometry: &mut editor_cook::GeometryCache,
) -> Result<JobOutput, Diagnostic> {
    check_cancel(cancel)?;
    match operation {
        JobOperation::Geometry => {
            let view = session.snapshot();
            let document = view
                .preview
                .as_ref()
                .map(|p| &p.document)
                .unwrap_or(&view.document);
            let package = geometry.cook(document.clone(), &|| check_cancel(cancel))?;
            Ok(JobOutput::Geometry {
                binding: GeometryBinding::from_snapshot(&view),
                simulation: package.simulation,
                visual: package.visual,
                timings: package.timings,
            })
        }
        JobOperation::Open(path) => {
            let loaded = load_document(&path, cancel)?;
            let mut next = EditorSession::new(loaded.document)?;
            next.mark_saved(loaded.source_hash);
            next.set_timings(Some(loaded.timings.total_us), None, None);
            *session = next;
            geometry.clear();
            Ok(JobOutput::Opened {
                timings: loaded.timings,
                disk_hash: loaded.disk_hash,
                source_hash: loaded.source_hash,
            })
        }
        JobOperation::Save { path, expected } => {
            let receipt = save_document(&path, &session.snapshot().document, expected, cancel)?;
            session.mark_saved(receipt.source_hash);
            Ok(JobOutput::Saved(receipt))
        }
        JobOperation::Cook { package_root, game } => {
            // Each placed emitter's file is read now, and it is those exact bytes whose
            // hash goes into the package: not what anything said of the file earlier.
            let emitters =
                |emitter_id: &str| crate::emitters::cooked_emitter(game.as_deref(), emitter_id);
            let package =
                editor_cook::cook_with_emitters(&session.snapshot().document, &emitters, &|| {
                    check_cancel(cancel)
                })?;
            check_cancel(cancel)?;
            let publication = Instant::now();
            let expected = cooked_package_identity(&package)?;
            let path = publish_package(&package_root, &package, cancel)?;
            let receipt = verify_package(&path, Some(&expected), cancel)?;
            let publication_us = elapsed(publication);
            session.set_timings(
                None,
                Some(package.timings.total_us),
                Some(package.simulation_hash),
            );
            Ok(JobOutput::Cooked {
                path: receipt.path().to_path_buf(),
                source_hash: receipt.identity().source_hash,
                simulation_hash: receipt.identity().simulation_hash,
                receipt,
                timings: package.timings,
                publication_us,
            })
        }
        JobOperation::VerifyPackage { path, expected } => {
            let start = Instant::now();
            let receipt = verify_package(&path, expected.as_ref(), cancel)?;
            Ok(JobOutput::PackageVerified {
                receipt,
                verification_us: elapsed(start),
            })
        }
    }
}

#[cfg(test)]
#[path = "actor_tests.rs"]
mod tests;
