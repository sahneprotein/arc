use crate::*;
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};

#[derive(Component, Clone, Debug)]
pub struct GroundTile {
    pub object: ObjectId,
    pub tile: TileId,
    pub key: TileKey,
    pub cells_per_tile: u32,
}
pub fn ground_mesh(data: &GroundMesh) -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, data.positions.clone())
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, data.normals.clone())
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, data.colors.clone())
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, data.uv.clone())
    .with_inserted_indices(Indices::U32(data.indices.clone()))
}
pub fn ground_material() -> StandardMaterial {
    StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.9,
        ..Default::default()
    }
}
/// What a view asks the ground's one material to be made with in place of the defaults of
/// [`ground_material`]: its colour under the vertices' paint, as linear RGBA, and its roughness.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundLook {
    pub base_color_linear_rgba: [f32; 4],
    pub roughness: f32,
}
type GroundEntry = (Entity, Handle<Mesh>, Arc<GroundMesh>);
/// Each viewport/game world owns one instance. Both use the same factory and defaults.
#[derive(Resource, Default)]
pub struct GroundRenderer {
    cache: GroundCache,
    tiles: BTreeMap<(ObjectId, TileId), GroundEntry>,
    material: Option<Handle<StandardMaterial>>,
    look: Option<GroundLook>,
}
impl GroundRenderer {
    /// Makes the ground's material with `look`, or with the defaults again for `None`. The
    /// one material every tile shares is changed where it is, so tiles that are drawn take it
    /// without being made anew; a material that is not made yet is made with it. Meshes and
    /// their vertex colours are not touched.
    pub fn set_look(&mut self, look: Option<GroundLook>, materials: &mut Assets<StandardMaterial>) {
        self.look = look;
        if let Some(handle) = &self.material
            && let Some(mut material) = materials.get_mut(handle.id())
        {
            let wanted = self.material_now();
            material.base_color = wanted.base_color;
            material.perceptual_roughness = wanted.perceptual_roughness;
        }
    }
    /// What the material is made with in place of the defaults, if anything.
    pub fn look(&self) -> Option<GroundLook> {
        self.look
    }
    fn material_now(&self) -> StandardMaterial {
        match self.look {
            Some(look) => {
                let [red, green, blue, alpha] = look.base_color_linear_rgba;
                StandardMaterial {
                    base_color: Color::linear_rgba(red, green, blue, alpha),
                    perceptual_roughness: look.roughness,
                    ..ground_material()
                }
            }
            None => ground_material(),
        }
    }
    pub fn set_full_detail(&mut self, value: bool) {
        self.cache.set_full_detail(value);
    }
    pub fn sync(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        ground: &[CookedGround],
        dirty: &[DirtyRegion],
    ) -> Result<UpdateFacts, Diagnostic> {
        self.sync_with_visual(commands, meshes, materials, ground, None, dirty)
    }
    pub fn sync_with_visual(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<StandardMaterial>,
        ground: &[CookedGround],
        visual: Option<&VisualPayload>,
        dirty: &[DirtyRegion],
    ) -> Result<UpdateFacts, Diagnostic> {
        let facts = self.cache.update_with_visual(ground, visual, dirty)?;

        let made_with = self.material_now();
        let material = self
            .material
            .get_or_insert_with(|| materials.add(made_with))
            .clone();
        self.tiles.retain(|key, (entity, _, _)| {
            if self.cache.tiles.contains_key(key) {
                true
            } else {
                commands.entity(*entity).despawn();
                false
            }
        });
        for (key, data) in &self.cache.tiles {
            if let Some((entity, handle, previous)) = self.tiles.get_mut(key) {
                if Arc::ptr_eq(previous, data) {
                    continue;
                }
                if let Some(mut mesh) = meshes.get_mut(handle.id()) {
                    *mesh = ground_mesh(data);
                } else {
                    *handle = meshes.add(ground_mesh(data));
                    commands.entity(*entity).insert(Mesh3d(handle.clone()));
                }
                *previous = data.clone();
            } else {
                let mesh = meshes.add(ground_mesh(data));
                let entity = commands
                    .spawn((
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::default(),
                        GroundTile {
                            object: data.object.clone(),
                            tile: data.tile.clone(),
                            key: data.key,
                            cells_per_tile: data.cells_per_tile,
                        },
                    ))
                    .id();
                self.tiles.insert(key.clone(), (entity, mesh, data.clone()));
            }
        }
        Ok(facts)
    }
}
