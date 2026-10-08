//! Base-relative authored paint with canonical shared texels and no event stacking.
use crate::*;
use scene_data::{layers::default_weight, terrain::*, LayerId, ObjectData};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub const MAX_PAINT_POINTS: usize = MAX_SCULPT_POINTS;
pub const MAX_PAINT_RADIUS_M: f32 = MAX_SCULPT_RADIUS_M;
pub const MAX_PAINT_CHANGED_TEXELS: usize = 65_536;
const MAX_RECT_VISITS: u64 = 1_000_000;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Rational {
    numerator: i64,
    denominator: u32,
}
impl Rational {
    fn new(tile: i32, index: u32, intervals: u32) -> Self {
        let numerator = i64::from(tile) * i64::from(intervals) + i64::from(index);
        let mut a = numerator.unsigned_abs();
        let mut b = u64::from(intervals);
        while b != 0 {
            let next = a % b;
            a = b;
            b = next;
        }
        Self {
            numerator: numerator / a as i64,
            denominator: intervals / a as u32,
        }
    }
    fn meters(self, cells: u32, spacing: f32) -> f64 {
        // Integer height-grid samples reproduce sculpt/cooker f32 multiplication.
        f64::from(
            (self.numerator as f64 / f64::from(self.denominator) * f64::from(cells)) as f32
                * spacing,
        )
    }
}
struct Site {
    owner: usize,
    index: u32,
    weight: f32,
    copies: BTreeSet<(usize, u32)>,
}

