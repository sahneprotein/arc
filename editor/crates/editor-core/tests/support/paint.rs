use editor_core::*;
use scene_data::{terrain::*, *};

pub fn source(cells: u32) -> SceneDocument {
    let mut t = TerrainDocument::flat(
        cells,
        [1.0, 1.0],
        (-1..=0)
            .flat_map(|z| {
                (-1..=0).map(move |x| {
                    (
                        StableId::from_u128(100 + ((z + 1) * 2 + x + 1) as u128),
                        TileKey { x, z },
                    )
                })
            })
            .collect(),
    )
    .unwrap();
    let group = StableId::from_u128(200);
    let layers = (0..3)
        .map(|i| LayerDefinition {
            id: StableId::from_u128(201 + i),
            name: format!("layer{i}"),
            kind: if i == 2 {
                LayerKind::Mask
            } else {
                LayerKind::BlendWeight {
                    group: group.clone(),
                    default_weight: if i == 0 { 1.0 } else { 0.0 },
                }
            },
            color_linear_rgba: if i == 2 {
                None
            } else {
                Some(if i == 0 {
                    [1.0, 0.0, 0.0, 1.0]
                } else {
                    [0.0, 0.0, 1.0, 1.0]
                })
            },
        })
        .collect();
    scene_data::layers::set_registry(&mut t, layers, Some(group), cells + 1).unwrap();
    SceneDocument {
        schema: SCHEMA_VERSION,
        id: StableId::from_u128(1),
        objects: vec![ObjectRecord {
            id: StableId::from_u128(49),
            parent: None,
            sibling_order: 0,
            name: "terrain".into(),
            local: Default::default(),
            data: ObjectData::Terrain(t),
        }],
    }
}
pub fn t(s: &mut SceneDocument) -> &mut TerrainDocument {
    let ObjectData::Terrain(t) = &mut s.objects[0].data else {
        panic!()
    };
    t
}
pub fn request(layer: u128) -> TerrainPaintRequest {
    TerrainPaintRequest {
        object: StableId::from_u128(49),
        layer: StableId::from_u128(layer),
        path_world_m: vec![[0.0, 0.0, 0.0]],
        radius_m: 1.5,
        strength: 0.75,
        falloff: 1.0,
        mode: TerrainPaintMode::Paint,
    }
}
pub fn send(
    s: &mut EditorSession,
    id: u64,
    command: Command,
) -> Result<CommandOutcome, Diagnostic> {
    let v = s.snapshot();
    s.submit(CommandRequest {
        request: id,
        session: v.session,
        expected_revision: v.document_revision,
        expected_inspector_revision: None,
        command,
    })
    .outcome
}
pub fn begin(s: &mut EditorSession, id: u64) -> u64 {
    let CommandOutcome::TransactionStarted(t) = send(
        s,
        id,
        Command::BeginViewportEdit {
            selection_revision: 0,
        },
    )
    .unwrap() else {
        panic!()
    };
    t
}
pub fn preview(
    s: &mut EditorSession,
    id: u64,
    txn: u64,
    seq: u64,
    r: TerrainPaintRequest,
) -> TerrainPaintResult {
    let CommandOutcome::TerrainPainted { sequence, result } = send(
        s,
        id,
        Command::PreviewTerrainPaint {
            transaction: txn,
            sequence: seq,
            request: r,
        },
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(sequence, seq);
    result
}
