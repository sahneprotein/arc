//! The UI's end of the editor core.
//!
//! The core owns the document on a worker thread. The UI holds an [`Editor`]: it reads
//! immutable snapshots from it and sends commands, each checked against the revision the UI
//! last saw. Nothing here waits for the worker.

use std::{path::PathBuf, sync::Arc};

use bevy::{prelude::*, window::RequestRedraw};
use editor_core::GeometryBinding;
use editor_core::{
    Command, CommandOutcome, CommandRequest, Diagnostic, Mutation, ObjectId, SceneDocument,
    TerrainPaintRequest, TerrainSculptRequest, TerrainSurfaceRay, TransactionId, ViewSnapshot,
};
use editor_services::{
    DiskVersion, EditorHandle, JobKind, JobOperation, JobOutput, JobTicket, PlayConfig,
    PlayControl, PlayEnd, PlayEvent, PlayHandle, PlayMode, PlayRequest, PlayState, PlayStatus,
    RunId, ServiceEvent, VerifiedPackageReceipt,
};
use scene_data::{
    terrain::{LayerDefinition, LayerKind, RuleDefinition, TerrainDocument, TileKey},
    ObjectData, ObjectRecord, RigidTransform, SimulationPayload, StableId, VisualPayload,
};

use crate::{rules, tool::PlaceAt};

/// Events of the core handled per frame at most. More wait for the next frame.
const EVENTS_PER_FRAME: usize = 64;

/// The open level and the way to change it. Lives on the main thread.
pub struct Editor {
    handle: EditorHandle,
    next_request: u64,
    /// Commands sent and not yet answered.
    unanswered: u32,
    /// Jobs started and not yet finished.
    running_jobs: u32,
    /// The last problem a command or a job reported. It stays until the same kind of action
    /// succeeds: a finished cook does not make a failed save disappear.
    pub problem: Option<Diagnostic>,
    problem_from: Option<Source>,
    /// Where Save writes the level.
    pub level_path: PathBuf,
    /// Where Cook publishes the cooked level.
    pub package_root: PathBuf,
    /// What Save expects to find on disk, so that it never overwrites a file it has not seen.
    on_disk: DiskVersion,
    /// The drag in the 3D view that is under way, if any.
    drag: Option<ViewDrag>,
    /// The file an Open or a Save is writing to or reading from, adopted only if it succeeds.
    pending_path: Option<PathBuf>,
    /// Whether a level file is open or was saved. Without one the level is new.
    has_file: bool,
    /// The other file the last save was meant for, so that trying again saves there.
    save_as_target: Option<PathBuf>,
    /// What the 3D view was last asked to be drawn from, and the answer when it has come.
    geometry_asked: Option<GeometryBinding>,
    geometry: Option<Geometry>,
    next_id: u64,
    /// What the last finished cook was made from and what it made: the hash of the level's
    /// source and the hash of the cooked simulation data (what can be walked on).
    pub last_cook: Option<(String, String)>,
    /// A new object to select as soon as the core has it.
    select_when_there: Option<ObjectId>,
    /// The request that commits a drag and the transaction it commits, until it is answered.
    committing: Option<(u64, TransactionId)>,
    /// A request the editor sent to tidy up after a refusal. Its answer is not news: it must
    /// not make the refusal's notice disappear.
    silent: Option<u64>,
    /// The cooked level the last finished cook published, as the core verified it then.
    cooked: Option<VerifiedPackageReceipt>,
    /// What is known of that package as it is on disk now.
    package_check: PackageCheck,
    /// The check that is under way, to cancel it.
    check_ticket: Option<JobTicket>,
    /// The core's Play coordinator, if this editor has a game to start. It runs beside the
    /// editing core: a game that runs is not work the editor waits for.
    play: Option<PlayHandle>,
    next_play_request: u64,
    /// The way to play that was chosen and waits for the check of the cooked level to pass.
    play_when_checked: Option<PlayMode>,
    /// A start that was sent and not yet answered.
    start_unanswered: bool,
    /// The last thing the coordinator refused, until the next request.
    play_refusal: Option<Diagnostic>,
    /// How the last run ended, and which of its notices were put away.
    last_end: Option<PlayEnd>,
    end_dismissed: bool,
    cleanup_dismissed: bool,
}

/// Whether the cooked level on disk is still the one this editor cooked. Asked for by the
/// person, answered by the core's `VerifyPackage` job; a new cook starts from "not asked".
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum PackageCheck {
    /// Nobody asked since the last cook, or the check was cancelled.
    #[default]
    NotAsked,
    /// The core is reading the package.
    Checking,
    /// The package on disk is the one this editor cooked. How long reading it took, in
    /// microseconds.
    Passed { micros: u64 },
    /// The core refused the package, with this code. The problem itself is the editor's
    /// [`Editor::problem`] until it is dismissed; this stays.
    Refused { code: String },
}

/// The level as the cook step sees it, for the 3D view: the same geometry the game gets.
pub struct Geometry {
    /// Which state of the level this was made from.
    pub binding: GeometryBinding,
    pub simulation: Arc<SimulationPayload>,
    pub visual: Arc<VisualPayload>,
}

/// What a drag has reached: new values of objects, or a brush stroke on a landscape.
enum Preview {
    Objects(Vec<Mutation>),
    Sculpt(TerrainSculptRequest),
    Paint(TerrainPaintRequest),
}

/// A drag in the 3D view: one transaction of the core, which becomes one undo step.
struct ViewDrag {
    /// The core's name for the transaction, once it has answered the request to begin.
    transaction: Option<TransactionId>,
    sequence: u64,
    /// The latest values, sent as soon as the transaction is known.
    latest: Option<Preview>,
    /// How the drag ended while the core had not yet answered: commit (`true`) or cancel.
    ended: Option<bool>,
    /// What the undo step is called when the drag is committed: a key of the wording table.
    label: &'static str,
}

impl Editor {
    /// Starts the core with `document` as the open level, not yet saved anywhere.
    pub fn new(
        document: SceneDocument,
        level_path: PathBuf,
        package_root: PathBuf,
    ) -> Result<Self, Diagnostic> {
        Ok(Self {
            handle: EditorHandle::start(document)?,
            next_request: 1,
            unanswered: 0,
            running_jobs: 0,
            problem: None,
            problem_from: None,
            level_path,
            package_root,
            on_disk: DiskVersion::Absent,
            drag: None,
            pending_path: None,
            has_file: false,
            save_as_target: None,
            geometry_asked: None,
            geometry: None,
            next_id: 0,
            last_cook: None,
            select_when_there: None,
            committing: None,
            silent: None,
            cooked: None,
            package_check: PackageCheck::NotAsked,
            check_ticket: None,
            play: None,
            next_play_request: 1,
            play_when_checked: None,
            start_unanswered: false,
            play_refusal: None,
            last_end: None,
            end_dismissed: false,
            cleanup_dismissed: false,
        })
    }

    /// The latest state of the level. Cheap: a shared pointer to immutable data.
    pub fn snapshot(&self) -> Arc<ViewSnapshot> {
        self.handle.snapshot()
    }

    /// Whether the core still owes an answer or a job result.
    pub fn busy(&self) -> bool {
        self.unanswered > 0 || self.running_jobs > 0
    }

    /// Sends a command, checked against the revision this UI last saw.
    pub fn submit(&mut self, command: Command) {
        let _ = self.send(command, None);
    }

    /// Sends an edit made in the inspector, also checked against the inspector's revision.
    pub fn submit_from_inspector(&mut self, command: Command) {
        let inspector = self.snapshot().regions.inspector;
        let _ = self.send(command, Some(inspector));
    }

    /// Sends a command and returns the number its answer will carry.
    fn send(&mut self, command: Command, expected_inspector_revision: Option<u64>) -> u64 {
        let snapshot = self.snapshot();
        let number = self.next_request;
        let request = CommandRequest {
            request: number,
            session: snapshot.session,
            expected_revision: snapshot.document_revision,
            expected_inspector_revision,
            command,
        };
        self.next_request += 1;
        debug!(
            "command {} at revision {}: {}",
            request.request,
            request.expected_revision,
            command_name(&request.command)
        );
        match self.handle.submit(request) {
            Ok(()) => self.unanswered += 1,
            Err(problem) => self.report(Source::Command, Err(problem)),
        }
        number
    }

    /// Gives up a transaction the core still holds after it refused something in it. Without
    /// this the core would refuse every later command as busy.
    fn release(&mut self, transaction: TransactionId) {
        self.silent = Some(self.send(Command::CancelViewportEdit { transaction }, None));
    }

    /// Records the outcome of an action: a failure is shown, a success clears the problem of
    /// the same kind of action and leaves any other.
    fn report(&mut self, source: Source, outcome: Result<(), Diagnostic>) {
        match outcome {
            Err(problem) => {
                self.problem = Some(problem);
                self.problem_from = Some(source);
            }
            Ok(()) if self.problem_from == Some(source) => {
                self.problem = None;
                self.problem_from = None;
            }
            Ok(()) => {}
        }
    }

    /// Begins a drag in the 3D view on the current selection. Does nothing if one is under way.
    pub fn begin_drag(&mut self) {
        self.begin_stroke("drag");
    }

    /// Begins a drag whose undo step is named by `label`: "sculpt" or "paint" for a stroke of
    /// the brush. Does nothing if a drag is under way.
    pub fn begin_stroke(&mut self, label: &'static str) {
        if self.drag.is_some() {
            return;
        }
        let selection_revision = self.snapshot().regions.selection;
        self.drag = Some(ViewDrag {
            transaction: None,
            sequence: 0,
            latest: None,
            ended: None,
            label,
        });
        self.submit(Command::BeginViewportEdit { selection_revision });
    }

    /// Whether a drag in the 3D view is under way.
    pub fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Shows the values the drag has reached. They replace the previous preview; the level
    /// itself changes only when the drag is committed.
    pub fn preview_drag(&mut self, mutations: Vec<Mutation>) {
        self.preview(Preview::Objects(mutations));
    }

    /// Shows the brush stroke so far. The request holds the whole stroke; the core works the
    /// ground out afresh from where the stroke began, so the result does not depend on how
    /// often this is called.
    pub fn preview_sculpt(&mut self, stroke: TerrainSculptRequest) {
        self.preview(Preview::Sculpt(stroke));
    }

    /// Shows the paint stroke so far, on the same terms as a sculpt stroke: the whole stroke
    /// each time, worked out by the core from the level as it was when the stroke began.
    pub fn preview_paint(&mut self, stroke: TerrainPaintRequest) {
        self.preview(Preview::Paint(stroke));
    }