pub fn terrain_paint(
    scene: &SceneDocument,
    request: &TerrainPaintRequest,
) -> Result<TerrainPaintResult, Diagnostic> {
    evaluate_terrain_paint(scene, request).map(|(result, _)| result)
}
pub(crate) fn evaluate_terrain_paint(
    scene: &SceneDocument,
    r: &TerrainPaintRequest,
) -> Result<(TerrainPaintResult, Option<SceneDocument>), Diagnostic> {
    scene_data::validate(scene)?;
    crate::brush::validate_path(
        &r.object,
        &r.path_world_m,
        r.radius_m,
        r.strength,
        r.falloff,
    )?;
    let object = scene
        .object(&r.object)
        .ok_or_else(|| Diagnostic::new("object.missing").at(&r.object))?;
    let ObjectData::Terrain(t) = &object.data else {
        return Err(Diagnostic::new("field.kind").at(&r.object));
    };
    let selected = t
        .layers
        .iter()
        .find(|l| l.id == r.layer)
        .ok_or_else(|| Diagnostic::new("layer.missing").at(&r.layer))?;
    let derived: BTreeSet<_> = t
        .rules
        .iter()
        .filter_map(|rule| rule.output_layer.as_ref())
        .collect();
    if derived.contains(&r.layer) {
        return Err(Diagnostic::new("layer.derived_output").at(&r.layer));
    }
    let mut members:Vec<_>=t.layers.iter().filter(|l|l.id==r.layer || matches!((&selected.kind,&l.kind),(LayerKind::BlendWeight{group:a,..},LayerKind::BlendWeight{group:b,..})if a==b)).collect();
    members.sort_by(|a, b| a.id.cmp(&b.id));
    let points = crate::brush::local_path(scene, &r.object, &r.path_world_m)?;
    let sites = sites(t, r, &points, &members)?;
    let mut deltas = vec![];
    let mut writes = BTreeMap::new();
    for site in sites.values() {
        let owner = &t.tiles[site.owner];
        let before: BTreeMap<_, _> = members
            .iter()
            .map(|l| (l.id.clone(), value(owner, l, site.index)))
            .collect();
        let base = before[&r.layer];
        let target = match r.mode {
            TerrainPaintMode::Paint if site.weight == 1.0 => r.strength,
            TerrainPaintMode::Paint => {
                ((1.0 - f64::from(site.weight)) * f64::from(base)
                    + f64::from(site.weight) * f64::from(r.strength)) as f32
            }
            TerrainPaintMode::Erase => {
                (f64::from(base) * (1.0 - f64::from(r.strength) * f64::from(site.weight))) as f32
            }
        }
        .clamp(0.0, 1.0);
        let after = scene_data::layers::redistribute(&t.layers, &r.layer, &before, target)?;
        for (tile_index, index) in &site.copies {
            let tile = &t.tiles[*tile_index];
            for (layer, after) in &after {
                let definition = members.iter().find(|l| &l.id == layer).unwrap();
                let before = value(tile, definition, *index);
                if before.to_bits() == after.to_bits() {
                    continue;
                }
                if derived.contains(layer) {
                    return Err(Diagnostic::new("layer.derived_output").at(layer));
                }
                deltas.push(TerrainPaintTexel {
                    tile: tile.id.clone(),
                    layer: layer.clone(),
                    index: *index,
                    before,
                    after: *after,
                    weight: site.weight,
                });
                writes.insert((*tile_index, layer.clone(), *index), *after);
                if deltas.len() > MAX_PAINT_CHANGED_TEXELS {
                    return Err(Diagnostic::new("terrain.brush_limit").at(&r.object));
                }
            }
        }
    }
    deltas.sort_by(|a, b| (&a.tile, &a.layer, a.index).cmp(&(&b.tile, &b.layer, b.index)));
    let mut result = TerrainPaintResult {
        object: r.object.clone(),
        layer: r.layer.clone(),
        texel_deltas: deltas.into(),
        dirty_regions: Arc::from([]),
    };
    if result.texel_deltas.is_empty() {
        return Ok((result, None));
    }
    let missing: BTreeSet<_> = writes
        .keys()
        .filter(|(tile, layer, _)| !t.tiles[*tile].layers.iter().any(|p| &p.layer == layer))
        .map(|(tile, layer, _)| (*tile, layer.clone()))
        .collect();
    let added: u64 = missing
        .iter()
        .map(|(tile, _)| u64::from(side(t, &t.tiles[*tile], &r.layer, &members)).pow(2))
        .sum();
    let authored: u64 = t
        .tiles
        .iter()
        .flat_map(|tile| &tile.layers)
        .map(|p| p.weights.len() as u64)
        .sum();
    if authored + added > 8_000_000 {
        return Err(Diagnostic::new("layer.limit"));
    }
    let scene_authored: u64 = scene
        .objects
        .iter()
        .filter_map(|o| match &o.data {
            ObjectData::Terrain(t) => Some(t),
            _ => None,
        })
        .flat_map(|t| &t.tiles)
        .flat_map(|tile| &tile.layers)
        .map(|p| p.weights.len() as u64)
        .sum();
    if scene_authored + added > 8_000_000 {
        return Err(Diagnostic::new("scene.resource_limit"));
    }
    let mut next = scene.clone();
    let ObjectData::Terrain(next_t) = &mut next
        .objects
        .iter_mut()
        .find(|o| o.id == r.object)
        .unwrap()
        .data
    else {
        unreachable!()
    };
    for (tile, layer) in missing {
        let n = side(t, &t.tiles[tile], &r.layer, &members);
        let definition = t.layers.iter().find(|l| l.id == layer).unwrap();
        next_t.tiles[tile].layers.push(TileLayer {
            layer,
            side_samples: n,
            weights: vec![default_weight(definition); (n * n) as usize],
        });
        next_t.tiles[tile]
            .layers
            .sort_by(|a, b| a.layer.cmp(&b.layer));
    }
    for ((tile, layer, index), value) in writes {
        next_t.tiles[tile]
            .layers
            .iter_mut()
            .find(|p| p.layer == layer)
            .unwrap()
            .weights[index as usize] = value;
    }
    scene_data::validate(&next)?;
    let mut dirty = crate::session::dirty::terrain_dirty(scene, &next).to_vec();
    dirty.sort_by(|a, b| (&a.object, &a.tile).cmp(&(&b.object, &b.tile)));
    result.dirty_regions = dirty.into();
    Ok((result, Some(next)))
}
fn value(tile: &TerrainTile, l: &LayerDefinition, index: u32) -> f32 {
    tile.layers
        .iter()
        .find(|p| p.layer == l.id)
        .map_or_else(|| default_weight(l), |p| p.weights[index as usize])
}
fn side(
    t: &TerrainDocument,
    tile: &TerrainTile,
    selected: &LayerId,
    members: &[&LayerDefinition],
) -> u32 {
    tile.layers
        .iter()
        .find(|p| &p.layer == selected)
        .or_else(|| {
            tile.layers
                .iter()
                .find(|p| members.iter().any(|l| l.id == p.layer))
        })
        .map_or(t.cells_per_tile + 1, |p| p.side_samples)
}
fn sites(
    t: &TerrainDocument,
    r: &TerrainPaintRequest,
    points: &[[f32; 3]],
    members: &[&LayerDefinition],
) -> Result<BTreeMap<(Rational, Rational), Site>, Diagnostic> {
    let mut tiles: Vec<_> = t.tiles.iter().enumerate().collect();
    tiles.sort_by_key(|(_, tile)| tile.key);
    let mut sites = BTreeMap::new();
    let mut visits = 0u64;
    let segments: Vec<_> = if points.len() == 1 {
        vec![(points[0], points[0])]
    } else {
        points.windows(2).map(|p| (p[0], p[1])).collect()
    };
    for (tile_index, tile) in tiles {
        let n = side(t, tile, &r.layer, members) - 1;
        let physical = [
            f64::from(t.cells_per_tile) * f64::from(t.spacing_xz_m[0]),
            f64::from(t.cells_per_tile) * f64::from(t.spacing_xz_m[1]),
        ];
        let origin = [
            f64::from(tile.key.x) * physical[0],
            f64::from(tile.key.z) * physical[1],
        ];
        for (start, end) in &segments {
            let mut lo = [0; 2];
            let mut hi = [0; 2];
            for axis in 0..2 {
                let (a, b) = (f64::from(start[axis * 2]), f64::from(end[axis * 2]));
                let spacing = physical[axis] / f64::from(n);
                lo[axis] = (((a.min(b) - f64::from(r.radius_m) - origin[axis]) / spacing).floor()
                    as i64)
                    .saturating_sub(1)
                    .max(0);
                hi[axis] = (((a.max(b) + f64::from(r.radius_m) - origin[axis]) / spacing).ceil()
                    as i64)
                    .saturating_add(1)
                    .min(i64::from(n));
            }
            if lo[0] > hi[0] || lo[1] > hi[1] {
                continue;
            }
            visits = visits.saturating_add(((hi[0] - lo[0] + 1) * (hi[1] - lo[1] + 1)) as u64);
            if visits > MAX_RECT_VISITS {
                return Err(Diagnostic::new("terrain.brush_limit").at(&r.object));
            }
            for z in lo[1]..=hi[1] {
                for x in lo[0]..=hi[0] {
                    let key = (
                        Rational::new(tile.key.x, x as u32, n),
                        Rational::new(tile.key.z, z as u32, n),
                    );
                    let sample = [
                        key.0.meters(t.cells_per_tile, t.spacing_xz_m[0]),
                        key.1.meters(t.cells_per_tile, t.spacing_xz_m[1]),
                    ];
                    let weight = crate::brush::influence(
                        *start,
                        *end,
                        sample,
                        f64::from(r.radius_m),
                        r.falloff,
                        1.0,
                    );
                    if weight <= 0.0 {
                        continue;
                    }
                    let index = z as u32 * (n + 1) + x as u32;
                    let site = sites.entry(key).or_insert_with(|| Site {
                        owner: tile_index,
                        index,
                        weight,
                        copies: BTreeSet::new(),
                    });
                    site.weight = site.weight.max(weight);
                    site.copies.insert((tile_index, index));
                    if sites.len() > MAX_PAINT_CHANGED_TEXELS {
                        return Err(Diagnostic::new("terrain.brush_limit").at(&r.object));
                    }
                }
            }
        }
    }
    Ok(sites)
}
