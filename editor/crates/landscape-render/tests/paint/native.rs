use super::*;

#[test]
fn native_paint_changes_vertex_colors_reuses_entity_asset_and_preserves_collision_buffer() {
    use bevy::{
        ecs::world::CommandQueue,
        mesh::{Indices, VertexAttributeValues},
        prelude::*,
    };
    use landscape_render::{GroundRenderer, GroundTile};
    let mut session = EditorSession::new(scene()).unwrap();
    let mut cache = GeometryCache::default();
    let original = parity(&mut cache, session.snapshot().document.clone());
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
            &original.simulation.ground,
            Some(&original.visual),
            &[],
        )
        .unwrap();
    queue.apply(&mut world);
    let mut query = world.query::<(Entity, &GroundTile, &Mesh3d)>();
    let identities: std::collections::BTreeMap<_, _> = query
        .iter(&world)
        .map(|(e, t, m)| (t.tile.clone(), (e, m.id())))
        .collect();
    let CommandOutcome::TransactionStarted(txn) = send(
        &mut session,
        1,
        editor_core::Command::BeginViewportEdit {
            selection_revision: 0,
        },
    ) else {
        panic!()
    };
    let result = preview(&mut session, 2, txn, 201, 0.75);
    let painted = parity(
        &mut cache,
        session
            .snapshot()
            .preview
            .as_ref()
            .unwrap()
            .document
            .clone(),
    );
    let facts = renderer
        .sync_with_visual(
            &mut Commands::new(&mut queue, &world),
            &mut meshes,
            &mut materials,
            &painted.simulation.ground,
            Some(&painted.visual),
            &result.dirty_regions,
        )
        .unwrap();
    queue.apply(&mut world);
    assert_eq!(facts.tiles_rebuilt, 2);
    for (entity, tile, handle) in query.iter(&world) {
        assert_eq!(identities[&tile.tile], (entity, handle.id()));
        let mesh = meshes.get(handle.id()).unwrap();
        let g = painted
            .simulation
            .ground
            .iter()
            .find(|g| g.tile == tile.tile)
            .unwrap();
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!()
        };
        assert_eq!(colors, &vec![[0.25, 0.0, 0.75, 1.0]; 9]);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!()
        };
        assert_eq!(positions, &g.positions_m);
        let Some(Indices::U32(indices)) = mesh.indices() else {
            panic!()
        };
        assert_eq!(
            indices,
            &g.triangles.iter().flatten().copied().collect::<Vec<_>>()
        );
    }
    assert_eq!(meshes.len(), 2);
    assert_eq!(
        renderer
            .sync_with_visual(
                &mut Commands::new(&mut queue, &world),
                &mut meshes,
                &mut materials,
                &painted.simulation.ground,
                Some(&painted.visual),
                &[]
            )
            .unwrap()
            .tiles_rebuilt,
        0
    );
    send(
        &mut session,
        3,
        editor_core::Command::CancelViewportEdit { transaction: txn },
    );
    let restored = parity(&mut cache, session.snapshot().document.clone());
    renderer
        .sync_with_visual(
            &mut Commands::new(&mut queue, &world),
            &mut meshes,
            &mut materials,
            &restored.simulation.ground,
            Some(&restored.visual),
            &[],
        )
        .unwrap();
    queue.apply(&mut world);
    for (entity, tile, handle) in query.iter(&world) {
        assert_eq!(identities[&tile.tile], (entity, handle.id()));
        let mesh = meshes.get(handle.id()).unwrap();
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!()
        };
        assert_eq!(colors, &vec![[1.0, 0.0, 0.0, 1.0]; 9]);
    }
}
