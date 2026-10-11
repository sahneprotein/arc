mod commands;
pub(super) mod dirty;
mod history;
use crate::{
    command::mutate,
    view::{hierarchy, inspector},
    *,
};
use scene_data::{terrain::SampleKey, ObjectData};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);
const HISTORY_LIMIT: usize = 128;
const HISTORY_BYTES: usize = 64 * 1024 * 1024;
const RESULT_LIMIT: usize = 256;
struct History {
    before: Arc<SceneDocument>,
    after: Arc<SceneDocument>,
    label: String,
    bytes: usize,
}
struct Transaction {
    id: u64,
    sequence: u64,
    base: Arc<SceneDocument>,
    preview: Arc<SceneDocument>,
    dirty: Arc<[TerrainDirty]>,
}

/// Own this on a service worker. Views contain no locks or references into mutable state.
pub struct EditorSession {
    id: u64,
    revision: u64,
    view_revision: u64,
    regions: RegionRevisions,
    document: Arc<SceneDocument>,
    hash: ContentHash,
    saved: Option<ContentHash>,
    selection: Arc<[ObjectId]>,
    active: Option<ObjectId>,
    pivot: GizmoPivot,
    snap: SnapSettings,
    emitters: Arc<EmitterAssets>,
    review: Arc<[ReviewProblem]>,
    hierarchy: Arc<[HierarchyRow]>,
    inspector: Arc<[FieldDescriptor]>,
    changed: Arc<[ObjectId]>,
    undo: VecDeque<History>,
    redo: VecDeque<History>,
    transaction: Option<Transaction>,
    next_transaction: u64,
    results: VecDeque<(CommandRequest, CommandResult)>,
    status: StatusFacts,
    jobs: Arc<[JobFact]>,
}
impl EditorSession {
    pub fn new(document: SceneDocument) -> Result<Self, Diagnostic> {
        let hash = scene_data::source_hash(&document)?;
        Ok(Self {
            id: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            revision: 0,
            view_revision: 0,
            regions: RegionRevisions::default(),
            hierarchy: hierarchy(&document),
            review: review_document(&document).into(),
            document: Arc::new(document),
            hash,
            saved: None,
            selection: Arc::from([]),
            active: None,
            pivot: GizmoPivot::Active,
            snap: SnapSettings::default(),
            emitters: Arc::default(),
            inspector: Arc::from([]),
            changed: Arc::from([]),
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            transaction: None,
            next_transaction: 1,
            results: VecDeque::new(),
            jobs: Arc::from([]),
            status: StatusFacts {
                cook_needed: true,
                ..Default::default()
            },
        })
    }
    pub fn snapshot(&self) -> Arc<ViewSnapshot> {
        let mut status = self.status.clone();
        status.dirty = self.saved != Some(self.hash);
        status.saved_source_hash = self.saved;
        Arc::new(ViewSnapshot {
            session: self.id,
            document_revision: self.revision,
            view_revision: self.view_revision,
            regions: self.regions,
            document: self.document.clone(),
            hierarchy: self.hierarchy.clone(),
            selection: self.selection.clone(),
            active: self.active.clone(),
            pivot: self.pivot,
            snap: self.snap,
            review: self.review.clone(),
            inspector: self.inspector.clone(),
            changed_objects: self.changed.clone(),
            jobs: self.jobs.clone(),
            status,
            undo_label: self.undo.back().map(|h| h.label.clone()),
            redo_label: self.redo.back().map(|h| h.label.clone()),
            preview: self.transaction.as_ref().map(|t| PreviewSnapshot {
                transaction: t.id,
                sequence: t.sequence,
                document: t.preview.clone(),
                dirty: t.dirty.clone(),
            }),
        })
    }
    pub fn set_job(&mut self, fact: JobFact) {
        let mut jobs: Vec<_> = self
            .jobs
            .iter()
            .filter(|j| j.id != fact.id)
            .cloned()
            .collect();
        jobs.push(fact);
        if jobs.len() > 32 {
            jobs.remove(0);
        }
        self.jobs = jobs.into();
        self.regions.jobs += 1;
        self.view_revision += 1;
    }
    pub fn source_hash(&self) -> ContentHash {
        self.hash
    }
    /// A save completion for an older revision still records the exact bytes saved.
    pub fn mark_saved(&mut self, hash: ContentHash) {
        self.saved = Some(hash);
        self.regions.status += 1;
        self.view_revision += 1;
    }
    pub fn set_timings(
        &mut self,
        open_us: Option<u64>,
        cook_us: Option<u64>,
        simulation_hash: Option<ContentHash>,
    ) {
        if open_us.is_some() {
            self.status.open_us = open_us;
        }
        if cook_us.is_some() {
            self.status.cook_us = cook_us;
            self.status.simulation_hash = simulation_hash;
            self.status.cook_needed = false;
        }
        self.regions.status += 1;
        self.view_revision += 1;
    }
    pub fn submit(&mut self, request: CommandRequest) -> CommandResult {
        if let Some((previous, result)) = self
            .results
            .iter()
            .find(|(r, _)| r.request == request.request)
        {
            return if previous == &request {
                result.clone()
            } else {
                CommandResult {
                    request: request.request,
                    revision: self.revision,
                    outcome: Err(Diagnostic::new("request.reused")),
                }
            };
        }
        let outcome = if request.session != self.id {
            Err(Diagnostic::new("session.stale"))
        } else if request.expected_revision != self.revision {
            Err(Diagnostic::new("revision.stale").with("current", self.revision))
        } else if request
            .expected_inspector_revision
            .is_some_and(|r| r != self.regions.inspector)
        {
            Err(Diagnostic::new("inspector.stale"))
        } else if matches!(request.command, Command::AssignField { .. })
            && request.expected_inspector_revision.is_none()
        {
            Err(Diagnostic::new("inspector.revision_required"))
        } else {
            self.apply(&request.command)
        };
        let result = CommandResult {
            request: request.request,
            revision: self.revision,
            outcome,
        };
        self.results.push_back((request, result.clone()));
        if self.results.len() > RESULT_LIMIT {
            self.results.pop_front();
        }
        result
    }
}
