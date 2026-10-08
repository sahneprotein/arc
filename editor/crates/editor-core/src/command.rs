use scene_data::terrain::DirtyRegion;
use scene_data::terrain::SampleKey;
use scene_data::*;
use std::sync::Arc;

pub type Revision = u64;
pub type RequestId = u64;
pub type TransactionId = u64;
#[derive(Clone, Debug, PartialEq)]
pub enum TerrainSculptMode {
    Raise { amount_m: f32 },
    Lower { amount_m: f32 },
    Smooth,
    Flatten { height_m: f32 },
}
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainSculptRequest {
    pub object: ObjectId,
    pub path_world_m: Vec<[f32; 3]>,
    pub radius_m: f32,
    pub strength: f32,
    /// Fraction of the radius used for the soft quadratic edge; 0 is hard edged, 1 spans the radius.
    pub falloff: f32,
    pub mode: TerrainSculptMode,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainSculptSample {
    pub key: SampleKey,
    pub before_m: f32,
    pub after_m: f32,
    pub weight: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainSculptResult {
    pub object: ObjectId,
    pub sample_deltas: Arc<[TerrainSculptSample]>,
    pub dirty_regions: Arc<[DirtyRegion]>,
}
impl TerrainSculptResult {
    pub fn mutation(&self) -> Mutation {
        Mutation::TerrainSamples {
            object: self.object.clone(),
            values: self
                .sample_deltas
                .iter()
                .map(|delta| (delta.key, delta.after_m))
                .collect(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainPaintMode {
    Paint,
    Erase,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainPaintRequest {
    pub object: ObjectId,
    pub layer: scene_data::LayerId,
    pub path_world_m: Vec<[f32; 3]>,
    pub radius_m: f32,
    pub strength: f32,
    pub falloff: f32,
    pub mode: TerrainPaintMode,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainPaintTexel {
    pub tile: scene_data::TileId,
    pub layer: scene_data::LayerId,
    pub index: u32,
    pub before: f32,
    pub after: f32,
    pub weight: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainPaintResult {
    pub object: ObjectId,
    pub layer: scene_data::LayerId,
    pub texel_deltas: Arc<[TerrainPaintTexel]>,
    pub dirty_regions: Arc<[DirtyRegion]>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CommandRequest {
    pub request: RequestId,
    pub session: u64,
    pub expected_revision: Revision,
    pub expected_inspector_revision: Option<Revision>,
    pub command: Command,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Edit {
        label_key: String,
        mutations: Vec<Mutation>,
    },
    Select(Vec<ObjectId>),
    Undo,
    Redo,
    BeginViewportEdit {
        selection_revision: Revision,
    },
    Preview {
        transaction: TransactionId,
        sequence: u64,
        mutations: Vec<Mutation>,
    },
    PreviewTerrainSculpt {
        transaction: TransactionId,
        sequence: u64,
        request: TerrainSculptRequest,
    },
    PreviewTerrainPaint {
        transaction: TransactionId,
        sequence: u64,
        request: TerrainPaintRequest,
    },
    CommitViewportEdit {
        transaction: TransactionId,
        label_key: String,
    },
    CancelViewportEdit {
        transaction: TransactionId,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum Mutation {
    Create(ObjectRecord),
    Delete(ObjectId),
    Rename {
        object: ObjectId,
        name: String,
    },
    Transform {
        object: ObjectId,
        value: RigidTransform,
    },
    BoxExtents {
        object: ObjectId,
        value: [f32; 3],
    },
    Reparent {
        object: ObjectId,
        parent: Option<ObjectId>,
        order: u32,
    },
    // One mutation touches shared global samples; affected tiles derive dirtiness.
    TerrainSamples {
        object: ObjectId,
        values: Vec<(SampleKey, f32)>,
    },
    TerrainRules {
        object: ObjectId,
        rules: Vec<scene_data::terrain::RuleDefinition>,
    },
    TerrainRegistry {
        object: ObjectId,
        layers: Vec<scene_data::terrain::LayerDefinition>,
        surface_group: Option<StableId>,
        side_samples: u32,
    },
    /// Explicit resample of every authored layer; ordinary registry edits preserve resolution.
    TerrainLayerResolution {
        object: ObjectId,
        side_samples: u32,
    },
    TerrainLayer {
        object: ObjectId,
        tile: TileId,
        layer: LayerId,
        values: Vec<(usize, f32)>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct CommandResult {
    pub request: RequestId,
    pub revision: Revision,
    pub outcome: Result<CommandOutcome, Diagnostic>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum CommandOutcome {
    Applied,
    SelectionChanged,
    TransactionStarted(TransactionId),
    Previewed,
    TerrainSculpted {
        sequence: u64,
        result: TerrainSculptResult,
    },
    TerrainPainted {
        sequence: u64,
        result: TerrainPaintResult,
    },
    Cancelled,
}

pub(crate) fn mutate(scene: &mut SceneDocument, mutations: &[Mutation]) -> Result<(), Diagnostic> {
    for mutation in mutations {
        if let Mutation::Create(o) = mutation {
            scene.objects.push(o.clone());
            continue;
        }
        if let Mutation::Delete(id) = mutation {
            let before = scene.objects.len();
            scene.objects.retain(|o| &o.id != id);
            if before == scene.objects.len() {
                return Err(Diagnostic::new("object.missing").at(id));
            }
            continue;
        }
        let id = match mutation {
            Mutation::Rename { object, .. }
            | Mutation::Transform { object, .. }
            | Mutation::BoxExtents { object, .. }
            | Mutation::Reparent { object, .. }
            | Mutation::TerrainSamples { object, .. }
            | Mutation::TerrainRules { object, .. }
            | Mutation::TerrainRegistry { object, .. }
            | Mutation::TerrainLayer { object, .. }
            | Mutation::TerrainLayerResolution { object, .. } => object,
            _ => unreachable!(),
        };
        let o = scene
            .object_mut(id)
            .ok_or_else(|| Diagnostic::new("object.missing").at(id))?;
        match mutation {
            Mutation::Rename { name, .. } => o.name = name.clone(),
            Mutation::Transform { value, .. } => o.local = value.clone(),
            Mutation::BoxExtents { value, .. } => match &mut o.data {
                ObjectData::Box { half_extents_m } => *half_extents_m = *value,
                _ => return Err(Diagnostic::new("field.kind").at(id)),
            },
            Mutation::Reparent { parent, order, .. } => {
                o.parent = parent.clone();
                o.sibling_order = *order;
            }
            Mutation::TerrainSamples { values, .. } => match &mut o.data {
                ObjectData::Terrain(t) => {
                    // Apply a stroke in one source pass rather than one linear search per delta.
                    let mut pending: std::collections::BTreeMap<_, _> =
                        values.iter().copied().collect();
                    for sample in &mut t.samples {
                        if let Some(value) = pending.remove(&sample.key) {
                            sample.height_m = value;
                        }
                    }
                    if !pending.is_empty() {
                        return Err(Diagnostic::new("terrain.sample_missing").at(id));
                    }
                }
                _ => return Err(Diagnostic::new("field.kind").at(id)),
            },
            Mutation::TerrainRules { rules, .. } => match &mut o.data {
                ObjectData::Terrain(t) => t.rules = rules.clone(),
                _ => return Err(Diagnostic::new("field.kind").at(id)),
            },
            Mutation::TerrainRegistry {
                layers,
                surface_group,
                side_samples,
                ..
            } => match &mut o.data {
                ObjectData::Terrain(t) => scene_data::layers::set_registry(
                    t,
                    layers.clone(),
                    surface_group.clone(),
                    *side_samples,
                )?,
                _ => return Err(Diagnostic::new("field.kind").at(id)),
            },
            Mutation::TerrainLayerResolution { side_samples, .. } => match &mut o.data {
                ObjectData::Terrain(t) => scene_data::layers::resample_layers(t, *side_samples)?,
                _ => return Err(Diagnostic::new("field.kind").at(id)),
            },
            Mutation::TerrainLayer {
                tile,
                layer,
                values,
                ..
            } => match &mut o.data {
                ObjectData::Terrain(t) => scene_data::layers::paint(t, tile, layer, values)?,
                _ => return Err(Diagnostic::new("field.kind").at(id)),
            },
            _ => unreachable!(),
        }
    }
    scene_data::validate(scene)
}
