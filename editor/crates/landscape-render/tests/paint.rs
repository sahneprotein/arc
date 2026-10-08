use editor_cook::GeometryCache;
use editor_core::*;
use landscape_render::GroundCache;
use scene_data::{terrain::*, *};
use std::sync::Arc;
fn id(n: u128) -> StableId {
    StableId::from_u128(n)
}
fn scene() -> SceneDocument {
    let mut t = TerrainDocument::flat(
        2,
        [1.0, 1.0],
        vec![
            (id(100), TileKey { x: -1, z: 0 }),
            (id(101), TileKey { x: 0, z: 0 }),
        ],
    )
    .unwrap();
    let defs = (0..6)
        .map(|i| LayerDefinition {
            id: id(200 + i),
            name: format!("layer{i}"),
            kind: match i {
                2 | 3 => LayerKind::Mask,
                _ => LayerKind::BlendWeight {
                    group: id(if i < 2 { 300 } else { 301 }),
                    default_weight: if i == 0 || i == 4 { 1.0 } else { 0.0 },
                },
            },
            color_linear_rgba: match i {
                0 => Some([1.0, 0.0, 0.0, 1.0]),
                1 => Some([0.0, 0.0, 1.0, 1.0]),
                4 | 5 => Some([0.0, 1.0, 0.0, 1.0]),
                _ => None,
            },
        })
        .collect();
    scene_data::layers::set_registry(&mut t, defs, Some(id(300)), 3).unwrap();
    t.rules = vec![
        RuleDefinition {
            id: id(400),
            algorithm: scene_data::rules::AUTO_LAYER_SLOPE_HEIGHT.into(),
            version: 1,
            seed: 0,
            input_layers: vec![id(202)],
            input_rules: vec![],
            output_layer: Some(id(203)),
            density_layer: None,
            exclusion_layers: vec![],
            parameters: [("slope_falloff".into(), 0.0)].into(),
        },
        RuleDefinition {
            id: id(401),
            algorithm: scene_data::rules::GRASS_SCATTER.into(),
            version: 1,
            seed: 9,
            input_layers: vec![id(203)],
            input_rules: vec![id(400)],
            output_layer: None,
            density_layer: Some(id(203)),
            exclusion_layers: vec![],
            parameters: [("density".into(), 1.0), ("candidates_per_cell".into(), 1.0)].into(),
        },
    ];
    SceneDocument {
        schema: SCHEMA_VERSION,
        id: id(1),
        objects: vec![ObjectRecord {
            id: id(2),
            parent: None,
            sibling_order: 0,
            name: "Ground".into(),
            local: Default::default(),
            data: ObjectData::Terrain(t),
        }],
    }
}
fn send(s: &mut EditorSession, n: u64, command: Command) -> CommandOutcome {
    let v = s.snapshot();
    s.submit(CommandRequest {
        request: n,
        session: v.session,
        expected_revision: v.document_revision,
        expected_inspector_revision: None,
        command,
    })
    .outcome
    .unwrap()
}
fn preview(
    s: &mut EditorSession,
    n: u64,
    txn: u64,
    layer: u128,
    strength: f32,
) -> TerrainPaintResult {
    let CommandOutcome::TerrainPainted { result, .. } = send(
        s,
        n,
        Command::PreviewTerrainPaint {
            transaction: txn,
            sequence: n,
            request: TerrainPaintRequest {
                object: id(2),
                layer: id(layer),
                path_world_m: vec![[0.0, 0.0, 1.0]],
                radius_m: 10.0,
                strength,
                falloff: 0.0,
                mode: TerrainPaintMode::Paint,
            },
        },
    ) else {
        panic!()
    };
    result
}
fn parity(cache: &mut GeometryCache, document: Arc<SceneDocument>) -> editor_cook::GeometryPackage {
    let full = editor_cook::cook(&document).unwrap();
    let package = cache.cook(document, &|| Ok(())).unwrap();
    assert_eq!(
        serde_json::to_vec(package.simulation.as_ref()).unwrap(),
        full.simulation_bytes
    );
    assert_eq!(
        serde_json::to_vec(package.visual.as_ref()).unwrap(),
        full.visual_bytes
    );
    package
}
#[test]
fn paint_preview_commit_cancel_rule_updates_and_material_edits_match_full_cook_with_identical_collision(
) {
    let source = scene();
    let base = editor_cook::cook(&source).unwrap();
    let mut session = EditorSession::new(source.clone()).unwrap();
    let mut cook = GeometryCache::default();
    let original = parity(&mut cook, Arc::new(source));
    let mut renderer = GroundCache::default();
    renderer
        .update_with_visual(&original.simulation.ground, Some(&original.visual), &[])
        .unwrap();
    let original_meshes = renderer.tiles().clone();
    let CommandOutcome::TransactionStarted(txn) = send(
        &mut session,
        1,
        Command::BeginViewportEdit {
            selection_revision: 0,
        },
    ) else {
        panic!()
    };
    let result = preview(&mut session, 2, txn, 201, 0.75);
    assert_eq!(result.dirty_regions.len(), 2);
    let document = session
        .snapshot()
        .preview
        .as_ref()
        .unwrap()
        .document
        .clone();
    let painted = parity(&mut cook, document.clone());
    assert_eq!(painted.work.rebuilt_tiles, 0);
    assert_eq!(painted.work.reused_tiles, 2);
    assert_eq!(painted.simulation, original.simulation);
    let full = editor_cook::cook(&document).unwrap();
    assert_eq!(full.simulation_hash, base.simulation_hash);
    assert_ne!(full.visual_hash, base.visual_hash);
    assert_ne!(full.source_hash, base.source_hash);
    let facts = renderer
        .update_with_visual(
            &painted.simulation.ground,
            Some(&painted.visual),
            &result.dirty_regions,
        )
        .unwrap();
    assert_eq!(facts.tiles_rebuilt, 2);
    for (key, mesh) in renderer.tiles() {
        assert_eq!(mesh.colors, vec![[0.25, 0.0, 0.75, 1.0]; 9]);
        assert_eq!(mesh.positions, original_meshes[key].positions);
        assert_eq!(mesh.indices, original_meshes[key].indices);
    }
    send(
        &mut session,
        3,
        Command::CancelViewportEdit { transaction: txn },
    );
    let restored = parity(&mut cook, session.snapshot().document.clone());
    assert_eq!(restored.work.rebuilt_tiles, 0);
    renderer
        .update_with_visual(&restored.simulation.ground, Some(&restored.visual), &[])
        .unwrap();
    assert_eq!(renderer.tiles(), &original_meshes);
    let CommandOutcome::TransactionStarted(txn) = send(
        &mut session,
        4,
        Command::BeginViewportEdit {
            selection_revision: 0,
        },
    ) else {
        panic!()
    };
    preview(&mut session, 5, txn, 202, 1.0);
    let masked = parity(
        &mut cook,
        session
            .snapshot()
            .preview
            .as_ref()
            .unwrap()
            .document
            .clone(),
    );
    assert!(original.visual.terrain[0].decoration.is_empty());
    assert!(!masked.visual.terrain[0].decoration.is_empty());
    assert!(masked.visual.terrain[0]
        .derived_layers
        .iter()
        .all(|d| d.weights == vec![1.0; 9]));
    assert_eq!(masked.simulation, original.simulation);
    assert_eq!(masked.work.rebuilt_tiles, 0);
    assert_eq!(
        renderer
            .update_with_visual(&masked.simulation.ground, Some(&masked.visual), &[])
            .unwrap()
            .tiles_rebuilt,
        0
    );
    send(
        &mut session,
        6,
        Command::CommitViewportEdit {
            transaction: txn,
            label_key: "paint".into(),
        },
    );
    let committed = parity(&mut cook, session.snapshot().document.clone());
    assert_eq!(committed.visual, masked.visual);
    let CommandOutcome::TransactionStarted(txn) = send(
        &mut session,
        7,
        Command::BeginViewportEdit {
            selection_revision: 0,
        },
    ) else {
        panic!()
    };
    preview(&mut session, 8, txn, 205, 0.8);
    let other = parity(
        &mut cook,
        session
            .snapshot()
            .preview
            .as_ref()
            .unwrap()
            .document
            .clone(),
    );
    assert_eq!(other.work.rebuilt_tiles, 0);
    assert_eq!(
        renderer
            .update_with_visual(&other.simulation.ground, Some(&other.visual), &[])
            .unwrap()
            .tiles_rebuilt,
        0
    );
    send(
        &mut session,
        9,
        Command::CancelViewportEdit { transaction: txn },
    );
    send(&mut session, 10, Command::Undo);
    let undone = parity(&mut cook, session.snapshot().document.clone());
    assert_eq!(undone.visual, original.visual);
    send(&mut session, 11, Command::Redo);
    assert_eq!(
        parity(&mut cook, session.snapshot().document.clone()).visual,
        masked.visual
    );
    let snapshot = session.snapshot();
    let ObjectData::Terrain(t) = &snapshot.document.objects[0].data else {
        panic!()
    };
    let mut defs = t.layers.clone();
    defs[0].color_linear_rgba = Some([0.0, 1.0, 0.0, 1.0]);
    send(
        &mut session,
        12,
        Command::Edit {
            label_key: "color".into(),
            mutations: vec![Mutation::TerrainRegistry {
                object: id(2),
                layers: defs,
                surface_group: Some(id(300)),
                side_samples: 7,
            }],
        },
    );
    let recolored = parity(&mut cook, session.snapshot().document.clone());
    assert_eq!(recolored.work.rebuilt_tiles, 0);
    assert_eq!(
        renderer
            .update_with_visual(&recolored.simulation.ground, Some(&recolored.visual), &[])
            .unwrap()
            .tiles_rebuilt,
        2
    );
    assert!(renderer
        .tiles()
        .values()
        .all(|m| m.colors == vec![[0.0, 1.0, 0.0, 1.0]; 9]));
}

#[cfg(feature = "bevy-render")]
#[path = "paint/native.rs"]
mod native;