    fn preview(&mut self, preview: Preview) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        let Some(transaction) = drag.transaction else {
            // The core has not named the transaction yet: keep only the latest values.
            drag.latest = Some(preview);
            return;
        };
        drag.sequence += 1;
        let sequence = drag.sequence;
        self.submit(match preview {
            Preview::Objects(mutations) => Command::Preview {
                transaction,
                sequence,
                mutations,
            },
            Preview::Sculpt(request) => Command::PreviewTerrainSculpt {
                transaction,
                sequence,
                request,
            },
            Preview::Paint(request) => Command::PreviewTerrainPaint {
                transaction,
                sequence,
                request,
            },
        });
    }

    /// Ends the drag: `commit` makes it one undo step, otherwise the level stays as it was.
    pub fn end_drag(&mut self, commit: bool) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        let Some(transaction) = drag.transaction else {
            drag.ended = Some(commit);
            return;
        };
        let label_key = drag.label.to_owned();
        self.drag = None;
        if commit {
            let command = Command::CommitViewportEdit {
                transaction,
                label_key,
            };
            self.committing = Some((self.send(command, None), transaction));
        } else {
            self.submit(Command::CancelViewportEdit { transaction });
        }
    }

    /// The core has named the transaction: send what the drag did in the meantime.
    fn drag_started(&mut self, transaction: TransactionId) {
        let Some(drag) = &mut self.drag else {
            // The drag was given up before the core named its transaction.
            self.release(transaction);
            return;
        };
        drag.transaction = Some(transaction);
        let latest = drag.latest.take();
        let ended = drag.ended.take();
        if let Some(preview) = latest {
            self.preview(preview);
        }
        if let Some(commit) = ended {
            self.end_drag(commit);
        }
    }

    /// A new stable ID. The time in nanoseconds mixed with a counter: unique in this editor,
    /// and unlikely to meet an ID made elsewhere.
    fn fresh_id(&mut self) -> ObjectId {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        self.next_id += 1;
        ObjectId::from_u128(id_bits(nanos, self.next_id))
    }

    /// The half size of a new box, in metres: a cube of one metre.
    pub const NEW_BOX_HALF_SIZE: f32 = 0.5;

    /// How far above the place it is put a new spawn point is, in metres, as the spawn
    /// points of the game's map are above its ground.
    pub const NEW_SPAWN_HEIGHT: f32 = 2.0;

    /// How far along the view's line of sight a landscape is looked for, in metres.
    const PLACE_REACH_M: f64 = 10_000.0;

    /// The point a new object stands on: where the view's line of sight first meets a
    /// landscape, by the core's own account of the ground's surface, or the fallback point
    /// if it meets none. `None` if the core could not say; that is reported, and nothing is
    /// added.
    pub fn place_point(&mut self, place: &PlaceAt) -> Option<Vec3> {
        let snapshot = self.snapshot();
        let ray = TerrainSurfaceRay {
            origin_world_m: place.origin.to_array(),
            direction_world: place.direction.to_array(),
            max_distance_m: Self::PLACE_REACH_M,
        };
        // A line of sight that is not one, before the view exists for instance.
        if !place.direction.is_finite() || place.direction.length_squared() == 0.0 {
            return Some(place.fallback);
        }
        let mut nearest: Option<(f64, [f32; 3])> = None;
        for object in &snapshot.document.objects {
            if !matches!(object.data, ObjectData::Terrain(_)) {
                continue;
            }
            match editor_core::terrain_surface_hit(&snapshot.document, &object.id, &ray) {
                Ok(Some(hit)) if nearest.is_none_or(|(distance, _)| hit.distance_m < distance) => {
                    nearest = Some((hit.distance_m, hit.position_world_m));
                }
                Ok(_) => {}
                Err(problem) => {
                    self.report(Source::Command, Err(problem));
                    return None;
                }
            }
        }
        Some(nearest.map_or(place.fallback, |(_, position)| Vec3::from_array(position)))
    }

    /// Adds a box that stands where the view looks, and selects it. One undo step.
    pub fn add_box(&mut self, place: &PlaceAt) {
        let Some(place) = self.place_point(place) else {
            return;
        };
        let half = Self::NEW_BOX_HALF_SIZE;
        let count = self.count(|data| matches!(data, ObjectData::Box { .. }));
        self.add_object(
            format!("Box {}", count + 1),
            place + Vec3::Y * half,
            ObjectData::Box {
                half_extents_m: [half; 3],
            },
        );
    }

    /// Adds a spawn point above `place`, after the last one in order, and selects it. One
    /// undo step.
    pub fn add_spawn(&mut self, place: &PlaceAt) {
        let Some(place) = self.place_point(place) else {
            return;
        };
        let order = self
            .snapshot()
            .document
            .objects
            .iter()
            .filter_map(|object| match object.data {
                ObjectData::Spawn { order } => Some(order + 1),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        self.add_object(
            format!("Spawn {}", order + 1),
            place + Vec3::Y * Self::NEW_SPAWN_HEIGHT,
            ObjectData::Spawn { order },
        );
    }

    fn count(&self, kind: impl Fn(&ObjectData) -> bool) -> usize {
        let snapshot = self.snapshot();
        snapshot
            .document
            .objects
            .iter()
            .filter(|object| kind(&object.data))
            .count()
    }

    fn add_object(&mut self, name: String, at: Vec3, data: ObjectData) {
        let order = self.snapshot().document.objects.len() as u32;
        let id = self.fresh_id();
        let object = ObjectRecord {
            id: id.clone(),
            parent: None,
            sibling_order: order,
            name,
            local: RigidTransform {
                translation_m: at.to_array(),
                ..RigidTransform::default()
            },
            data,
        };
        self.select_when_there = Some(id);
        self.submit(Command::Edit {
            label_key: "create".into(),
            mutations: vec![Mutation::Create(object)],
        });
    }

    /// Removes the selected objects. One undo step; nothing happens without a selection.
    pub fn delete_selected(&mut self) {
        let mutations: Vec<Mutation> = self
            .snapshot()
            .selection
            .iter()
            .cloned()
            .map(Mutation::Delete)
            .collect();
        if !mutations.is_empty() {
            self.submit(Command::Edit {
                label_key: "delete".into(),
                mutations,
            });
        }
    }

    /// Selects the object that was just added, once the core has it and owes no answer, so
    /// that the selection is checked against the level the object is in.
    fn select_what_was_added(&mut self) {
        if self.unanswered > 0 {
            return;
        }
        if let Some(id) = self.select_when_there.take()
            && self.snapshot().document.object(&id).is_some()
        {
            self.select(vec![id]);
        }
    }

    /// Adds a flat landscape of two by two tiles around the middle of the level, each of
    /// `cells` cells of `spacing` metres on a side. The landscape is then in the hierarchy
    /// like any other object.
    pub fn add_landscape(&mut self, cells: u32, spacing: f32) {
        let tiles = [(-1, -1), (0, -1), (-1, 0), (0, 0)]
            .map(|(x, z)| (self.fresh_id(), TileKey { x, z }))
            .to_vec();
        let terrain = match TerrainDocument::flat(cells, [spacing, spacing], tiles) {
            Ok(terrain) => terrain,
            Err(problem) => {
                self.report(Source::Command, Err(problem));
                return;
            }
        };
        let order = self.snapshot().document.objects.len() as u32;
        let object = ObjectRecord {
            id: self.fresh_id(),
            parent: None,
            sibling_order: order,
            name: "Landscape".into(),
            local: RigidTransform::default(),
            data: ObjectData::Terrain(terrain),
        };
        self.submit(Command::Edit {
            label_key: "create".into(),
            mutations: vec![Mutation::Create(object)],
        });
    }

    /// Adds a layer to the landscape, named and coloured after the next entry of
    /// [`LAYER_STARTS`]. The first layer covers the whole ground; later ones start empty.
    /// One undo step. Returns the new layer's ID, or `None` without a landscape.
    pub fn add_layer(&mut self) -> Option<String> {
        let snapshot = self.snapshot();
        let (object, terrain) = landscape(&snapshot)?;
        let group = terrain
            .surface_group
            .clone()
            .unwrap_or_else(|| self.fresh_id());
        let surface_layers = terrain
            .layers
            .iter()
            .filter(|layer| matches!(layer.kind, LayerKind::BlendWeight { .. }))
            .count();
        let (name, color) = LAYER_STARTS.get(terrain.layers.len()).map_or_else(
            || {
                (
                    format!("Layer {}", terrain.layers.len() + 1),
                    [0.5, 0.5, 0.5, 1.0],
                )
            },
            |(name, color)| ((*name).to_owned(), *color),
        );
        let id = self.fresh_id();
        let mut layers = terrain.layers.clone();
        layers.push(LayerDefinition {
            id: id.clone(),
            name,
            kind: LayerKind::BlendWeight {
                group: group.clone(),
                default_weight: if surface_layers == 0 { 1.0 } else { 0.0 },
            },
            color_linear_rgba: Some(color),
        });
        self.set_layers(object.id.clone(), terrain, layers, Some(group));
        Some(id.0)
    }

    /// Removes a layer from the landscape. If it was the one new ground starts with, the
    /// first layer left takes that over, since one layer must. One undo step.
    pub fn remove_layer(&mut self, layer: &str) {
        let snapshot = self.snapshot();
        let Some((object, terrain)) = landscape(&snapshot) else {
            return;
        };
        let mut layers: Vec<LayerDefinition> = terrain
            .layers
            .iter()
            .filter(|definition| definition.id.0 != layer)
            .cloned()
            .collect();
        if layers.len() == terrain.layers.len() {
            return;
        }
        let starts_somewhere = layers.iter().any(|definition| {
            matches!(definition.kind, LayerKind::BlendWeight { default_weight, .. } if default_weight > 0.0)
        });
        if !starts_somewhere
            && let Some(LayerKind::BlendWeight { default_weight, .. }) = layers
                .iter_mut()
                .map(|definition| &mut definition.kind)
                .find(|kind| matches!(kind, LayerKind::BlendWeight { .. }))
        {
            *default_weight = 1.0;
        }
        let group = layers
            .iter()
            .any(|definition| matches!(definition.kind, LayerKind::BlendWeight { .. }))
            .then(|| terrain.surface_group.clone())
            .flatten();
        // A rule that fills the layer goes with it, and no rule reads it any more: the core
        // takes the layer and the rules together or not at all.
        let rules = rules::without_layer(&terrain.rules, &StableId(layer.to_owned()));
        let mut mutations = vec![Self::registry(object.id.clone(), terrain, layers, group)];
        if rules != terrain.rules {
            mutations.push(Mutation::TerrainRules {
                object: object.id.clone(),
                rules,
            });
        }
        self.submit(Command::Edit {
            label_key: "layers".into(),
            mutations,
        });
    }

    /// Adds a rule that scatters grass on the landscape's first layer, or everywhere if it
    /// has no layers. One undo step. Returns the rule's ID, or `None` without a landscape.
    pub fn add_grass_rule(&mut self) -> Option<String> {
        let snapshot = self.snapshot();
        let (object, terrain) = landscape(&snapshot)?;
        let id = self.fresh_id();
        let grows_on = terrain.layers.first().map(|layer| layer.id.clone());
        let mut rules = terrain.rules.clone();
        rules.push(rules::new_grass(id.clone(), seed_of(&id), grows_on));
        self.set_rules(object.id.clone(), rules);
        Some(id.0)
    }

    /// Adds a rule that fills a layer on steep ground, and the layer it fills, together as one
    /// undo step: a rule's layer shows what the rule makes, so it gets one of its own rather
    /// than taking over one that is painted. A landscape without layers also gets the layer
    /// its ground starts with. Returns the rule's ID, or `None` without a landscape.
    pub fn add_slope_rule(&mut self) -> Option<String> {
        let snapshot = self.snapshot();
        let (object, terrain) = landscape(&snapshot)?;
        let group = terrain
            .surface_group
            .clone()
            .unwrap_or_else(|| self.fresh_id());
        let mut layers = terrain.layers.clone();
        let blend = |group: &ObjectId, default_weight| LayerKind::BlendWeight {
            group: group.clone(),
            default_weight,
        };
        if !layers
            .iter()
            .any(|layer| matches!(layer.kind, LayerKind::BlendWeight { .. }))
        {
            let (name, color) = LAYER_STARTS[0];
            layers.push(LayerDefinition {
                id: self.fresh_id(),
                name: name.to_owned(),
                kind: blend(&group, 1.0),
                color_linear_rgba: Some(color),
            });
        }
        let (rock, color) = LAYER_STARTS[2];
        let name = if layers.iter().any(|layer| layer.name == rock) {
            format!("Steep ground {}", layers.len() + 1)
        } else {
            rock.to_owned()
        };
        let output = self.fresh_id();
        layers.push(LayerDefinition {
            id: output.clone(),
            name,
            kind: blend(&group, 0.0),
            color_linear_rgba: Some(color),
        });
        let id = self.fresh_id();
        let mut rules = terrain.rules.clone();
        rules.push(rules::new_slope(id.clone(), seed_of(&id), output));
        self.submit(Command::Edit {
            label_key: "rule".into(),
            mutations: vec![
                Self::registry(object.id.clone(), terrain, layers, Some(group)),
                Mutation::TerrainRules {
                    object: object.id.clone(),
                    rules: rules::tidy(rules),
                },
            ],
        });
        Some(id.0)
    }

    /// Removes a rule. The layer it filled stays, and shows what is painted on it again.
    pub fn remove_rule(&mut self, rule: &str) {
        self.change_rules(|rules| rules.retain(|other| other.id.0 != rule));
    }

    /// Gives a rule a new value for one of its settings, held to the range the core takes.
    pub fn set_rule_setting(&mut self, rule: &str, setting: &rules::Setting, value: f32) {
        let value = value.clamp(setting.min, setting.max);
        let value = if setting.whole { value.round() } else { value };
        self.change_rules(|rules| {
            if let Some(rule) = rules.iter_mut().find(|other| other.id.0 == rule) {
                rule.parameters.insert(setting.key.to_owned(), value);
            }
        });
    }

    /// Steps one of a rule's layers on to the next choice: see [`rules::next_layer`].
    pub fn step_rule_layer(&mut self, rule: &str, place: RulePlace) {
        let snapshot = self.snapshot();
        let Some((_, terrain)) = landscape(&snapshot) else {
            return;
        };
        let Some(now) = terrain.rules.iter().find(|other| other.id.0 == rule) else {
            return;
        };
        let taken_by_others = |layer: &ObjectId| {
            terrain
                .rules
                .iter()
                .any(|other| other.id != now.id && other.output_layer.as_ref() == Some(layer))
        };
        let all = terrain.layers.iter().map(|layer| layer.id.clone());
        let next = match place {
            // Not a layer that already keeps the grass out.
            RulePlace::GrowsOn => {
                let choices: Vec<_> = all
                    .filter(|layer| !now.exclusion_layers.contains(layer))
                    .collect();
                rules::next_layer(&choices, now.density_layer.as_ref(), true)
            }
            // Not the layer the grass grows on.
            RulePlace::KeptOutBy => {
                let choices: Vec<_> = all
                    .filter(|layer| now.density_layer.as_ref() != Some(layer))
                    .collect();
                rules::next_layer(&choices, now.exclusion_layers.first(), true)
            }
            // A layer no other rule fills; a rule of this kind always fills one.
            RulePlace::Fills => {
                let choices: Vec<_> = all.filter(|layer| !taken_by_others(layer)).collect();
                rules::next_layer(&choices, now.output_layer.as_ref(), false)
            }
        };
        self.change_rules(|rules| {
            if let Some(rule) = rules.iter_mut().find(|other| other.id.0 == rule) {
                match place {
                    RulePlace::GrowsOn => rule.density_layer = next,
                    RulePlace::KeptOutBy => rule.exclusion_layers = next.into_iter().collect(),
                    RulePlace::Fills => rule.output_layer = next,
                }
            }
        });
    }

    /// Changes the landscape's rules and sends the whole list, if anything changed.
    fn change_rules(&mut self, change: impl FnOnce(&mut Vec<RuleDefinition>)) {
        let snapshot = self.snapshot();
        let Some((object, terrain)) = landscape(&snapshot) else {
            return;
        };
        let mut rules = terrain.rules.clone();
        change(&mut rules);
        if rules != terrain.rules {
            self.set_rules(object.id.clone(), rules);
        }
    }

    /// Sends the landscape's whole list of rules. One undo step.
    fn set_rules(&mut self, object: ObjectId, rules: Vec<RuleDefinition>) {
        self.submit(Command::Edit {
            label_key: "rule".into(),
            mutations: vec![Mutation::TerrainRules {
                object,
                rules: rules::tidy(rules),
            }],
        });
    }

    /// Renames a layer of the landscape. One undo step.
    pub fn rename_layer(&mut self, layer: &str, name: String) {
        let snapshot = self.snapshot();
        let Some((object, terrain)) = landscape(&snapshot) else {
            return;
        };
        let mut layers = terrain.layers.clone();
        let Some(definition) = layers
            .iter_mut()
            .find(|definition| definition.id.0 == layer)
        else {
            return;
        };
        definition.name = name;
        let group = terrain.surface_group.clone();
        self.set_layers(object.id.clone(), terrain, layers, group);
    }

    /// Sends the landscape's whole list of layers. The core keeps what is painted on the
    /// layers that stay.
    fn set_layers(
        &mut self,
        object: ObjectId,
        terrain: &TerrainDocument,
        layers: Vec<LayerDefinition>,
        surface_group: Option<ObjectId>,
    ) {
        self.submit(Command::Edit {
            label_key: "layers".into(),
            mutations: vec![Self::registry(object, terrain, layers, surface_group)],
        });
    }

    /// The landscape's whole list of layers as the core takes it.
    fn registry(
        object: ObjectId,
        terrain: &TerrainDocument,
        layers: Vec<LayerDefinition>,
        surface_group: Option<ObjectId>,
    ) -> Mutation {
        // One weight per height sample, unless the landscape's layers already say otherwise.
        let side_samples = terrain
            .tiles
            .iter()
            .flat_map(|tile| tile.layers.first())
            .map(|layer| layer.side_samples)
            .next()
            .unwrap_or(terrain.cells_per_tile + 1);
        Mutation::TerrainRegistry {
            object,
            layers,
            surface_group,
            side_samples,
        }
    }

    /// Puts the problem away. It comes back if the same thing fails again.
    pub fn dismiss_problem(&mut self) {
        self.problem = None;
        self.problem_from = None;
    }

    /// Whether the problem shown came from saving the level.
    pub fn problem_is_from_save(&self) -> bool {
        self.problem.is_some() && self.problem_from == Some(Source::Job(JobKind::Save))
    }

    /// Puts an object under another, after that one's other children, or at the top of the
    /// level with `None`. One undo step. A move the core refuses, such as under its own
    /// descendant, is reported like any other problem.
    pub fn reparent(&mut self, object: ObjectId, parent: Option<ObjectId>) {
        let order = self
            .snapshot()
            .document
            .objects
            .iter()
            .filter(|other| other.parent == parent && other.id != object)
            .map(|other| other.sibling_order + 1)
            .max()
            .unwrap_or(0);
        self.submit(Command::Edit {
            label_key: "reparent".into(),
            mutations: vec![Mutation::Reparent {
                object,
                parent,
                order,
            }],
        });
    }

    /// Renames an object. One undo step.
    pub fn rename(&mut self, object: ObjectId, name: String) {
        self.submit_from_inspector(Command::Edit {
            label_key: "rename".into(),
            mutations: vec![Mutation::Rename { object, name }],
        });
    }

    pub fn select(&mut self, objects: Vec<ObjectId>) {
        self.submit(Command::Select(objects));
    }

    pub fn undo(&mut self) {
        self.submit(Command::Undo);
    }

    pub fn redo(&mut self) {
        self.submit(Command::Redo);
    }

    /// Replaces the open level with a new, empty one that will be saved to `level_path`.
    /// Whatever was not saved in the old level is gone; the caller has asked first.
    ///
    /// The core gets a new session for it, so every panel and the view start afresh, and the
    /// old level's undo steps do not reach into the new one.
    pub fn new_level(&mut self, level_path: PathBuf) {
        let document = SceneDocument {
            schema: scene_data::SCHEMA_VERSION,
            id: self.fresh_id(),
            objects: Vec::new(),
        };
        match Self::new(document, level_path, self.package_root.clone()) {
            Ok(mut fresh) => {
                fresh.next_id = self.next_id;
                // A game that runs keeps running on its own cooked level: the coordinator
                // and what is known of the last run belong to the editor, not to the level.
                fresh.play = self.play.take();
                fresh.next_play_request = self.next_play_request;
                fresh.start_unanswered = self.start_unanswered;
                fresh.play_refusal = self.play_refusal.take();
                fresh.last_end = self.last_end.take();
                fresh.end_dismissed = self.end_dismissed;
                fresh.cleanup_dismissed = self.cleanup_dismissed;
                *self = fresh;
            }
            Err(problem) => {
                self.problem = Some(problem);
                self.problem_from = Some(Source::Command);
            }
        }
    }

    /// Saves again where the last save was meant to go.
    pub fn retry_save(&mut self) {
        match self.save_as_target.clone() {
            Some(path) => self.save_as(path),
            None => self.save(),
        }
    }

    /// Saves the level to [`Editor::level_path`].
    pub fn save(&mut self) {
        self.save_as_target = None;
        let operation = JobOperation::Save {
            path: self.level_path.clone(),
            expected: self.on_disk,
        };
        self.pending_path = None;
        self.start_job(JobKind::Save, operation);
    }

    /// Saves the level to another file. The editor has not seen that file, so it must not
    /// exist; the new path becomes the level's path only when the save succeeded.
    pub fn save_as(&mut self, path: PathBuf) {
        if path == self.level_path {
            self.save();
            return;
        }
        let operation = JobOperation::Save {
            path: path.clone(),
            expected: DiskVersion::Absent,
        };
        self.save_as_target = Some(path.clone());
        self.pending_path = Some(path);
        self.start_job(JobKind::Save, operation);
    }

    /// Opens a level file. The level that is open stays until the file has been read and
    /// found valid; the path is adopted only then.
    pub fn open(&mut self, path: PathBuf) {
        self.pending_path = Some(path.clone());
        self.start_job(JobKind::Open, JobOperation::Open(path));
    }

    /// Whether the level has a file: it was opened from one or saved to one.
    pub fn has_file(&self) -> bool {
        self.has_file
    }

    /// Asks the core for the level's geometry if the view's is out of date. Cheap to call
    /// every frame: it asks once per state of the level.
    pub fn want_geometry(&mut self) {
        let wanted = GeometryBinding::from_snapshot(&self.snapshot());
        if self.geometry_asked == Some(wanted) {
            return;
        }
        if self.handle.job(JobOperation::Geometry).is_ok() {
            self.running_jobs += 1;
            self.geometry_asked = Some(wanted);
        }
    }

    /// The geometry the core has made, if it is of the level's current state. Geometry of an
    /// earlier state is dropped: a newer one has been asked for.
    pub fn take_geometry(&mut self) -> Option<Geometry> {
        let geometry = self.geometry.take()?;
        geometry
            .binding
            .matches(&self.snapshot())
            .then_some(geometry)
    }

    /// Cooks the level into [`Editor::package_root`].
    pub fn cook(&mut self) {
        let operation = JobOperation::Cook {
            package_root: self.package_root.clone(),
        };
        self.start_job(JobKind::Cook, operation);
    }

    /// The cooked level of the last finished cook, if there was one in this session.
    pub fn cooked(&self) -> Option<&VerifiedPackageReceipt> {
        self.cooked.as_ref()
    }

    /// What is known of the cooked level as it is on disk now.
    pub fn package_check(&self) -> &PackageCheck {
        &self.package_check
    }

    /// Asks the core whether the cooked level on disk is still the one this editor cooked:
    /// every hash, the schema, the profile and the engine revision of the package it
    /// published. Does nothing without a cooked level or while a check is under way.
    pub fn check_cooked(&mut self) {
        let Some(receipt) = &self.cooked else {
            return;
        };
        if self.package_check == PackageCheck::Checking {
            return;
        }
        let operation = JobOperation::VerifyPackage {
            path: receipt.path().to_path_buf(),
            expected: Some(receipt.identity().clone()),
        };
        match self.handle.job(operation) {
            Ok(ticket) => {
                self.running_jobs += 1;
                self.check_ticket = Some(ticket);
                self.package_check = PackageCheck::Checking;
            }
            Err(problem) => {
                self.package_check = PackageCheck::Refused {
                    code: problem.code.clone(),
                };
                self.report(Source::Job(JobKind::VerifyPackage), Err(problem));
            }
        }
    }

    /// Takes the end of a package check, and says what to report of it: nothing, a success
    /// or a problem.
    ///
    /// Only the end of the check under way is news. A check that was given up, by Stop or
    /// because a new cook replaced its package, still ends some time later; by then another
    /// check may be under way. The older one's end must not end that check, take away the
    /// means to cancel it, or be shown as its result. The core runs jobs one after another
    /// today, so an older check always ends first; this does not rely on that.
    fn check_finished(
        &mut self,
        job: u64,
        found: Result<(VerifiedPackageReceipt, u64), Diagnostic>,
    ) -> Option<Result<(), Diagnostic>> {
        if self.check_ticket.as_ref().map(|ticket| ticket.id) != Some(job) {
            return None;
        }
        self.check_ticket = None;
        match found {
            Ok((receipt, micros)) => {
                // The check is of the package the editor holds now, or it says nothing.
                let current = self.cooked.as_ref() == Some(&receipt);
                self.package_check = if current {
                    PackageCheck::Passed { micros }
                } else {
                    PackageCheck::NotAsked
                };
                // A way to play was waiting for this: the game is started from the receipt
                // of this very check, never from an older one.
                if let Some(mode) = self.play_when_checked.take()
                    && current
                {
                    // A new run begins: what the last one left to say is put away.
                    self.last_end = None;
                    self.start_unanswered = true;
                    self.submit_play(PlayControl::Start { mode, receipt });
                }
                Some(Ok(()))
            }
            // The person stopped the check: nothing is known, and nothing went wrong.
            Err(problem) if problem.code == "job.cancelled" => {
                self.package_check = PackageCheck::NotAsked;
                self.play_when_checked = None;
                None
            }
            Err(problem) => {
                self.package_check = PackageCheck::Refused {
                    code: problem.code.clone(),
                };
                // Nothing is played from a package the core refused.
                self.play_when_checked = None;
                Some(Err(problem))
            }
        }
    }

    /// Asks the core to give up the check that is under way. The core stops between two
    /// reads; a check that had finished already keeps its result.
    pub fn cancel_check(&mut self) {
        if let Some(ticket) = &self.check_ticket {
            ticket.cancel();
        }
    }

    /// Gives the editor a game to start: the core's Play coordinator.
    pub fn connect_play(&mut self, handle: PlayHandle) {
        self.play = Some(handle);
    }

    /// Gives the editor the game in this folder to start: the folder its server and client
    /// are built in, laid out as the game's repository is. The coordinator checks the
    /// programs when a run is started, not here; a game that is not built is a start that
    /// fails, with its reason. The game's logs go under the folder's `target`.
    pub fn connect_game(&mut self, folder: PathBuf) -> Result<(), Diagnostic> {
        let logs = folder.join("target/arc-editor-play-logs");
        let handle = PlayHandle::start(PlayConfig::from_game_worktree(folder, logs))?;
        self.connect_play(handle);
        Ok(())
    }

    /// Whether this editor has a game to start.
    pub fn play_connected(&self) -> bool {
        self.play.is_some()
    }

    /// Has the coordinator call `wake` whenever a run changes, from its own thread. The game
    /// runs while the editor sleeps; this is what makes the editor look again.
    pub fn set_play_waker(&self, wake: Box<dyn Fn() + Send + Sync + 'static>) {
        if let Some(play) = &self.play {
            play.set_waker(wake);
        }
    }

    /// The coordinator's latest account of the run. Cheap, and it never waits.
    pub fn play_status(&self) -> Option<Arc<PlayStatus>> {
        self.play.as_ref().map(PlayHandle::snapshot)
    }

    /// The run that is starting, running or stopping, if there is one.
    pub fn active_run(&self) -> Option<RunId> {
        let status = self.play_status()?;
        let run = status.run.as_ref()?;
        (!matches!(run.state, PlayState::Ended(_))).then_some(run.id)
    }

    /// Whether a start was asked for and the coordinator has not yet said what became of
    /// it: the check of the cooked level is under way for it, or the start itself is.
    pub fn play_requested(&self) -> bool {
        self.play_when_checked.is_some() || self.start_unanswered
    }

    /// What the coordinator last refused, until the next request.
    pub fn play_refusal(&self) -> Option<&Diagnostic> {
        self.play_refusal.as_ref()
    }

    /// How the last run ended.
    pub fn last_end(&self) -> Option<&PlayEnd> {
        self.last_end.as_ref()
    }

    /// Whether the notice of how the last run ended, and the notice of what it left
    /// behind, were put away.
    pub fn end_notices_dismissed(&self) -> (bool, bool) {
        (self.end_dismissed, self.cleanup_dismissed)
    }

    pub fn dismiss_run_end(&mut self) {
        self.end_dismissed = true;
    }

    pub fn dismiss_cleanup(&mut self) {
        self.cleanup_dismissed = true;
    }

    /// Plays the cooked level in the game's own window. The cooked level on disk is checked
    /// first, and the game is started from the receipt of that check; a package the core
    /// refuses is not played. Does nothing without a game, without a cooked level, or while
    /// a run or a start is under way.
    pub fn play_external(&mut self) {
        if self.play.is_none()
            || self.cooked.is_none()
            || self.active_run().is_some()
            || self.play_requested()
        {
            return;
        }
        self.play_refusal = None;
        self.play_when_checked = Some(PlayMode::ExternalGame);
        if self.package_check != PackageCheck::Checking {
            self.check_cooked();
        }
        if self.package_check != PackageCheck::Checking {
            // The check could not even be asked for; its problem is reported already.
            self.play_when_checked = None;
        }
    }

    /// Stops whatever the Play control shows as under way: the check of the cooked level,
    /// with the start that waited for it, or the run. A run that is still starting is
    /// stopped the same way.
    pub fn stop_play(&mut self) {
        self.play_when_checked = None;
        if self.package_check == PackageCheck::Checking {
            self.cancel_check();
        } else if let Some(run) = self.active_run() {
            self.submit_play(PlayControl::Stop { run });
        }
    }

    fn submit_play(&mut self, control: PlayControl) {
        let Some(play) = &self.play else {
            return;
        };
        let request = PlayRequest {
            id: self.next_play_request,
            control,
        };
        self.next_play_request += 1;
        if let Err(problem) = play.submit(request) {
            self.start_unanswered = false;
            self.play_refusal = Some(problem);
        }
    }

    /// Takes what the Play coordinator has to say since the last frame.
    fn pump_play(&mut self) {
        let Some(play) = &mut self.play else {
            return;
        };
        for event in play.poll(EVENTS_PER_FRAME) {
            match event {
                PlayEvent::Answer(answer) => {
                    self.start_unanswered = false;
                    match answer.result {
                        // A stop that came as the run ended anyway is not news.
                        Err(problem)
                            if matches!(
                                problem.code.as_str(),
                                "play.no_active_run" | "play.stale_run"
                            ) => {}
                        Err(problem) => self.play_refusal = Some(problem),
                        Ok(()) => self.play_refusal = None,
                    }
                }
                PlayEvent::Ended { end, .. } => {
                    self.last_end = Some(end);
                    self.end_dismissed = false;
                    self.cleanup_dismissed = false;
                }
            }
        }
    }

    fn start_job(&mut self, kind: JobKind, operation: JobOperation) {
        match self.handle.job(operation) {
            Ok(_ticket) => self.running_jobs += 1,
            Err(problem) => self.report(Source::Job(kind), Err(problem)),
        }
    }

    /// Takes what the core has finished since the last frame.
    fn pump(&mut self) {
        for event in self.handle.poll(EVENTS_PER_FRAME) {
            match event {
                ServiceEvent::Command(result) => {
                    debug!(
                        "answer to command {}: {:?}",
                        result.request,
                        result.outcome.as_ref().map(outcome_name)
                    );
                    self.unanswered = self.unanswered.saturating_sub(1);
                    let silent = self.silent == Some(result.request);
                    if silent {
                        self.silent = None;
                    }
                    let committed = self
                        .committing
                        .take_if(|(request, _)| *request == result.request)
                        .map(|(_, transaction)| transaction);
                    match &result.outcome {
                        Ok(CommandOutcome::TransactionStarted(transaction)) => {
                            self.drag_started(*transaction);
                        }
                        Err(_) => {
                            // A drag the core refused or lost is over. The core keeps its
                            // transaction open after a refusal, so it is given up here.
                            let open = self
                                .drag
                                .take()
                                .and_then(|drag| drag.transaction)
                                .or(committed);
                            if let Some(transaction) = open {
                                self.release(transaction);
                            }
                        }
                        Ok(_) => {}
                    }
                    if !silent {
                        self.report(Source::Command, result.outcome.map(|_| ()));
                    }
                }
                ServiceEvent::JobFinished { fact, result } => {
                    self.running_jobs = self.running_jobs.saturating_sub(1);
                    if fact.kind == JobKind::VerifyPackage {
                        // Not a file job: the pending path is left to the job it belongs to.
                        let found = match result {
                            Ok(JobOutput::PackageVerified {
                                receipt,
                                verification_us,
                            }) => Ok((receipt, verification_us)),
                            Ok(_) => continue,
                            Err(problem) => Err(problem),
                        };
                        if let Some(outcome) = self.check_finished(fact.id, found) {
                            self.report(Source::Job(fact.kind), outcome);
                        }
                        continue;
                    }
                    let pending_path = self.pending_path.take();
                    let outcome = match result {
                        Ok(JobOutput::Geometry {
                            binding,
                            simulation,
                            visual,
                            ..
                        }) => {
                            // Not a file job: the pending path is someone else's.
                            self.pending_path = pending_path;
                            self.geometry = Some(Geometry {
                                binding,
                                simulation,
                                visual,
                            });
                            Ok(())
                        }
                        Ok(JobOutput::Opened { disk_hash, .. }) => {
                            // Another level is open now: the cooked package the editor
                            // held was of the level before it.
                            self.cancel_check();
                            self.check_ticket = None;
                            self.cooked = None;
                            self.package_check = PackageCheck::NotAsked;
                            self.on_disk = DiskVersion::Hash(disk_hash);
                            self.has_file = true;
                            if let Some(path) = pending_path {
                                self.level_path = path;
                            }
                            Ok(())
                        }
                        Ok(JobOutput::Saved(receipt)) => {
                            self.on_disk = DiskVersion::Hash(receipt.disk_hash);
                            self.has_file = true;
                            if let Some(path) = pending_path {
                                self.level_path = path;
                            }
                            receipt.durability_warning.map_or(Ok(()), Err)
                        }
                        Ok(JobOutput::Cooked {
                            receipt,
                            source_hash,
                            simulation_hash,
                            ..
                        }) => {
                            self.last_cook = Some((source_hash.hex(), simulation_hash.hex()));
                            // A new package: whatever was known of the old one is not about
                            // this one. A check of the old one is given up and forgotten, so
                            // that its end, whenever it comes, is not taken for news.
                            self.cancel_check();
                            self.check_ticket = None;
                            self.cooked = Some(receipt);
                            self.package_check = PackageCheck::NotAsked;
                            Ok(())
                        }
                        // A kind of result the core added that needs nothing done here.
                        #[allow(
                            unreachable_patterns,
                            reason = "the core's job results grow ahead of the UI"
                        )]
                        Ok(_) => Ok(()),
                        Err(problem) => {
                            // A geometry job that failed leaves a file job's path alone.
                            if fact.kind == JobKind::Geometry {
                                self.pending_path = pending_path;
                                // Ask again when the level changes, not every frame.
                            }
                            if superseded(fact.kind, &problem.code) {
                                // Not a failure and not a success: the newer request stays
                                // asked, and a real geometry problem stays shown.
                                continue;
                            }
                            Err(problem)
                        }
                    };
                    self.report(Source::Job(fact.kind), outcome);
                }
                ServiceEvent::JobStarted(_) | ServiceEvent::Snapshot(_) => {}
            }
        }
        self.pump_play();
        self.select_what_was_added();
    }
}

