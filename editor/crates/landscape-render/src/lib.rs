//! Shared ground geometry/material preparation. No camera, picking or input ownership.
#[cfg(feature = "bevy-render")]
mod bevy_render;
#[cfg(feature = "bevy-render")]
pub use bevy_render::*;
mod decoration;
mod grass;
mod layers;
pub use decoration::*;
pub use grass::*;
#[cfg(feature = "bevy-render")]
mod grass_render;
#[cfg(feature = "bevy-render")]
pub use grass_render::*;
use scene_data::{
    terrain::{DirtyRegion, TileKey},
    *,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Instant,
};

pub const DEFAULT_GROUND_LINEAR_RGBA: [f32; 4] = [0.18, 0.24, 0.12, 1.0];
#[derive(Clone, Debug, PartialEq)]
pub struct GroundMesh {
    pub object: ObjectId,
    pub tile: TileId,
    pub key: TileKey,
    pub cells_per_tile: u32,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    pub uv: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct UpdateFacts {
    pub tiles_examined: usize,
    pub tiles_rebuilt: usize,
    pub tiles_removed: usize,
    /// CPU bytes prepared for upload; does NOT claim observed GPU allocation/write volume.
    pub prepared_bytes: usize,
    pub preparation_us: u64,
}
#[derive(Default)]
pub struct GroundCache {
    tiles: BTreeMap<(ObjectId, TileId), Arc<GroundMesh>>,
    full_detail: bool,
}
impl GroundCache {
    pub fn tiles(&self) -> &BTreeMap<(ObjectId, TileId), Arc<GroundMesh>> {
        &self.tiles
    }
    /// Explicit detail override; L1 already has one detail level, so geometry is unchanged.
    pub fn set_full_detail(&mut self, value: bool) {
        self.full_detail = value;
    }
    pub fn full_detail(&self) -> bool {
        self.full_detail
    }
    /// Conservatively compares cooked tiles and rebuilds complete changed CPU buffers.
    /// Dirty inputs describe previews/halos; GPU subrange updates are not promised.
    pub fn update(
        &mut self,
        ground: &[CookedGround],
        dirty: &[DirtyRegion],
    ) -> Result<UpdateFacts, Diagnostic> {
        self.update_with_visual(ground, None, dirty)
    }
    pub fn update_with_visual(
        &mut self,
        ground: &[CookedGround],
        visual: Option<&VisualPayload>,
        dirty: &[DirtyRegion],
    ) -> Result<UpdateFacts, Diagnostic> {
        let start = Instant::now();
        SimulationPayload {
            schema: COOK_SCHEMA,
            profile: COLLISION_PROFILE.into(),
            boxes: Vec::new(),
            spawns: Vec::new(),
            ground: ground.to_vec(),
        }
        .validate()?;
        if let Some(visual) = visual {
            visual.validate(ground)?;
        }
        for region in dirty {
            let tile = ground
                .iter()
                .find(|g| g.object == region.object && g.tile == region.tile)
                .ok_or_else(|| Diagnostic::new("render.dirty_tile_missing"))?;
            if (0..2).any(|i| {
                region.min_xz[i] >= region.max_xz_exclusive[i]
                    || region.max_xz_exclusive[i] > tile.cells_per_tile + 1
            }) {
                return Err(Diagnostic::new("render.dirty_bounds"));
            }
        }
        let mut facts = UpdateFacts {
            tiles_examined: ground.len(),
            ..Default::default()
        };
        let keys: BTreeSet<_> = ground
            .iter()
            .map(|g| (g.object.clone(), g.tile.clone()))
            .collect();
        let before = self.tiles.len();
        self.tiles.retain(|key, _| keys.contains(key));
        facts.tiles_removed = before - self.tiles.len();
        for g in ground {
            let key = (g.object.clone(), g.tile.clone());
            let colors = layers::colors(g, visual);
            let unchanged = self.tiles.get(&key).is_some_and(|old| {
                old.key == g.key
                    && old.cells_per_tile == g.cells_per_tile
                    && old.positions == g.positions_m
                    && old.normals == g.normals
                    && old.colors == colors
                    && old
                        .indices
                        .iter()
                        .copied()
                        .eq(g.triangles.iter().flatten().copied())
            });
            if unchanged {
                continue;
            }
            let n = g.cells_per_tile as usize;
            let uv = (0..g.positions_m.len())
                .map(|i| {
                    [
                        (i % (n + 1)) as f32 / n as f32,
                        (i / (n + 1)) as f32 / n as f32,
                    ]
                })
                .collect();
            let mesh = GroundMesh {
                object: g.object.clone(),
                tile: g.tile.clone(),
                key: g.key,
                cells_per_tile: g.cells_per_tile,
                positions: g.positions_m.clone(),
                normals: g.normals.clone(),
                colors,
                uv,
                indices: g.triangles.iter().flatten().copied().collect(),
            };
            facts.tiles_rebuilt += 1;
            facts.prepared_bytes += mesh.positions.len() * 12
                + mesh.normals.len() * 12
                + mesh.uv.len() * 8
                + mesh.indices.len() * 4
                + mesh.colors.len() * 16;
            self.tiles.insert(key, Arc::new(mesh));
        }
        facts.preparation_us = start.elapsed().as_micros().min(u64::MAX as u128) as u64;
        Ok(facts)
    }
}
