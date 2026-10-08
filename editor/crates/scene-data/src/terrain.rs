//! Tiled source authority. All border/corner samples have a single global key.
mod surface;
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub use surface::{cell_triangles, sample_world_position};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TileKey {
    pub x: i32,
    pub z: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SampleKey {
    pub x: i64,
    pub z: i64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirtyRegion {
    pub object: ObjectId,
    pub tile: TileId,
    pub min_xz: [u32; 2],
    pub max_xz_exclusive: [u32; 2],
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeightSample {
    pub key: SampleKey,
    pub height_m: f32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainTile {
    pub id: TileId,
    pub key: TileKey,
    pub layers: Vec<TileLayer>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TileLayer {
    pub layer: LayerId,
    pub side_samples: u32,
    pub weights: Vec<f32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
pub enum LayerKind {
    BlendWeight {
        group: StableId,
        default_weight: f32,
    },
    Mask,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerDefinition {
    pub id: LayerId,
    pub name: String,
    pub kind: LayerKind,
    #[serde(default)]
    pub color_linear_rgba: Option<[f32; 4]>,
}
// Only registered visual semantics exist in this slice. No serialized/UI role toggle.
impl LayerDefinition {
    pub fn affects_simulation(&self) -> bool {
        false
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleDefinition {
    pub id: RuleId,
    pub algorithm: String,
    pub version: u32,
    pub seed: u64,
    pub input_layers: Vec<LayerId>,
    pub input_rules: Vec<RuleId>,
    pub output_layer: Option<LayerId>,
    #[serde(default)]
    pub density_layer: Option<LayerId>,
    #[serde(default)]
    pub exclusion_layers: Vec<LayerId>,
    pub parameters: BTreeMap<String, f32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TerrainDocument {
    pub cells_per_tile: u32,
    pub spacing_xz_m: [f32; 2],
    pub tiles: Vec<TerrainTile>,
    pub samples: Vec<HeightSample>,
    pub layers: Vec<LayerDefinition>,
    pub rules: Vec<RuleDefinition>,
    #[serde(default)]
    pub surface_group: Option<StableId>,
}
impl TerrainDocument {
    pub fn flat(
        cells_per_tile: u32,
        spacing_xz_m: [f32; 2],
        tiles: Vec<(TileId, TileKey)>,
    ) -> Result<Self, Diagnostic> {
        if !(1..=1024).contains(&cells_per_tile)
            || (cells_per_tile as u64 + 1).pow(2) * tiles.len() as u64 > 2_000_000
        {
            return Err(Diagnostic::new("terrain.invalid"));
        }
        let mut t = Self {
            cells_per_tile,
            spacing_xz_m,
            tiles: tiles
                .into_iter()
                .map(|(id, key)| TerrainTile {
                    id,
                    key,
                    layers: Vec::new(),
                })
                .collect(),
            samples: Vec::new(),
            layers: Vec::new(),
            rules: Vec::new(),
            surface_group: None,
        };
        let mut keys = BTreeSet::new();
        for tile in &t.tiles {
            for z in 0..=cells_per_tile {
                for x in 0..=cells_per_tile {
                    keys.insert(t.sample_key(tile.key, x, z));
                }
            }
        }
        t.samples = keys
            .into_iter()
            .map(|key| HeightSample { key, height_m: 0.0 })
            .collect();
        validate(&t)?;
        Ok(t)
    }
    pub fn sample_key(&self, tile: TileKey, x: u32, z: u32) -> SampleKey {
        SampleKey {
            x: tile.x as i64 * self.cells_per_tile as i64 + x as i64,
            z: tile.z as i64 * self.cells_per_tile as i64 + z as i64,
        }
    }
    pub fn heights(&self) -> BTreeMap<SampleKey, f32> {
        self.samples.iter().map(|s| (s.key, s.height_m)).collect()
    }
}
pub fn validate(t: &TerrainDocument) -> Result<(), Diagnostic> {
    let bad = || Diagnostic::new("terrain.invalid");
    if !(1..=1024).contains(&t.cells_per_tile)
        || t.tiles.len() > 4096
        || t.samples.len() > 1_000_000
        || (t.cells_per_tile as u64 + 1).pow(2) * t.tiles.len() as u64 > 2_000_000
        || !t.spacing_xz_m.iter().all(|v| v.is_finite() && *v > 0.0)
    {
        return Err(bad());
    }
    let mut tile_ids = BTreeSet::new();
    let mut tile_keys = BTreeSet::new();
    let mut samples = BTreeMap::new();
    for s in &t.samples {
        if !s.height_m.is_finite() || samples.insert(s.key, s.height_m).is_some() {
            return Err(Diagnostic::new("terrain.sample_invalid_or_duplicate"));
        }
    }
    crate::layers::validate_layers(&t.layers, &t.tiles, t.surface_group.as_ref())?;
    for tile in &t.tiles {
        if !tile.id.valid() || !tile_ids.insert(&tile.id) || !tile_keys.insert(tile.key) {
            return Err(Diagnostic::new("terrain.tile_duplicate"));
        }
        for z in 0..=t.cells_per_tile {
            for x in 0..=t.cells_per_tile {
                if !samples.contains_key(&t.sample_key(tile.key, x, z)) {
                    return Err(Diagnostic::new("terrain.sample_missing"));
                }
            }
        }
    }
    crate::rules::validate_rules(
        &t.rules,
        &t.layers,
        t.cells_per_tile as u64 * t.cells_per_tile as u64 * t.tiles.len() as u64,
    )?;
    Ok(())
}