/// A place in a rule that holds a layer.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum RulePlace {
    /// The layer grass grows on; none means everywhere.
    #[default]
    GrowsOn,
    /// A layer that keeps grass out; none means nothing does.
    KeptOutBy,
    /// The layer a slope rule fills.
    Fills,
}

/// A rule's seed from its ID: as unique as the ID, and the same on every machine once the
/// level holds it.
fn seed_of(id: &ObjectId) -> u64 {
    u64::from_str_radix(id.0.get(16..).unwrap_or_default(), 16).unwrap_or(1)
}

/// What the first layers of a landscape are called and coloured (linear RGB), until the
/// person renames them. Further layers are "Layer 6" and so on, in grey.
pub const LAYER_STARTS: [(&str, [f32; 4]); 5] = [
    ("Grass", [0.10, 0.25, 0.05, 1.0]),
    ("Dirt", [0.22, 0.13, 0.06, 1.0]),
    ("Rock", [0.20, 0.20, 0.21, 1.0]),
    ("Sand", [0.55, 0.45, 0.25, 1.0]),
    ("Snow", [0.85, 0.87, 0.90, 1.0]),
];

/// The landscape the landscape tools work on: the selected one, or else the level's first.
pub fn landscape(snapshot: &ViewSnapshot) -> Option<(&ObjectRecord, &TerrainDocument)> {
    let terrain = |object: &'_ ObjectRecord| matches!(&object.data, ObjectData::Terrain(_));
    let document = &snapshot.document;
    let object = snapshot
        .selection
        .iter()
        .filter_map(|id| document.object(id))
        .find(|object| terrain(object))
        .or_else(|| document.objects.iter().find(|object| terrain(object)))?;
    match &object.data {
        ObjectData::Terrain(data) => Some((object, data)),
        _ => None,
    }
}

