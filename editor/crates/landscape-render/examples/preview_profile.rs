//! Controlled functional profiling: actual command/Geometry worker path and CPU renderer cache.
use editor_core::*;
use editor_services::*;
use landscape_render::GroundCache;
use scene_data::{terrain::*, *};
use std::{
    fs::File,
    io::{BufWriter, Write},
    time::{Duration, Instant},
};

fn scene(side: u32, rules: bool) -> SceneDocument {
    let mut source = current_map();
    if side == 0 {
        return source;
    }
    let tiles = (0..side)
        .flat_map(|z| {
            (0..side).map(move |x| {
                (
                    StableId::from_u128(10_000 + u128::from(z * side + x)),
                    TileKey {
                        x: x as i32,
                        z: z as i32,
                    },
                )
            })
        })
        .collect();
    let mut terrain = TerrainDocument::flat(32, [1.0, 1.0], tiles).unwrap();
    if rules {
        let layer = StableId::from_u128(20_000);
        let producer = StableId::from_u128(20_001);
        terrain.layers.push(LayerDefinition {
            id: layer.clone(),
            name: "Derived".into(),
            kind: LayerKind::Mask,
            color_linear_rgba: None,
        });
        terrain.rules.push(RuleDefinition {
            id: producer.clone(),
            algorithm: scene_data::rules::AUTO_LAYER_SLOPE_HEIGHT.into(),
            version: 1,
            seed: 0,
            input_layers: vec![],
            input_rules: vec![],
            output_layer: Some(layer.clone()),
            density_layer: None,
            exclusion_layers: vec![],
            parameters: [("slope_falloff".into(), 0.0)].into_iter().collect(),
        });
        terrain.rules.push(RuleDefinition {
            id: StableId::from_u128(20_002),
            algorithm: scene_data::rules::GRASS_SCATTER.into(),
            version: 1,
            seed: 17,
            input_layers: vec![layer.clone()],
            input_rules: vec![producer],
            output_layer: None,
            density_layer: Some(layer),
            exclusion_layers: vec![],
            parameters: [
                ("density".into(), 0.05),
                ("candidates_per_cell".into(), 1.0),
            ]
            .into_iter()
            .collect(),
        });
    }
    source.objects.push(ObjectRecord {
        id: StableId::from_u128(9000),
        parent: None,
        sibling_order: 9000,
        name: "Profile terrain".into(),
        local: Default::default(),
        data: ObjectData::Terrain(terrain),
    });
    source
}
fn command(handle: &mut EditorHandle, id: u64, command: Command) -> CommandOutcome {
    let before = handle.snapshot();
    handle
        .submit(CommandRequest {
            request: id,
            session: before.session,
            expected_revision: before.document_revision,
            expected_inspector_revision: None,
            command,
        })
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut result = None;
    while Instant::now() < deadline {
        for event in handle.poll(128) {
            if let ServiceEvent::Command(reply) = event
                && reply.request == id
            {
                result = Some(reply.outcome.unwrap());
            }
        }
        if handle.snapshot().view_revision > before.view_revision
            && let Some(outcome) = result.take()
        {
            return outcome;
        }
        std::thread::sleep(Duration::from_micros(100));
    }
    panic!("command timeout");
}
fn geometry(handle: &mut EditorHandle) -> (JobOutput, u64) {
    let ticket = handle.job(JobOperation::Geometry).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        for event in handle.poll(128) {
            if let ServiceEvent::JobFinished { fact, result } = event
                && fact.id == ticket.id
            {
                return (result.unwrap(), fact.elapsed_us.unwrap());
            }
        }
        std::thread::sleep(Duration::from_micros(100));
    }
    panic!("geometry timeout");
}
fn elapsed(start: Instant) -> u64 {
    start.elapsed().as_micros() as u64
}
fn main() {
    let mut args = std::env::args().skip(1);
    let output = args.next().expect("output.jsonl");
    let label = args.next().expect("baseline or optimized label");
    match args.next().as_deref() {
        Some("burst") => {
            burst(&output, &label);
            return;
        }
        Some("work") => {
            work(&output);
            return;
        }
        _ => {}
    }
    let mut file = BufWriter::new(File::create(output).unwrap());
    let mut cases = vec![(0, 0, false)];
    for side in [1, 2, 4, 8] {
        for points in [1, 16, 64, 256] {
            cases.push((side, points, false));
        }
        cases.push((side, 64, true));
    }
    for (side, points, rules) in cases {
        let source = scene(side, rules);
        let samples = source
            .objects
            .iter()
            .find_map(|o| match &o.data {
                ObjectData::Terrain(t) => Some(t.samples.len()),
                _ => None,
            })
            .unwrap_or(0);
        let mut handle = EditorHandle::start(source.clone()).unwrap();
        let mut cache = GroundCache::default();
        let mut request_id = 0;
        for iteration in 0..14 {
            request_id += 1;
            let CommandOutcome::TransactionStarted(transaction) = command(
                &mut handle,
                request_id,
                Command::BeginViewportEdit {
                    selection_revision: 0,
                },
            ) else {
                panic!()
            };
            let amount = 0.25 * (1 + iteration % 3) as f32;
            request_id += 1;
            let start = Instant::now();
            if side == 0 {
                let mut value = source.objects[0].local.clone();
                value.translation_m[0] += amount;
                command(
                    &mut handle,
                    request_id,
                    Command::Preview {
                        transaction,
                        sequence: 1,
                        mutations: vec![Mutation::Transform {
                            object: source.objects[0].id.clone(),
                            value,
                        }],
                    },
                );
            } else {
                let path_world_m = (0..points)
                    .map(|i| {
                        [
                            if points == 1 {
                                16.0
                            } else {
                                4.0 + 24.0 * i as f32 / (points - 1) as f32
                            },
                            0.0,
                            16.0,
                        ]
                    })
                    .collect();
                command(
                    &mut handle,
                    request_id,
                    Command::PreviewTerrainSculpt {
                        transaction,
                        sequence: 1,
                        request: TerrainSculptRequest {
                            object: StableId::from_u128(9000),
                            path_world_m,
                            radius_m: 2.0,
                            strength: 1.0,
                            falloff: 1.0,
                            mode: TerrainSculptMode::Raise { amount_m: amount },
                        },
                    },
                );
            }
            let command_us = elapsed(start);
            let start = Instant::now();
            let (result, worker_us) = geometry(&mut handle);
            let geometry_end_to_end_us = elapsed(start);
            let JobOutput::Geometry {
                binding,
                simulation,
                visual,
                timings,
            } = result
            else {
                panic!()
            };
            let snapshot = handle.snapshot();
            assert!(binding.matches(&snapshot));
            let dirty = &snapshot.preview.as_ref().unwrap().dirty;
            let facts = cache
                .update_with_visual(&simulation.ground, Some(&visual), dirty)
                .unwrap();
            let row = serde_json::json!({"label":label,"tile_side":side,"tiles":side*side,"cells_per_tile":32,"stroke_points":points,"derived_rules":rules,
                "iteration":iteration,"warmup":iteration<2,"boxes":14,"spawns":8,"height_samples":samples,"dirty_tiles":dirty.len(),
                "command_us":command_us,"geometry_end_to_end_us":geometry_end_to_end_us,"worker_us":worker_us,
                "validation_us":timings.validation_us,"geometry_us":timings.geometry_us,"rule_us":timings.rule_us,"encoding_us":timings.encoding_us,"total_us":timings.total_us,
                "renderer_preparation_us":facts.preparation_us,"tiles_examined":facts.tiles_examined,"tiles_rebuilt":facts.tiles_rebuilt,"prepared_bytes":facts.prepared_bytes});
            serde_json::to_writer(&mut file, &row).unwrap();
            writeln!(file).unwrap();
            request_id += 1;
            command(
                &mut handle,
                request_id,
                Command::CancelViewportEdit { transaction },
            );
            assert_eq!(handle.snapshot().document.as_ref(), &source);
        }
        file.flush().unwrap();
        println!("finished {label}: {side}x{side} tiles, {points} points, rules={rules}");
    }
}

