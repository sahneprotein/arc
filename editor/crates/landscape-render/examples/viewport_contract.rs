//! Compiling UI/core seam example; the application supplies its own cameras and input.
//! This headless check prepares one real terrain tile without opening a window.
use bevy::prelude::*;
use editor_services::{EditorHandle, JobOperation, JobOutput, ServiceEvent};
use landscape_render::{GroundRenderer, GroundTile, UpdateFacts};
use scene_data::{terrain::*, *};
use std::time::{Duration, Instant};

#[derive(Resource, Default)]
struct PendingGeometry(Option<JobOutput>);
#[derive(Resource, Default)]
struct Applied(Option<UpdateFacts>);

fn apply_geometry(
    mut editor: NonSendMut<EditorHandle>,
    mut pending: ResMut<PendingGeometry>,
    mut renderer: ResMut<GroundRenderer>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut applied: ResMut<Applied>,
) {
    // Bounded drain first, then take the latest cached snapshot. A later Snapshot
    // in the same batch may invalidate an earlier JobFinished geometry result.
    for event in editor.poll(64) {
        if let ServiceEvent::JobFinished { result, .. } = event
            && let Ok(output @ JobOutput::Geometry { .. }) = *result
        {
            pending.0 = Some(output);
        }
    }
    let view = editor.snapshot();
    if let Some(JobOutput::Geometry {
        binding,
        simulation,
        visual,
        ..
    }) = pending.0.take()
    {
        if !binding.matches(&view) {
            return; // Submit a fresh Geometry request for the newest binding.
        }
        let dirty = view
            .preview
            .as_ref()
            .map(|p| p.dirty.as_ref())
            .unwrap_or(&[]);
        // Positions/normals/indices are already world-space and shared with collision.
        // GroundRenderer never creates a camera or modifies UI overlay entities.
        applied.0 = Some(
            renderer
                .sync_with_visual(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    &simulation.ground,
                    Some(&visual),
                    dirty,
                )
                .expect("validated geometry"),
        );
    }
}

fn main() {
    let mut document = current_map();
    document.objects.push(ObjectRecord {
        id: StableId::from_u128(1000),
        parent: None,
        sibling_order: 1000,
        name: "Contract terrain".into(),
        local: RigidTransform::default(),
        data: ObjectData::Terrain(
            TerrainDocument::flat(
                2,
                [1.0, 1.0],
                vec![(StableId::from_u128(1001), TileKey { x: -1, z: 0 })],
            )
            .unwrap(),
        ),
    });
    let mut editor = EditorHandle::start(document).unwrap();
    let _ticket = editor.job(JobOperation::Geometry).unwrap();
    let mut app = App::new();
    app.insert_non_send(editor)
        .init_resource::<PendingGeometry>()
        .init_resource::<Applied>()
        .init_resource::<GroundRenderer>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<StandardMaterial>>()
        .add_systems(Update, apply_geometry);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.world().resource::<Applied>().0.is_none() && Instant::now() < deadline {
        app.update();
        std::thread::sleep(Duration::from_millis(1));
    }
    let facts = app.world().resource::<Applied>().0.expect("geometry ready");
    assert_eq!(facts.tiles_rebuilt, 1);
    let mut tiles = app.world_mut().query::<(&GroundTile, &Mesh3d)>();
    let (_, mesh) = tiles.single(app.world()).unwrap();
    assert_eq!(
        app.world()
            .resource::<Assets<Mesh>>()
            .get(mesh.id())
            .unwrap()
            .count_vertices(),
        9
    );
    println!(
        "matched geometry; {} tile, {} CPU bytes prepared",
        facts.tiles_rebuilt, facts.prepared_bytes
    );
}