/// The average weight of a layer as it is painted, 0 to 1: the mean over the tiles of each
/// tile's mean weight, a tile nothing is painted on counting with what new ground starts
/// with. It is an average of the painted samples, not an area of the world, and it says
/// nothing about a layer a rule fills: `None` for those.
pub fn layer_average(terrain: &TerrainDocument, layer: &LayerDefinition) -> Option<f32> {
    if filled_by_rule(terrain, layer) {
        return None;
    }
    let start = match layer.kind {
        LayerKind::BlendWeight { default_weight, .. } => default_weight,
        LayerKind::Mask => 0.0,
    };
    if terrain.tiles.is_empty() {
        return Some(start);
    }
    let sum: f64 = terrain
        .tiles
        .iter()
        .map(|tile| {
            tile.layers
                .iter()
                .find(|payload| payload.layer == layer.id && !payload.weights.is_empty())
                .map_or(f64::from(start), |payload| {
                    payload
                        .weights
                        .iter()
                        .map(|weight| f64::from(*weight))
                        .sum::<f64>()
                        / payload.weights.len() as f64
                })
        })
        .sum();
    Some((sum / terrain.tiles.len() as f64) as f32)
}

/// Whether a rule of the landscape writes this layer. Such a layer shows what the rule
/// makes, whatever is painted on it.
pub fn filled_by_rule(terrain: &TerrainDocument, layer: &LayerDefinition) -> bool {
    terrain
        .rules
        .iter()
        .any(|rule| rule.output_layer.as_ref() == Some(&layer.id))
}

