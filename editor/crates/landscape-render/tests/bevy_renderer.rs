#![cfg(feature = "bevy-render")]
use bevy::{
    ecs::world::CommandQueue,
    mesh::{Indices, VertexAttributeValues},
    prelude::*,
};
use landscape_render::*;
use scene_data::{terrain::*, *};
fn package() -> editor_cook::CookedPackage {
    let mut terrain = TerrainDocument::flat(
        1,
        [1.0, 1.0],
        vec![(StableId::from_u128(100), TileKey { x: -1, z: 0 })],
    )
    .unwrap();
    let group = StableId::from_u128(300);
    let blue = StableId::from_u128(302);
    terrain.surface_group = Some(group.clone());
    terrain.layers = vec![
        LayerDefinition {
            id: StableId::from_u128(301),
            name: "Red".into(),
            kind: LayerKind::BlendWeight {
                group: group.clone(),
                default_weight: 1.0,
            },
            color_linear_rgba: Some([1.0, 0.0, 0.0, 1.0]),
        },
        LayerDefinition {
            id: blue.clone(),
            name: "Blue".into(),
            kind: LayerKind::BlendWeight {
                group,
                default_weight: 0.0,
            },
            color_linear_rgba: Some([0.0, 0.0, 1.0, 1.0]),
        },
    ];
    terrain.rules.push(RuleDefinition {
        id: StableId::from_u128(201),
        algorithm: scene_data::rules::AUTO_LAYER_SLOPE_HEIGHT.into(),
        version: scene_data::rules::AUTO_LAYER_VERSION,
        seed: 0,
        input_layers: vec![],
        input_rules: vec![],
        output_layer: Some(blue),
        density_layer: None,
        exclusion_layers: vec![],
        parameters: [("slope_falloff".into(), 0.0)].into_iter().collect(),
    });
    terrain.rules.push(RuleDefinition {
        id: StableId::from_u128(200),
        algorithm: scene_data::rules::GRASS_SCATTER.into(),
        version: 1,
        seed: 17,
        input_layers: vec![],
        input_rules: vec![],
        output_layer: None,
        density_layer: None,
        exclusion_layers: vec![],
        parameters: Default::default(),
    });
    editor_cook::cook(&SceneDocument {
        schema: SCHEMA_VERSION,
        id: StableId::from_u128(1),
        objects: vec![ObjectRecord {
            id: StableId::from_u128(2),
            parent: None,
            sibling_order: 0,
            name: "Ground".into(),
            local: RigidTransform::default(),
            data: ObjectData::Terrain(terrain),
        }],
    })
    .unwrap()
}
/// Draws the package's ground with this renderer into this world.
fn draw(
    renderer: &mut GroundRenderer,
    world: &mut World,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    package: &editor_cook::CookedPackage,
) {
    let mut queue = CommandQueue::default();
    renderer
        .sync_with_visual(
            &mut Commands::new(&mut queue, world),
            meshes,
            materials,
            &package.simulation.ground,
            Some(&package.visual),
            &[],
        )
        .unwrap();
    queue.apply(world);
}
#[test]
fn the_grounds_material_takes_a_views_look_where_it_is_and_the_defaults_back() {
    let package = package();
    let mut world = World::new();
    let mut meshes = Assets::<Mesh>::default();
    let mut materials = Assets::<StandardMaterial>::default();
    let mut renderer = GroundRenderer::default();
    draw(
        &mut renderer,
        &mut world,
        &mut meshes,
        &mut materials,
        &package,
    );
    let mut query = world.query::<(&MeshMaterial3d<StandardMaterial>, &Mesh3d)>();
    let (material, mesh) = query.single(&world).unwrap();
    let (material, mesh) = (material.0.clone(), mesh.0.clone());
    let defaults = ground_material();
    let now = |materials: &Assets<StandardMaterial>| {
        let material = materials.get(material.id()).unwrap();
        (material.base_color, material.perceptual_roughness)
    };
    assert_eq!(renderer.look(), None);
    assert_eq!(
        now(&materials),
        (defaults.base_color, defaults.perceptual_roughness)
    );

    // A view's look changes the one material where it is: the tile keeps its entity, its
    // material and its mesh, vertex colours and all.
    let look = GroundLook {
        base_color_linear_rgba: [0.5, 0.25, 0.125, 1.0],
        roughness: 0.4,
    };
    renderer.set_look(Some(look), &mut materials);
    assert_eq!(renderer.look(), Some(look));
    assert_eq!(
        now(&materials),
        (Color::linear_rgba(0.5, 0.25, 0.125, 1.0), 0.4)
    );
    draw(
        &mut renderer,
        &mut world,
        &mut meshes,
        &mut materials,
        &package,
    );
    assert_eq!(materials.len(), 1);
    let (after_material, after_mesh) = query.single(&world).unwrap();
    assert_eq!((&after_material.0, &after_mesh.0), (&material, &mesh));
    let Some(VertexAttributeValues::Float32x4(colors)) = meshes
        .get(mesh.id())
        .unwrap()
        .attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!()
    };
    assert_eq!(colors, &vec![[0.5, 0.0, 0.5, 1.0]; 4]);

    // Without a look the defaults are back, exactly.
    renderer.set_look(None, &mut materials);
    assert_eq!(
        now(&materials),
        (defaults.base_color, defaults.perceptual_roughness)
    );

    // A look that is set before anything is drawn is what the material is made with.
    let mut early = GroundRenderer::default();
    let mut early_world = World::new();
    let mut early_materials = Assets::<StandardMaterial>::default();
    early.set_look(Some(look), &mut early_materials);
    draw(
        &mut early,
        &mut early_world,
        &mut meshes,
        &mut early_materials,
        &package,
    );
    let (_, made) = early_materials.iter().next().unwrap();
    assert_eq!(made.perceptual_roughness, 0.4);
    assert_eq!(made.base_color, Color::linear_rgba(0.5, 0.25, 0.125, 1.0));
}
#[test]
fn bevy_ground_mesh_is_exact_collision_geometry_and_reuses_entity_and_asset() {
    let package = package();
    let mut world = World::new();
    let mut meshes = Assets::<Mesh>::default();
    let mut materials = Assets::<StandardMaterial>::default();
    let mut renderer = GroundRenderer::default();
    let mut queue = CommandQueue::default();
    renderer
        .sync_with_visual(
            &mut Commands::new(&mut queue, &world),
            &mut meshes,
            &mut materials,
            &package.simulation.ground,
            Some(&package.visual),
            &[],
        )
        .unwrap();
    queue.apply(&mut world);
    let mut query = world.query::<(Entity, &GroundTile, &Mesh3d, &Transform)>();
    let (entity, tile, handle, transform) = query.single(&world).unwrap();
    let mesh_handle = handle.clone();
    assert_eq!(tile.tile, package.simulation.ground[0].tile);
    assert_eq!(*transform, Transform::IDENTITY);
    let mesh = meshes.get(handle.id()).unwrap();
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!()
    };
    assert_eq!(*positions, package.simulation.ground[0].positions_m);
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!()
    };
    assert_eq!(colors, &vec![[0.5, 0.0, 0.5, 1.0]; 4]);
    let Some(Indices::U32(indices)) = mesh.indices() else {
        panic!()
    };
    assert_eq!(
        *indices,
        package.simulation.ground[0]
            .triangles
            .iter()
            .flatten()
            .copied()
            .collect::<Vec<_>>()
    );
    let facts = renderer
        .sync_with_visual(
            &mut Commands::new(&mut queue, &world),
            &mut meshes,
            &mut materials,
            &package.simulation.ground,
            Some(&package.visual),
            &[],
        )
        .unwrap();
    queue.apply(&mut world);
    assert_eq!(facts.tiles_rebuilt, 0);
    let (reused, _, handle, _) = query.single(&world).unwrap();
    assert_eq!(reused, entity);
    assert_eq!(handle.id(), mesh_handle.id());
    assert_eq!(meshes.len(), 1);
}
#[test]
fn grass_entities_toggle_per_view_without_source_changes_and_remove_mesh_assets() {
    let mut package = package();
    let simulation_before = package.simulation.clone();
    let visual_before = package.visual.clone();
    let mut world = World::new();
    let mut meshes = Assets::<Mesh>::default();
    let mut materials = Assets::<StandardMaterial>::default();
    let mut renderer = GrassRenderer::default();
    let mut queue = CommandQueue::default();
    let facts = renderer
        .sync(
            &mut Commands::new(&mut queue, &world),
            &mut meshes,
            &mut materials,
            &package.visual,
            &package.simulation.ground,
        )
        .unwrap();
    queue.apply(&mut world);
    assert_eq!(facts.scene_instances, 4);
    assert_eq!(facts.update.cpu_allocated_instances, 4);
    assert!(facts.gpu_submitted_instances.is_none());
    assert!(facts.gpu_visible_instances.is_none());
    let mut query = world.query::<(Entity, &GrassInstance, &Mesh3d, &Visibility)>();
    let (entity, instance, handle, visibility) = query.iter(&world).next().unwrap();
    let mesh_id = handle.id();
    assert_eq!(query.iter(&world).count(), 4);
    assert!(package.visual.terrain[0]
        .decoration
        .iter()
        .any(|d| d.id == instance.instance));
    assert!(query.iter(&world).all(|(_, _, h, _)| h.id() == mesh_id));
    assert_eq!(*visibility, Visibility::Inherited);
    assert_eq!(meshes.get(mesh_id).unwrap().count_vertices(), 8);
    renderer.set_enabled(false, &mut Commands::new(&mut queue, &world));
    queue.apply(&mut world);
    assert!(query
        .iter(&world)
        .all(|(_, _, _, v)| *v == Visibility::Hidden));
    assert_eq!(renderer.facts().scene_instances, 0);
    assert_eq!(renderer.facts().update.cpu_allocated_instances, 4);
    assert_eq!(meshes.len(), 1);
    renderer.set_enabled(true, &mut Commands::new(&mut queue, &world));
    queue.apply(&mut world);
    assert!(query.iter(&world).any(|(e, _, _, _)| e == entity));
    assert!(query.iter(&world).all(|(_, _, h, _)| h.id() == mesh_id));
    assert_eq!(renderer.facts().scene_instances, 4);
    assert_eq!(package.visual, visual_before);
    assert_eq!(package.simulation, simulation_before);
    let repeated = renderer
        .sync(
            &mut Commands::new(&mut queue, &world),
            &mut meshes,
            &mut materials,
            &package.visual,
            &package.simulation.ground,
        )
        .unwrap();
    queue.apply(&mut world);
    assert_eq!(repeated.update.instances_spawned, 0);
    assert_eq!(repeated.update.instances_updated, 0);
    assert_eq!(repeated.update.prepared_transform_bytes, 0);
    assert_eq!(meshes.len(), 1);
    package.visual.terrain[0].decoration[0].height_m += 0.1;
    let changed = renderer
        .sync(
            &mut Commands::new(&mut queue, &world),
            &mut meshes,
            &mut materials,
            &package.visual,
            &package.simulation.ground,
        )
        .unwrap();
    queue.apply(&mut world);
    assert_eq!(changed.update.instances_updated, 1);
    assert_eq!(meshes.len(), 1);
    // The render transforms agree with the exact cooker world placement and size.
    let mut transforms = world.query::<(&GrassInstance, &Transform)>();
    for (identity, transform) in transforms.iter(&world) {
        let source = package.visual.terrain[0]
            .decoration
            .iter()
            .find(|d| d.id == identity.instance)
            .unwrap();
        assert_eq!(transform.translation.to_array(), source.position_m);
        assert_eq!(
            transform.scale.to_array(),
            [source.width_m, source.height_m, source.width_m]
        );
    }
    package.visual.terrain[0].decoration.clear();
    let facts = renderer
        .sync(
            &mut Commands::new(&mut queue, &world),
            &mut meshes,
            &mut materials,
            &package.visual,
            &package.simulation.ground,
        )
        .unwrap();
    queue.apply(&mut world);
    assert_eq!(facts.update.instances_removed, 4);
    assert_eq!(facts.scene_instances, 0);
    assert!(query.iter(&world).next().is_none());
    assert!(meshes.is_empty());
}
