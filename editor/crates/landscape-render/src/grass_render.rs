//! Shared mesh/material instances use pinned Bevy's ordinary automatic batching.
use crate::*;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

#[derive(Component, Clone, Debug)]
pub struct GrassInstance {
    pub object: ObjectId,
    pub instance: StableId,
    pub tile: TileId,
    pub rule: RuleId,
}
/// One unit-size crossed blade mesh shared by every derived instance in the view.
pub fn grass_mesh() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-0.5, 0.0, 0.0],
            [0.5, 0.0, 0.0],
            [-0.125, 1.0, 0.0],
            [0.125, 1.0, 0.0],
            [0.0, 0.0, -0.5],
            [0.0, 0.0, 0.5],
            [0.0, 1.0, -0.125],
            [0.0, 1.0, 0.125],
        ],
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vec![
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
        ],
    )
    .with_inserted_indices(Indices::U32(vec![0, 1, 2, 1, 3, 2, 4, 5, 6, 5, 7, 6]))
}
pub fn grass_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::linear_rgba(
            GRASS_LINEAR_RGBA[0],
            GRASS_LINEAR_RGBA[1],
            GRASS_LINEAR_RGBA[2],
            GRASS_LINEAR_RGBA[3],
        ),
        perceptual_roughness: 0.95,
        cull_mode: None,
        double_sided: true,
        ..Default::default()
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct GrassSubmissionFacts {
    pub cpu_allocated_instances: usize,
    pub instances_spawned: usize,
    pub instances_updated: usize,
    pub instances_removed: usize,
    /// CPU transform data prepared; not observed GPU writes or allocations.
    pub prepared_transform_bytes: usize,
    pub preparation_us: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct GrassRenderFacts {
    pub update: GrassSubmissionFacts,
    pub enabled: bool,
    /// CPU instances represented by visible scene entities after deferred Commands apply.
    /// This is not an observed render-thread submission or GPU visibility count.
    pub scene_instances: usize,
    pub gpu_submitted_instances: Option<usize>,
    pub gpu_visible_instances: Option<usize>,
}
#[derive(Resource)]
pub struct GrassRenderer {
    instances: BTreeMap<(ObjectId, StableId), (Entity, DecorationInstance)>,
    mesh: Option<Handle<Mesh>>,
    material: Option<Handle<StandardMaterial>>,
    enabled: bool,
    last_update: GrassSubmissionFacts,
}
impl Default for GrassRenderer {
    fn default() -> Self {
        Self {
            instances: BTreeMap::new(),
            mesh: None,
            material: None,
            enabled: true,
            last_update: GrassSubmissionFacts::default(),
        }
    }
}
impl GrassRenderer {
    pub fn set_enabled(&mut self, value: bool, commands: &mut Commands) {
        self.enabled = value;
        for (entity, _) in self.instances.values() {
            commands.entity(*entity).insert(self.visibility());
        }
    }
    pub fn facts(&self) -> GrassRenderFacts {
        GrassRenderFacts {
            update: self.last_update,
            enabled: self.enabled,
            scene_instances: if self.enabled {
                self.instances.len()
            } else {
                0
            },
            gpu_submitted_instances: None,
            gpu_visible_instances: None,
        }
    }
    fn visibility(&self) -> Visibility {
        if self.enabled {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        }
    }
    pub fn sync(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        visual: &VisualPayload,
        ground: &[CookedGround],
    ) -> Result<GrassRenderFacts, Diagnostic> {
        let start = Instant::now();
        SimulationPayload {
            schema: COOK_SCHEMA,
            profile: COLLISION_PROFILE.into(),
            boxes: Vec::new(),
            spawns: Vec::new(),
            ground: ground.to_vec(),
        }
        .validate()?;
        visual.validate(ground)?;
        let next: BTreeMap<_, _> = visual
            .terrain
            .iter()
            .flat_map(|terrain| {
                terrain
                    .decoration
                    .iter()
                    .map(|d| ((terrain.object.clone(), d.id.clone()), d))
            })
            .collect();
        let mut update = GrassSubmissionFacts {
            cpu_allocated_instances: next.len(),
            ..Default::default()
        };
        self.instances.retain(|key, (entity, _)| {
            if next.contains_key(key) {
                true
            } else {
                commands.entity(*entity).despawn();
                update.instances_removed += 1;
                false
            }
        });
        if next.is_empty() {
            if let Some(mesh) = self.mesh.take() {
                meshes.remove(mesh.id());
            }
        } else {
            let replace_mesh = self
                .mesh
                .as_ref()
                .is_none_or(|h| meshes.get(h.id()).is_none());
            if replace_mesh {
                self.mesh = Some(meshes.add(grass_mesh()));
            }
            let mesh = self.mesh.as_ref().unwrap().clone();
            let material = self
                .material
                .get_or_insert_with(|| materials.add(grass_material()))
                .clone();
            let visibility = self.visibility();
            for (key, data) in next {
                let identity = GrassInstance {
                    object: key.0.clone(),
                    instance: data.id.clone(),
                    tile: data.tile.clone(),
                    rule: data.rule.clone(),
                };
                if let Some((entity, previous)) = self.instances.get_mut(&key) {
                    if previous != data {
                        commands.entity(*entity).insert((transform(data), identity));
                        *previous = data.clone();
                        update.instances_updated += 1;
                    }
                    if replace_mesh {
                        commands.entity(*entity).insert(Mesh3d(mesh.clone()));
                    }
                } else {
                    let entity = commands
                        .spawn((
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(material.clone()),
                            transform(data),
                            visibility,
                            identity,
                        ))
                        .id();
                    self.instances.insert(key, (entity, data.clone()));
                    update.instances_spawned += 1;
                }
            }
        }
        update.prepared_transform_bytes = (update.instances_spawned + update.instances_updated)
            * std::mem::size_of::<Transform>();
        update.preparation_us = start.elapsed().as_micros().min(u64::MAX as u128) as u64;
        self.last_update = update;
        Ok(self.facts())
    }
}
fn transform(data: &DecorationInstance) -> Transform {
    Transform {
        translation: Vec3::from_array(data.position_m),
        rotation: Quat::from_rotation_y(
            f32::from(data.yaw_turns) * (std::f32::consts::TAU / 65536.0),
        ),
        scale: Vec3::new(data.width_m, data.height_m, data.width_m),
    }
}