/// Whether a job ended only because a newer request took its place. The core ends a geometry
/// job that way when the level has moved on or a newer geometry request is queued: the view
/// has already asked for the newer state, so there is nothing to tell the person.
fn superseded(kind: JobKind, code: &str) -> bool {
    kind == JobKind::Geometry && matches!(code, "job.cancelled" | "job.stale")
}

/// What kind of action a problem came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Source {
    Command,
    Job(JobKind),
}

/// Takes the core's results every frame, and keeps frames coming while the core still owes
/// something. Without that the window, which only redraws on input, would show a result late.
pub fn pump_editor(mut editor: NonSendMut<Editor>, mut redraw: MessageWriter<RequestRedraw>) {
    editor.pump();
    if editor.busy() {
        redraw.write(RequestRedraw);
    }
}

/// Runs before every system that shows the level.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct PumpEditor;

/// The name of a command, for the log.
fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Edit { .. } => "edit",
        Command::Select(_) => "select",
        Command::Undo => "undo",
        Command::Redo => "redo",
        Command::BeginViewportEdit { .. } => "begin drag",
        Command::Preview { .. } => "preview",
        Command::PreviewTerrainSculpt { .. } => "preview sculpt",
        Command::PreviewTerrainPaint { .. } => "preview paint",
        Command::CommitViewportEdit { .. } => "commit drag",
        Command::CancelViewportEdit { .. } => "cancel drag",
        // A command the core added that this log has no name for yet.
        #[allow(
            unreachable_patterns,
            reason = "the core's commands grow ahead of the UI"
        )]
        _ => "another command",
    }
}

/// The name of a command's outcome, for the log.
fn outcome_name(outcome: &CommandOutcome) -> &'static str {
    match outcome {
        CommandOutcome::Applied => "applied",
        CommandOutcome::SelectionChanged => "selection changed",
        CommandOutcome::TransactionStarted(_) => "drag started",
        CommandOutcome::Previewed => "previewed",
        CommandOutcome::TerrainSculpted { .. } => "sculpted",
        CommandOutcome::TerrainPainted { .. } => "painted",
        CommandOutcome::Cancelled => "cancelled",
        #[allow(
            unreachable_patterns,
            reason = "the core's outcomes grow ahead of the UI"
        )]
        _ => "another outcome",
    }
}

