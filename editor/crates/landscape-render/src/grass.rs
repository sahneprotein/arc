//! Bounded world-space crossed blades; decoration never supplies collision geometry.
use crate::*;
use glam::{Quat, Vec3};

pub const GRASS_LINEAR_RGBA: [f32; 4] = [0.12, 0.30, 0.055, 1.0];
#[derive(Clone, Debug, PartialEq)]
pub struct GrassMesh {
    pub object: ObjectId,
    pub tile: TileId,
    pub instances: usize,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct GrassUpdateFacts {
    pub cpu_allocated_instances: usize,
    pub tiles_rebuilt: usize,
    pub tiles_removed: usize,
    pub prepared_bytes: usize,
    pub preparation_us: u64,
}
#[derive(Default)]
pub struct GrassCache {
    tiles: BTreeMap<(ObjectId, TileId), Arc<GrassMesh>>,
    source: BTreeMap<(ObjectId, TileId), Vec<DecorationInstance>>,
}
impl GrassCache {
    pub fn tiles(&self) -> &BTreeMap<(ObjectId, TileId), Arc<GrassMesh>> {
        &self.tiles
    }
    pub fn update(
        &mut self,
        visual: &VisualPayload,
        ground: &[CookedGround],
    ) -> Result<GrassUpdateFacts, Diagnostic> {
        let start = Instant::now();
        // Validate before touching the accepted view. Validation bounds total instances.
        SimulationPayload {
            schema: COOK_SCHEMA,
            profile: COLLISION_PROFILE.into(),
            boxes: Vec::new(),
            spawns: Vec::new(),
            ground: ground.to_vec(),
        }
        .validate()?;
        visual.validate(ground)?;
        let mut grouped: BTreeMap<_, Vec<DecorationInstance>> = BTreeMap::new();
        for terrain in &visual.terrain {
            for instance in &terrain.decoration {
                grouped
                    .entry((terrain.object.clone(), instance.tile.clone()))
                    .or_default()
                    .push(instance.clone());
            }
        }
        for instances in grouped.values_mut() {
            instances.sort_by(|a, b| a.id.cmp(&b.id));
        }
        let mut facts = GrassUpdateFacts {
            cpu_allocated_instances: grouped.values().map(Vec::len).sum(),
            ..Default::default()
        };
        let mut next_tiles = BTreeMap::new();
        for (key, instances) in &grouped {
            let data = if self.source.get(key) == Some(instances) {
                self.tiles[key].clone()
            } else {
                let mesh = blades(key, instances)?;
                facts.tiles_rebuilt += 1;
                facts.prepared_bytes += mesh.positions.len() * 24 + mesh.indices.len() * 4;
                Arc::new(mesh)
            };
            next_tiles.insert(key.clone(), data);
        }
        facts.tiles_removed = self
            .tiles
            .keys()
            .filter(|key| !next_tiles.contains_key(*key))
            .count();
        self.source = grouped;
        self.tiles = next_tiles;
        facts.preparation_us = start.elapsed().as_micros().min(u64::MAX as u128) as u64;
        Ok(facts)
    }
}
fn blades(
    key: &(ObjectId, TileId),
    instances: &[DecorationInstance],
) -> Result<GrassMesh, Diagnostic> {
    let mut mesh = GrassMesh {
        object: key.0.clone(),
        tile: key.1.clone(),
        instances: instances.len(),
        positions: Vec::with_capacity(instances.len() * 8),
        normals: Vec::with_capacity(instances.len() * 8),
        indices: Vec::with_capacity(instances.len() * 12),
    };
    for instance in instances {
        let center = Vec3::from_array(instance.position_m);
        let rotation = Quat::from_rotation_y(
            f32::from(instance.yaw_turns) * (std::f32::consts::TAU / 65536.0),
        );
        for axis in [Vec3::X, Vec3::Z] {
            let across = rotation * axis;
            let bottom = across * (instance.width_m * 0.5);
            let top = bottom * 0.25;
            let up = Vec3::Y * instance.height_m;
            let normal = across.cross(Vec3::Y).to_array();
            let base = mesh.positions.len() as u32;
            for point in [
                center - bottom,
                center + bottom,
                center + up - top,
                center + up + top,
            ] {
                if !point.is_finite() {
                    return Err(Diagnostic::new("render.grass_position"));
                }
                mesh.positions.push(point.to_array());
                mesh.normals.push(normal);
            }
            mesh.indices.extend_from_slice(&[
                base,
                base + 1,
                base + 2,
                base + 1,
                base + 3,
                base + 2,
            ]);
        }
    }
    Ok(mesh)
}