/// Saturate the bounded channel with24 previews and24 geometry tickets. All commands
/// have the same base revision; latest output must match the last successful sequence.
fn burst(output: &str, label: &str) {
    let mut file = BufWriter::new(File::create(output).unwrap());
    for iteration in 0..8 {
        let source = scene(8, true);
        let mut handle = EditorHandle::start(source.clone()).unwrap();
        let CommandOutcome::TransactionStarted(transaction) = command(
            &mut handle,
            1,
            Command::BeginViewportEdit {
                selection_revision: 0,
            },
        ) else {
            panic!()
        };
        let start = Instant::now();
        let mut tickets = std::collections::BTreeSet::new();
        for sequence in 1..=24 {
            let base = handle.snapshot();
            handle
                .submit(CommandRequest {
                    request: sequence + 1,
                    session: base.session,
                    expected_revision: base.document_revision,
                    expected_inspector_revision: None,
                    command: Command::PreviewTerrainSculpt {
                        transaction,
                        sequence,
                        request: TerrainSculptRequest {
                            object: StableId::from_u128(9000),
                            path_world_m: (0..64)
                                .map(|i| [4.0 + 24.0 * i as f32 / 63.0, 0.0, 16.0])
                                .collect(),
                            radius_m: 2.0,
                            strength: 1.0,
                            falloff: 1.0,
                            mode: TerrainSculptMode::Raise {
                                amount_m: sequence as f32 * 0.01,
                            },
                        },
                    },
                })
                .unwrap();
            tickets.insert(handle.job(JobOperation::Geometry).unwrap().id);
        }
        let mut completed = 0;
        let mut cancelled = 0;
        let mut worker_us = 0;
        let mut latest = None;
        let deadline = Instant::now() + Duration::from_secs(60);
        while !tickets.is_empty() && Instant::now() < deadline {
            for event in handle.poll(128) {
                match event {
                    ServiceEvent::Command(result) => {
                        result.outcome.unwrap();
                    }
                    ServiceEvent::JobFinished { fact, result } => {
                        assert!(tickets.remove(&fact.id));
                        worker_us += fact.elapsed_us.unwrap();
                        match result {
                            Ok(output) => {
                                completed += 1;
                                latest = Some(output);
                            }
                            Err(error) if error.code == "job.cancelled" => cancelled += 1,
                            Err(error) => panic!("{error:?}"),
                        }
                    }
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_micros(100));
        }
        assert!(tickets.is_empty());
        let elapsed_us = elapsed(start);
        // The terminal snapshot can trail the last result by one event.
        while handle.snapshot().preview.as_ref().map(|p| p.sequence) != Some(24)
            && Instant::now() < deadline
        {
            handle.poll(128);
        }
        let Some(JobOutput::Geometry { binding, .. }) = latest else {
            panic!()
        };
        assert_eq!(binding.preview, Some((transaction, 24)));
        assert!(binding.matches(&handle.snapshot()));
        serde_json::to_writer(&mut file,&serde_json::json!({"label":label,"iteration":iteration,"warmup":iteration==0,"tiles":64,"stroke_points":64,"queued_previews":24,
            "completed":completed,"cancelled":cancelled,"end_to_end_us":elapsed_us,"worker_us":worker_us})).unwrap();
        writeln!(file).unwrap();
        command(
            &mut handle,
            100,
            Command::CancelViewportEdit { transaction },
        );
        assert_eq!(handle.snapshot().document.as_ref(), &source);
    }
    file.flush().unwrap();
}

/// Functional scope evidence, separate from timed service path (no full publication).
fn work(output: &str) {
    let mut rows = vec![];
    for side in [1, 2, 4, 8] {
        for rules in [false, true] {
            let source = scene(side, rules);
            let mut cache = editor_cook::GeometryCache::default();
            cache
                .cook(std::sync::Arc::new(source.clone()), &|| Ok(()))
                .unwrap();
            let mut handle = EditorHandle::start(source).unwrap();
            let CommandOutcome::TransactionStarted(transaction) = command(
                &mut handle,
                1,
                Command::BeginViewportEdit {
                    selection_revision: 0,
                },
            ) else {
                panic!()
            };
            command(
                &mut handle,
                2,
                Command::PreviewTerrainSculpt {
                    transaction,
                    sequence: 1,
                    request: TerrainSculptRequest {
                        object: StableId::from_u128(9000),
                        path_world_m: (0..64)
                            .map(|i| [4.0 + 24.0 * i as f32 / 63.0, 0.0, 16.0])
                            .collect(),
                        radius_m: 2.0,
                        strength: 1.0,
                        falloff: 1.0,
                        mode: TerrainSculptMode::Raise { amount_m: 0.25 },
                    },
                },
            );
            let view = handle.snapshot();
            let source = view.preview.as_ref().unwrap().document.clone();
            let package = cache.cook(source.clone(), &|| Ok(())).unwrap();
            let full = editor_cook::cook(&source).unwrap();
            assert_eq!(
                serde_json::to_vec(package.simulation.as_ref()).unwrap(),
                full.simulation_bytes
            );
            assert_eq!(
                serde_json::to_vec(package.visual.as_ref()).unwrap(),
                full.visual_bytes
            );
            let w = package.work;
            rows.push(serde_json::json!({"tiles":side*side,"rules":rules,"changed_samples":w.changed_samples,"rebuilt_tiles":w.rebuilt_tiles,"context_tiles":w.context_tiles,"reused_tiles":w.reused_tiles,"derived_samples_evaluated":w.derived_samples_evaluated,"scatter_cells_visited":w.scatter_cells_visited,"full_scope_cells":side*side*32*32,"full_scope_derived_sample_duplicates":side*side*33*33}));
        }
    }
    std::fs::write(output, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
}