/// The bits of a new ID: the time above, the counter below it, and the lowest bit set so that
/// the ID is never zero. The counter keeps clear of that bit. The time alone does not tell two
/// IDs apart: the clock of some systems (macOS) counts in microseconds, and the four tiles of a
/// new landscape are named within one.
fn id_bits(nanos: u128, counter: u64) -> u128 {
    (nanos << 20) ^ (u128::from(counter) << 1) | 1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An editor on the game's current map, with its level and cooked folders in `folder`,
    /// after one cook.
    fn cooked_in(folder: &std::path::Path) -> Editor {
        let mut editor = Editor::new(
            scene_data::current_map(),
            folder.join("levels/map.level.json"),
            folder.join("cooked"),
        )
        .unwrap();
        editor.cook();
        settle(&mut editor);
        assert_eq!(editor.problem, None);
        editor
    }

    /// Adds a box and waits until the core has it, so that the next cook is of another level.
    fn change_the_level(editor: &mut Editor) {
        let before = editor.snapshot().document.objects.len();
        editor.add_box(&down_at(3.0, 3.0));
        settle(editor);
        assert_eq!(editor.snapshot().document.objects.len(), before + 1);
    }

    use crate::{
        panels::{
            notice::{notices, NoticeWords},
            play::{PlayView, RunView},
        },
        play_script::{Script, SERVER_PID},
    };

    /// Pumps until `done` holds.
    fn until(editor: &mut Editor, what: &str, done: impl Fn(&Editor) -> bool) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            editor.pump();
            if done(editor) {
                return;
            }
            assert!(std::time::Instant::now() < end, "never came: {what}");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    /// An editor with a cooked level and a scripted game in the game's place.
    fn with_game(folder: &std::path::Path, script: Script) -> Editor {
        let mut editor = cooked_in(folder);
        editor.connect_play(script.handle().unwrap());
        editor
    }

    fn run(editor: &Editor) -> RunView {
        PlayView::of(editor).run
    }

    /// Plays in the game's window and waits until the run has ended.
    fn play_to_the_end(editor: &mut Editor) {
        editor.play_external();
        until(editor, "the run's end", |editor| {
            editor.last_end().is_some()
        });
        assert_eq!(run(editor), RunView::None);
    }

    fn run_end_notice(editor: &Editor) -> Option<NoticeWords> {
        notices(editor)[1].clone()
    }

    fn left_behind_notice(editor: &Editor) -> Option<NoticeWords> {
        notices(editor)[2].clone()
    }

    #[test]
    fn the_game_is_started_from_a_check_that_passed_and_shown_running_only_when_it_is_ready() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = with_game(folder.path(), Script::Runs);
        let manifest = editor.cooked().unwrap().identity().manifest_hash.hex();
        assert!(PlayView::of(&editor).game_window_offered());

        editor.play_external();
        // Asked for: the cooked level is being checked first, and nothing runs yet.
        assert_eq!(editor.package_check(), &PackageCheck::Checking);
        assert_eq!(run(&editor), RunView::Starting);
        assert!(editor.active_run().is_none());

        until(&mut editor, "the game running", |editor| {
            run(editor) == RunView::Running
        });
        let view = PlayView::of(&editor);
        assert!(matches!(view.check, PackageCheck::Passed { .. }));
        assert_eq!(view.word().as_deref(), Some("Game window open"));
        assert!(view.word_is_running() && view.stop_offered());
        assert_eq!(view.run_package.as_deref(), Some(&manifest[..8]));
        assert_eq!(
            view.run_words(),
            format!("playing cooked {}", &manifest[..8])
        );
        assert!(!view.game_window_offered(), "one run at a time");
        // A game that runs is not work the editor waits for: it may sleep.
        assert!(!editor.busy());
        assert_eq!(editor.problem, None);

        editor.stop_play();
        until(&mut editor, "the run's end", |editor| {
            editor.last_end().is_some()
        });
        let view = PlayView::of(&editor);
        assert_eq!(view.run, RunView::None);
        assert_eq!(view.ended.as_deref(), Some("play.stopped"));
        assert_eq!(view.run_words(), "Stopped");
        assert_eq!(view.word(), None, "the control is its button again");
        // The person asked for this end: it is not a notice.
        assert_eq!(run_end_notice(&editor), None);
        assert_eq!(left_behind_notice(&editor), None);
        assert!(view.game_window_offered(), "and it can be played again");
    }

    #[test]
    fn a_game_that_is_still_starting_can_be_stopped() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = with_game(folder.path(), Script::Hangs);
        editor.play_external();
        // The client is accepted by the server and then never becomes ready.
        until(&mut editor, "the client's last step", |editor| {
            PlayView::of(editor)
                .steps
                .contains("client accepted by the server")
        });
        let view = PlayView::of(&editor);
        assert_eq!(view.run, RunView::Starting, "ready is not assumed");
        assert_eq!(view.word().as_deref(), Some("Starting"));
        assert!(!view.word_is_running());
        assert!(view.steps.contains("server collision ready"));
        assert!(view.run_words().starts_with("starting cooked "));
        assert!(view.stop_offered());

        editor.stop_play();
        until(&mut editor, "the run's end", |editor| {
            editor.last_end().is_some()
        });
        assert_eq!(PlayView::of(&editor).ended.as_deref(), Some("play.stopped"));
        assert_eq!(run_end_notice(&editor), None);
    }

    #[test]
    fn a_game_that_stops_by_itself_is_a_notice_with_its_exit_code_and_log() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = with_game(folder.path(), Script::Crashes);
        play_to_the_end(&mut editor);
        let view = PlayView::of(&editor);
        assert_eq!(view.ended.as_deref(), Some("play.crashed"));
        assert_eq!(
            view.run_words(),
            "",
            "the status bar does not repeat a notice"
        );
        let notice = run_end_notice(&editor).expect("a crash stays on screen");
        assert_eq!(notice.title, "The game stopped unexpectedly");
        assert!(notice.remedy.contains("ended by itself"));
        assert!(notice.facts.contains("exit code 101"), "{}", notice.facts);
        assert!(notice.facts.contains("client.log"), "{}", notice.facts);
        assert_eq!(left_behind_notice(&editor), None, "its server shut down");

        // It stays until it is put away, and a problem with the level has its own place.
        editor.pump();
        assert!(run_end_notice(&editor).is_some());
        editor.dismiss_run_end();
        assert_eq!(run_end_notice(&editor), None);
    }

    #[test]
    fn a_game_window_the_person_closed_is_said_in_the_status_bar_and_is_no_notice() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = with_game(folder.path(), Script::WindowClosed);
        play_to_the_end(&mut editor);
        let view = PlayView::of(&editor);
        assert_eq!(view.ended.as_deref(), Some("play.window_closed"));
        assert_eq!(view.run_words(), "The game window was closed");
        assert_eq!(run_end_notice(&editor), None);
        assert_eq!(left_behind_notice(&editor), None);
    }

    #[test]
    fn a_server_that_will_not_go_is_a_notice_of_its_own_beside_how_the_run_ended() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = with_game(folder.path(), Script::ServerStays);
        editor.play_external();
        until(&mut editor, "the game running", |editor| {
            run(editor) == RunView::Running
        });
        editor.stop_play();
        until(&mut editor, "stopping shown", |editor| {
            run(editor) != RunView::Running
        });
        let stopping = PlayView::of(&editor);
        if stopping.run == RunView::Stopping {
            assert!(stopping.stop_shown() && !stopping.stop_offered());
            assert_eq!(stopping.run_words(), "stopping the game");
        }
        until(&mut editor, "the run's end", |editor| {
            editor.last_end().is_some()
        });
        // How the run ended is what the person asked for, and is not hidden by what it left.
        assert_eq!(PlayView::of(&editor).ended.as_deref(), Some("play.stopped"));
        assert_eq!(PlayView::of(&editor).run_words(), "Stopped");
        assert_eq!(run_end_notice(&editor), None);
        let left = left_behind_notice(&editor).expect("the server is still there");
        assert_eq!(left.title, "The game's server did not shut down");
        assert!(
            left.facts.contains(&format!("process {SERVER_PID}")),
            "{}",
            left.facts
        );
        assert!(left.facts.contains("server.log"), "{}", left.facts);
        editor.dismiss_cleanup();
        assert_eq!(left_behind_notice(&editor), None);
    }

    #[test]
    fn a_server_that_had_to_be_killed_is_gone_and_is_not_said_to_be_left_behind() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = with_game(folder.path(), Script::NeedsKill);
        editor.play_external();
        until(&mut editor, "the game running", |editor| {
            run(editor) == RunView::Running
        });
        editor.stop_play();
        until(&mut editor, "the run's end", |editor| {
            editor.last_end().is_some()
        });
        let end = editor.last_end().unwrap().clone();
        // The coordinator says the server's cleanup overran, and that it saw it end.
        assert!(end
            .cleanup
            .diagnostics
            .iter()
            .any(|problem| problem.code == "play.server_cleanup_failed"));
        assert!(end.cleanup.server.as_ref().is_some_and(|exit| exit.forced));
        // So nothing may still be running, and nothing says so.
        assert_eq!(left_behind_notice(&editor), None);
        assert_eq!(PlayView::of(&editor).run_words(), "Stopped");
    }

    #[test]
    fn a_game_that_cannot_be_launched_yet_is_said_so_and_never_shown_running() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = with_game(folder.path(), Script::Unavailable);
        editor.play_external();
        let mut ran = false;
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while editor.last_end().is_none() {
            editor.pump();
            ran |= run(&editor) == RunView::Running;
            assert!(std::time::Instant::now() < end, "the run never ended");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(!ran, "a game that was never launched was shown as running");
        assert_eq!(
            PlayView::of(&editor).ended.as_deref(),
            Some("play.start_failed")
        );
        let notice = run_end_notice(&editor).expect("a failed start stays on screen");
        assert_eq!(notice.title, "The game did not start");
        assert!(
            notice.remedy.contains("cannot launch the game yet"),
            "{}",
            notice.remedy
        );
        // The cooked level passed its check: it is not what is wrong.
        assert!(matches!(
            editor.package_check(),
            PackageCheck::Passed { .. }
        ));
        assert_eq!(editor.problem, None);
    }

    #[test]
    fn a_cooked_level_the_core_refuses_is_not_played() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = with_game(folder.path(), Script::Runs);
        let package = editor.cooked().unwrap().clone();
        std::fs::write(package.path().join("level.sim.json"), b"tampered").unwrap();
        editor.play_external();
        settle(&mut editor);
        editor.pump();
        assert!(matches!(
            editor.package_check(),
            PackageCheck::Refused { .. }
        ));
        assert!(
            !editor.play_requested(),
            "the start is given up with the check"
        );
        assert!(
            editor.play_status().unwrap().run.is_none(),
            "nothing was started"
        );
        assert_eq!(run(&editor), RunView::None);
        assert_eq!(
            editor.problem.as_ref().map(|problem| problem.code.as_str()),
            Some("payload.hash_mismatch")
        );
    }

    #[test]
    fn stopping_while_the_cooked_level_is_checked_gives_up_the_start_too() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = with_game(folder.path(), Script::Runs);
        editor.play_external();
        assert_eq!(editor.package_check(), &PackageCheck::Checking);
        editor.stop_play();
        assert!(!editor.play_requested());
        settle(&mut editor);
        editor.pump();
        // The check may have finished before it saw the request to stop; either way the
        // game is not started from it.
        assert!(editor.play_status().unwrap().run.is_none());
        assert_eq!(run(&editor), RunView::None);
    }

    #[test]
    fn without_a_game_nothing_is_asked_for_and_the_control_says_why() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = cooked_in(folder.path());
        let view = PlayView::of(&editor);
        assert!(!view.game && !view.game_window_offered());
        assert!(view.reason_line().contains("--game"));
        editor.play_external();
        assert!(!editor.play_requested() && !editor.busy());
        assert_eq!(editor.package_check(), &PackageCheck::NotAsked);
    }

    #[test]
    fn a_second_start_is_refused_in_one_sentence_and_the_game_keeps_running() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = with_game(folder.path(), Script::Runs);
        editor.play_external();
        until(&mut editor, "the game running", |editor| {
            run(editor) == RunView::Running
        });
        // The control offers no second start while a game runs; the coordinator is asked
        // directly, as a second window or a script might.
        let receipt = editor.cooked().unwrap().clone();
        editor.submit_play(PlayControl::Start {
            mode: PlayMode::ExternalGame,
            receipt,
        });
        until(&mut editor, "the refusal", |editor| {
            editor.play_refusal().is_some()
        });
        let view = PlayView::of(&editor);
        assert_eq!(view.refusal.as_deref(), Some("play.already_active"));
        assert_eq!(
            view.run_words(),
            "A game is already running. Stop it first."
        );
        assert_eq!(view.run, RunView::Running);
        editor.stop_play();
        until(&mut editor, "the run's end", |editor| {
            editor.last_end().is_some()
        });
    }

    #[test]
    fn a_game_keeps_running_on_its_cooked_level_when_another_level_is_begun() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = with_game(folder.path(), Script::Runs);
        editor.play_external();
        until(&mut editor, "the game running", |editor| {
            run(editor) == RunView::Running
        });
        let playing = editor.active_run();
        editor.new_level(folder.path().join("levels/other.level.json"));
        assert_eq!(editor.active_run(), playing);
        let view = PlayView::of(&editor);
        assert_eq!(view.run, RunView::Running);
        assert!(view.run_older, "the new level is not what the game plays");
        assert_eq!(view.cooked, None, "the new level is not cooked");
        editor.stop_play();
        until(&mut editor, "the run's end", |editor| {
            editor.last_end().is_some()
        });
    }

    #[test]
    fn the_play_control_names_the_cooked_level_by_its_manifests_hash() {
        let folder = tempfile::tempdir().unwrap();
        let editor = cooked_in(folder.path());
        let identity = editor.cooked().unwrap().identity().clone();
        let manifest = identity.manifest_hash.hex();
        let simulation = identity.simulation_hash.hex();
        assert_ne!(
            manifest[..8],
            simulation[..8],
            "the two hashes of this package begin differently, so the test tells them apart"
        );

        let view = PlayView::of(&editor);
        assert_eq!(view.cooked.as_deref(), Some(&manifest[..8]));
        assert_eq!(view.cooked_line(), format!("cooked {}", &manifest[..8]));
        assert_eq!(view.status_words(), format!("cooked {}", &manifest[..8]));
        // It is the name of the cooked folder, which is what the game is started with.
        let folder_name = editor.cooked().unwrap().path().file_name().unwrap();
        assert!(folder_name.to_string_lossy().starts_with(&manifest[..8]));
        assert!(view.can_check);
    }

    #[test]
    fn without_a_cooked_level_there_is_nothing_to_check() {
        let mut editor = empty_level();
        editor.check_cooked();
        assert_eq!(editor.package_check(), &PackageCheck::NotAsked);
        assert!(!editor.busy(), "no job was started");
        assert_eq!(editor.problem, None);
    }

    #[test]
    fn the_cooked_level_on_disk_is_checked_by_the_core_and_a_damaged_one_is_refused() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = cooked_in(folder.path());
        let package = editor.cooked().expect("a cook leaves its receipt").clone();
        assert_eq!(editor.package_check(), &PackageCheck::NotAsked);

        editor.check_cooked();
        assert_eq!(editor.package_check(), &PackageCheck::Checking);
        settle(&mut editor);
        assert!(matches!(
            editor.package_check(),
            PackageCheck::Passed { .. }
        ));
        assert_eq!(editor.problem, None);

        // Something else writes into the package.
        std::fs::write(package.path().join("level.sim.json"), b"tampered").unwrap();
        editor.check_cooked();
        settle(&mut editor);
        assert_eq!(
            editor.package_check(),
            &PackageCheck::Refused {
                code: "payload.hash_mismatch".into()
            }
        );
        assert_eq!(
            editor.problem.as_ref().map(|problem| problem.code.as_str()),
            Some("payload.hash_mismatch")
        );
        // Putting the notice away does not make the package good.
        editor.dismiss_problem();
        assert!(matches!(
            editor.package_check(),
            PackageCheck::Refused { .. }
        ));

        // A cooked level's folder is named by its content and is never written over, so
        // cooking the same level again cannot put the damage right. The level changes and is
        // cooked: that is another package. Nothing is known of it until asked, and then it
        // passes and the problem is gone.
        editor.check_cooked();
        settle(&mut editor);
        assert!(editor.problem.is_some());
        change_the_level(&mut editor);
        editor.cook();
        settle(&mut editor);
        assert_ne!(editor.cooked(), Some(&package), "a new package");
        assert_eq!(editor.package_check(), &PackageCheck::NotAsked);
        editor.check_cooked();
        settle(&mut editor);
        assert!(matches!(
            editor.package_check(),
            PackageCheck::Passed { .. }
        ));
        assert_eq!(editor.problem, None);
    }

    #[test]
    fn the_end_of_an_older_check_neither_ends_nor_fails_the_check_under_way() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = cooked_in(folder.path());
        let old_package = editor.cooked().unwrap().clone();

        // A check is started and stopped; the level changes and is cooked again; a new check
        // of the new package is started.
        editor.check_cooked();
        let older = editor.check_ticket.as_ref().unwrap().id;
        editor.cancel_check();
        change_the_level(&mut editor);
        editor.cook();
        settle(&mut editor);
        let package = editor.cooked().unwrap().clone();
        assert_ne!(old_package, package);
        editor.check_cooked();
        let under_way = editor.check_ticket.as_ref().unwrap().id;
        assert_ne!(older, under_way);
        assert_eq!(editor.package_check(), &PackageCheck::Checking);

        // Now the old check's end arrives first, as it would from a core that does not end
        // jobs in the order they were asked for: cancelled, refused, or even passed, for the
        // old package or the new. None of it is about the check under way.
        for found in [
            Err(Diagnostic::new("job.cancelled")),
            Err(Diagnostic::new("payload.hash_mismatch")),
            Ok((old_package.clone(), 7)),
            Ok((package.clone(), 7)),
        ] {
            assert_eq!(editor.check_finished(older, found), None);
            assert_eq!(editor.package_check(), &PackageCheck::Checking);
            assert_eq!(
                editor.check_ticket.as_ref().map(|ticket| ticket.id),
                Some(under_way),
                "the check under way can still be cancelled"
            );
        }
        assert_eq!(editor.problem, None);

        // The check under way ends by itself, with its own result.
        settle(&mut editor);
        assert!(matches!(
            editor.package_check(),
            PackageCheck::Passed { .. }
        ));
        assert!(editor.check_ticket.is_none());
    }

    #[test]
    fn a_check_given_up_for_a_new_cook_says_nothing_about_the_new_package() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = cooked_in(folder.path());
        let old = editor.cooked().unwrap().clone();
        // A check of the old package, then a changed level and a new cook.
        editor.check_cooked();
        let given_up = editor.check_ticket.as_ref().unwrap().id;
        change_the_level(&mut editor);
        editor.cook();
        settle(&mut editor);
        let new = editor.cooked().unwrap().clone();
        assert_ne!(old, new, "the changed level cooks to another package");
        assert_eq!(editor.package_check(), &PackageCheck::NotAsked);
        assert!(editor.check_ticket.is_none());
        // The old check's end, were it to come only now, changes nothing.
        assert_eq!(editor.check_finished(given_up, Ok((old, 7))), None);
        assert_eq!(editor.package_check(), &PackageCheck::NotAsked);
    }

    #[test]
    fn stopping_a_check_leaves_nothing_known_and_nothing_wrong() {
        let folder = tempfile::tempdir().unwrap();
        let mut editor = cooked_in(folder.path());
        editor.check_cooked();
        let under_way = editor.check_ticket.as_ref().unwrap().id;
        editor.cancel_check();
        // As the core reports a job it gave up.
        assert_eq!(
            editor.check_finished(under_way, Err(Diagnostic::new("job.cancelled"))),
            None
        );
        assert_eq!(editor.package_check(), &PackageCheck::NotAsked);
        assert_eq!(editor.problem, None);
        // The job itself still ends in the core; its end is no longer news.
        settle(&mut editor);
        assert_eq!(editor.package_check(), &PackageCheck::NotAsked);
        assert_eq!(editor.problem, None);
    }

    #[test]
    fn ids_made_in_the_same_instant_differ() {
        // Counters as `fresh_id` hands them out, at one reading of the clock.
        let ids: Vec<u128> = (1..=64)
            .map(|counter| id_bits(1_791_000_000_000, counter))
            .collect();
        for (index, id) in ids.iter().enumerate() {
            assert!(
                !ids[..index].contains(id),
                "counter {} repeats an ID",
                index + 1
            );
            assert_ne!(*id, 0);
        }
    }

    /// Pumps until the core owes nothing.
    fn settle(editor: &mut Editor) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            editor.pump();
            if !editor.busy() {
                return;
            }
            assert!(std::time::Instant::now() < end, "the core did not answer");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn a_refused_preview_ends_the_drag_in_the_core_too_and_stays_reported() {
        let mut editor =
            Editor::new(scene_data::current_map(), "level".into(), "cooked".into()).unwrap();
        let object = editor.snapshot().document.objects[10].id.clone();
        editor.select(vec![object.clone()]);
        settle(&mut editor);
        editor.begin_drag();
        settle(&mut editor);
        // A box cannot have a negative size: the core refuses this preview.
        editor.preview_drag(vec![Mutation::BoxExtents {
            object: object.clone(),
            value: [-1.0, 1.0, 1.0],
        }]);
        settle(&mut editor);
        assert!(!editor.dragging());
        assert!(
            editor.snapshot().preview.is_none(),
            "the core still holds the drag"
        );
        // Giving the transaction up succeeded, and that did not put the refusal away.
        assert!(editor.problem.is_some());

        // The core takes the next command: it is not busy with a drag nobody holds.
        editor.rename(object.clone(), "Renamed".into());
        settle(&mut editor);
        assert_eq!(
            editor.problem.as_ref().map(|problem| problem.code.as_str()),
            None
        );
        assert_eq!(
            editor.snapshot().document.object(&object).unwrap().name,
            "Renamed"
        );
    }

    /// A level with a landscape of two layers, in a running editor.
    fn with_two_layers() -> Editor {
        let mut editor =
            Editor::new(scene_data::current_map(), "level".into(), "cooked".into()).unwrap();
        editor.add_landscape(4, 1.0);
        settle(&mut editor);
        editor.add_layer();
        settle(&mut editor);
        editor.add_layer();
        settle(&mut editor);
        assert_eq!(editor.problem, None);
        editor
    }

    fn terrain(editor: &Editor) -> TerrainDocument {
        let snapshot = editor.snapshot();
        landscape(&snapshot).unwrap().1.clone()
    }

    #[test]
    fn the_core_takes_a_grass_rule_and_each_change_made_to_it() {
        let mut editor = with_two_layers();
        let rule = editor.add_grass_rule().unwrap();
        settle(&mut editor);
        assert_eq!(editor.problem, None);
        let now = terrain(&editor);
        assert_eq!(now.rules.len(), 1);
        // It grows on the first layer and declares that it reads it.
        assert_eq!(now.rules[0].density_layer, Some(now.layers[0].id.clone()));
        assert_eq!(now.rules[0].input_layers, vec![now.layers[0].id.clone()]);

        editor.set_rule_setting(&rule, &rules::GRASS_SETTINGS[0], 0.5);
        settle(&mut editor);
        editor.step_rule_layer(&rule, RulePlace::KeptOutBy);
        settle(&mut editor);
        assert_eq!(editor.problem, None);
        let now = terrain(&editor);
        assert_eq!(now.rules[0].parameters.get("density"), Some(&0.5));
        // Kept out by the other layer, never by the one it grows on.
        assert_eq!(
            now.rules[0].exclusion_layers,
            vec![now.layers[1].id.clone()]
        );
        assert_eq!(now.rules[0].input_layers.len(), 2);

        editor.remove_rule(&rule);
        settle(&mut editor);
        assert_eq!(editor.problem, None);
        assert!(terrain(&editor).rules.is_empty());
    }

    #[test]
    fn a_slope_rule_comes_with_its_own_layer_and_goes_when_that_layer_goes() {
        let mut editor = with_two_layers();
        editor.add_slope_rule().unwrap();
        settle(&mut editor);
        assert_eq!(editor.problem, None);
        let now = terrain(&editor);
        assert_eq!((now.layers.len(), now.rules.len()), (3, 1));
        let filled = now.rules[0].output_layer.clone().unwrap();
        let layer = now.layers.iter().find(|layer| layer.id == filled).unwrap();
        assert_eq!(layer.name, "Rock");
        assert_eq!(layer_average(&now, layer), None, "a rule fills it");

        // Grass that this layer keeps out depends on the rule that fills it.
        let grass = editor.add_grass_rule().unwrap();
        settle(&mut editor);
        let mut steps = 0;
        while terrain(&editor)
            .rules
            .iter()
            .find(|rule| rule.id.0 == grass)
            .unwrap()
            .exclusion_layers
            != vec![filled.clone()]
        {
            editor.step_rule_layer(&grass, RulePlace::KeptOutBy);
            settle(&mut editor);
            assert_eq!(editor.problem, None);
            steps += 1;
            assert!(steps < 5, "the layer is never offered");
        }
        let now = terrain(&editor);
        let grass_rule = now.rules.iter().find(|rule| rule.id.0 == grass).unwrap();
        assert_eq!(grass_rule.input_rules.len(), 1);

        // Removing the layer takes its rule and the grass rule's reading of it, in one step.
        editor.remove_layer(&filled.0);
        settle(&mut editor);
        assert_eq!(editor.problem, None);
        let now = terrain(&editor);
        assert_eq!((now.layers.len(), now.rules.len()), (2, 1));
        assert!(now.rules[0].exclusion_layers.is_empty());
        assert!(now.rules[0].input_rules.is_empty());
        editor.undo();
        settle(&mut editor);
        let now = terrain(&editor);
        assert_eq!((now.layers.len(), now.rules.len()), (3, 2));
    }

    #[test]
    fn a_slope_rule_on_a_landscape_without_layers_brings_the_ground_layer_too() {
        let mut editor =
            Editor::new(scene_data::current_map(), "level".into(), "cooked".into()).unwrap();
        editor.add_landscape(4, 1.0);
        settle(&mut editor);
        editor.add_slope_rule().unwrap();
        settle(&mut editor);
        assert_eq!(editor.problem, None);
        let now = terrain(&editor);
        assert_eq!(now.layers.len(), 2);
        assert_eq!(now.rules.len(), 1);
    }

    #[test]
    fn a_new_object_stands_where_the_line_of_sight_meets_the_landscape_or_at_the_fallback() {
        let mut editor =
            Editor::new(scene_data::current_map(), "level".into(), "cooked".into()).unwrap();
        // Two by two tiles of four one-metre cells: the landscape reaches 4 m from the middle.
        editor.add_landscape(4, 1.0);
        settle(&mut editor);
        let fallback = Vec3::new(9.0, 0.0, 9.0);
        let down = |origin: Vec3| PlaceAt {
            origin,
            direction: Vec3::NEG_Y,
            fallback,
        };
        let on = editor
            .place_point(&down(Vec3::new(1.5, 10.0, -2.5)))
            .unwrap();
        assert!(on.distance(Vec3::new(1.5, 0.0, -2.5)) < 1e-4, "{on}");
        // Beside the landscape, and looking away from it: the fallback.
        assert_eq!(
            editor.place_point(&down(Vec3::new(30.0, 10.0, 0.0))),
            Some(fallback)
        );
        let up = PlaceAt {
            direction: Vec3::Y,
            ..down(Vec3::new(1.5, 10.0, -2.5))
        };
        assert_eq!(editor.place_point(&up), Some(fallback));
        // No line of sight at all.
        assert_eq!(
            editor.place_point(&PlaceAt {
                fallback,
                ..default()
            }),
            Some(fallback)
        );
        assert_eq!(editor.problem, None);
    }

    /// An editor on an empty level.
    fn empty_level() -> Editor {
        let document = SceneDocument {
            schema: scene_data::SCHEMA_VERSION,
            id: ObjectId::from_u128(0xd0c),
            objects: Vec::new(),
        };
        Editor::new(document, "level".into(), "cooked".into()).unwrap()
    }

    /// Adds an object through the core and waits for it.
    fn create(
        editor: &mut Editor,
        id: u128,
        parent: Option<u128>,
        local: RigidTransform,
        data: ObjectData,
    ) {
        let order = editor.snapshot().document.objects.len() as u32;
        editor.submit(Command::Edit {
            label_key: "create".into(),
            mutations: vec![Mutation::Create(ObjectRecord {
                id: ObjectId::from_u128(id),
                parent: parent.map(ObjectId::from_u128),
                sibling_order: order,
                name: format!("Object {id}"),
                local,
                data,
            })],
        });
        settle(editor);
        assert_eq!(editor.problem, None);
    }

    /// A flat landscape of four one-metre cells a tile, with the tiles at `keys`.
    fn flat(first_tile: u128, keys: &[(i32, i32)]) -> ObjectData {
        let tiles = keys
            .iter()
            .enumerate()
            .map(|(index, (x, z))| {
                (
                    ObjectId::from_u128(first_tile + index as u128),
                    TileKey { x: *x, z: *z },
                )
            })
            .collect();
        ObjectData::Terrain(TerrainDocument::flat(4, [1.0, 1.0], tiles).unwrap())
    }

    fn at(translation: [f32; 3], rotation: Quat) -> RigidTransform {
        RigidTransform {
            translation_m: translation,
            rotation_xyzw: rotation.to_array(),
        }
    }

    const FALLBACK: Vec3 = Vec3::new(99.0, 0.0, 99.0);

    fn down_at(x: f32, z: f32) -> PlaceAt {
        PlaceAt {
            origin: Vec3::new(x, 50.0, z),
            direction: Vec3::NEG_Y,
            fallback: FALLBACK,
        }
    }

    #[test]
    fn over_a_missing_tile_of_the_landscape_a_new_object_goes_to_the_fallback() {
        let mut editor = empty_level();
        // Two tiles that touch only at a corner: (-1, -1) and (0, 0). The other two are gaps.
        create(
            &mut editor,
            1,
            None,
            RigidTransform::default(),
            flat(100, &[(-1, -1), (0, 0)]),
        );
        let on = editor.place_point(&down_at(2.0, 2.0)).unwrap();
        assert!(on.distance(Vec3::new(2.0, 0.0, 2.0)) < 1e-4, "{on}");
        let on = editor.place_point(&down_at(-2.0, -2.0)).unwrap();
        assert!(on.distance(Vec3::new(-2.0, 0.0, -2.0)) < 1e-4, "{on}");
        // Over each missing tile there is no ground to stand on.
        assert_eq!(editor.place_point(&down_at(2.0, -2.0)), Some(FALLBACK));
        assert_eq!(editor.place_point(&down_at(-2.0, 2.0)), Some(FALLBACK));
        assert_eq!(editor.problem, None);
    }

    #[test]
    fn on_a_landscape_under_a_moved_and_turned_parent_the_object_stands_on_the_world_surface() {
        let mut editor = empty_level();
        // The parent is moved to (10, 2, 0) and tilted 30 degrees about X; the landscape sits
        // one metre up inside it.
        let tilt = Quat::from_rotation_x(30.0_f32.to_radians());
        create(
            &mut editor,
            1,
            None,
            at([10.0, 2.0, 0.0], tilt),
            ObjectData::Group,
        );
        create(
            &mut editor,
            2,
            Some(1),
            at([0.0, 1.0, 0.0], Quat::IDENTITY),
            flat(100, &[(-1, -1), (0, -1), (-1, 0), (0, 0)]),
        );
        // A point of the landscape's own surface, and where the parents put it in the world.
        let local = Vec3::new(0.5, 0.0, 0.577_35);
        let world = tilt * (local + Vec3::Y) + Vec3::new(10.0, 2.0, 0.0);
        let on = editor.place_point(&down_at(world.x, world.z)).unwrap();
        assert!(on.distance(world) < 1e-3, "{on} is not {world}");
        // By hand: x 10.5, z 1.0, and the surface there 2.577 high.
        assert!(on.distance(Vec3::new(10.5, 2.577_35, 1.0)) < 1e-3, "{on}");
        assert_eq!(editor.problem, None);
    }

    #[test]
    fn of_two_landscapes_the_nearer_surface_is_taken_whichever_was_made_first() {
        let low = |editor: &mut Editor| {
            create(
                editor,
                1,
                None,
                RigidTransform::default(),
                flat(100, &[(0, 0)]),
            );
        };
        let high = |editor: &mut Editor| {
            let up = at([0.0, 5.0, 0.0], Quat::IDENTITY);
            create(editor, 2, None, up, flat(200, &[(0, 0)]));
        };
        for low_first in [true, false] {
            let mut editor = empty_level();
            if low_first {
                low(&mut editor);
                high(&mut editor);
            } else {
                high(&mut editor);
                low(&mut editor);
            }
            // From above the upper one is met first, from below the lower one.
            let from_above = editor.place_point(&down_at(2.0, 2.0)).unwrap();
            assert!(
                from_above.distance(Vec3::new(2.0, 5.0, 2.0)) < 1e-4,
                "{from_above}"
            );
            let from_below = PlaceAt {
                origin: Vec3::new(2.0, -50.0, 2.0),
                direction: Vec3::Y,
                fallback: FALLBACK,
            };
            let from_below = editor.place_point(&from_below).unwrap();
            assert!(
                from_below.distance(Vec3::new(2.0, 0.0, 2.0)) < 1e-4,
                "{from_below}"
            );
        }
    }

    /// One stroke of the brush as the view sends it: begun, shown once, committed.
    fn stroke(editor: &mut Editor, label: &'static str, show: impl FnOnce(&mut Editor)) {
        editor.begin_stroke(label);
        settle(editor);
        show(editor);
        settle(editor);
        editor.end_drag(true);
        settle(editor);
        assert_eq!(editor.problem, None, "{label}");
    }

    fn raise(editor: &mut Editor, object: &ObjectId, path: &[Vec3]) {
        let request = crate::tool::Brush::default().stroke(object.clone(), path, 0.0);
        stroke(editor, "sculpt", |editor| editor.preview_sculpt(request));
    }

    fn paint(editor: &mut Editor, object: &ObjectId, layer: &ObjectId, path: &[Vec3]) {
        let request =
            crate::tool::Brush::default().paint_stroke(object.clone(), layer.clone(), path, false);
        stroke(editor, "paint", |editor| editor.preview_paint(request));
    }

    #[test]
    fn a_sculpted_painted_landscape_with_rules_survives_save_reopen_and_cooks_the_same() {
        let folder = tempfile::tempdir().unwrap();
        let level = folder.path().join("levels/hill.level.json");
        let package = folder.path().join("cooked");
        let mut editor =
            Editor::new(scene_data::current_map(), level.clone(), package.clone()).unwrap();
        editor.add_landscape(8, 1.0);
        settle(&mut editor);
        let object = landscape(&editor.snapshot()).unwrap().0.id.clone();
        raise(
            &mut editor,
            &object,
            &[Vec3::new(1.3, 0.0, 2.2), Vec3::new(3.1, 0.0, 2.4)],
        );
        editor.add_layer();
        settle(&mut editor);
        let dirt = StableId(editor.add_layer().unwrap());
        settle(&mut editor);
        paint(
            &mut editor,
            &object,
            &dirt,
            &[Vec3::new(-3.3, 0.0, -1.2), Vec3::new(2.4, 0.0, 3.1)],
        );
        editor.add_grass_rule().unwrap();
        settle(&mut editor);
        editor.add_slope_rule().unwrap();
        settle(&mut editor);
        assert_eq!(editor.problem, None);
        let authored = terrain(&editor);
        assert!(
            authored.samples.iter().any(|sample| sample.height_m > 0.0),
            "nothing sculpted"
        );
        assert_eq!((authored.layers.len(), authored.rules.len()), (3, 2));
        assert!(
            layer_average(&authored, &authored.layers[1]).unwrap() > 0.0,
            "nothing painted"
        );

        editor.save();
        settle(&mut editor);
        editor.cook();
        settle(&mut editor);
        assert_eq!(editor.problem, None);
        assert!(!editor.snapshot().status.dirty);
        let first_cook = editor.last_cook.clone().expect("a cook");

        // A fresh editor, as at the next start: it opens the file and holds the same level,
        // object for object and bit for bit.
        let mut reopened =
            Editor::new(scene_data::current_map(), level.clone(), package.clone()).unwrap();
        reopened.open(level.clone());
        settle(&mut reopened);
        assert_eq!(reopened.problem, None);
        assert!(reopened.has_file());
        assert!(!reopened.snapshot().status.dirty);
        assert_eq!(*reopened.snapshot().document, *editor.snapshot().document);

        // Cooked again, from the reopened level and once more: the same source, the same result.
        for _ in 0..2 {
            reopened.cook();
            settle(&mut reopened);
            assert_eq!(reopened.problem, None);
            assert_eq!(reopened.last_cook, Some(first_cook.clone()));
        }

        // More paint changes the level and its picture, not what can be walked on.
        paint(
            &mut reopened,
            &object,
            &dirt,
            &[Vec3::new(3.2, 0.0, -3.4), Vec3::new(-2.1, 0.0, 3.3)],
        );
        assert!(reopened.snapshot().status.dirty);
        reopened.save();
        settle(&mut reopened);
        reopened.cook();
        settle(&mut reopened);
        let (source, simulation) = reopened.last_cook.clone().expect("a cook");
        assert_ne!(
            source, first_cook.0,
            "the paint did not reach the level's source"
        );
        assert_eq!(
            simulation, first_cook.1,
            "paint changed what can be walked on"
        );

        // A change of height does change it.
        raise(&mut reopened, &object, &[Vec3::new(-4.2, 0.0, -4.4)]);
        reopened.save();
        settle(&mut reopened);
        reopened.cook();
        settle(&mut reopened);
        assert_ne!(reopened.last_cook.clone().expect("a cook").1, first_cook.1);
    }

    #[test]
    fn a_geometry_job_a_newer_one_replaced_is_not_a_problem() {
        assert!(superseded(JobKind::Geometry, "job.cancelled"));
        assert!(superseded(JobKind::Geometry, "job.stale"));
    }

    #[test]
    fn a_geometry_job_that_failed_and_a_save_that_was_cancelled_are_problems() {
        assert!(!superseded(JobKind::Geometry, "cook.degenerate_triangle"));
        assert!(!superseded(JobKind::Save, "job.cancelled"));
        assert!(!superseded(JobKind::Save, "job.stale"));
    }
}
