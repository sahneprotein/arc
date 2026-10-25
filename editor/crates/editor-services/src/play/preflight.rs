use crate::{package::verify_package_payloads, VerifiedPackageReceipt};
use scene_data::{CookedEmitter, Diagnostic};
#[cfg(test)]
use scene_data::{SimulationPayload, VisualPayload};
use std::{path::Path, sync::atomic::AtomicBool};
/// The optional project root owns project settings and reusable emitter files referenced
/// by the cooked visual payload. Standalone launches keep using the runtime asset root.
pub(crate) fn verify(
    receipt: &VerifiedPackageReceipt,
    project_root: Option<&Path>,
    standalone_content_root: &Path,
    cancel: &AtomicBool,
) -> Result<(), Diagnostic> {
    let (_, simulation, visual) =
        verify_package_payloads(receipt.path(), Some(receipt.identity()), cancel)?;
    if let Some(problem) = editor_core::review_play_payloads(&simulation, &visual)
        .into_iter()
        .next()
    {
        return Err(problem.diagnostic);
    }
    let emitter_root = project_root.unwrap_or(standalone_content_root);
    if let Some(project_root) = project_root {
        check_project_play_inputs(project_root)?;
    }
    placed_emitters(&visual.emitters, emitter_root)
}

/// Read-only project prerequisites for Play. The editor may cache this result and refresh
/// it after project, level or profile changes; coordinator preflight calls the same checker
/// again immediately before launching children.
pub fn check_project_play_inputs(project_root: &Path) -> Result<(), Diagnostic> {
    match crate::project::inspect(project_root).status {
        crate::project::ProjectInspectionStatus::Ready { .. } => {}
        crate::project::ProjectInspectionStatus::MissingStartLevel { start_level, .. } => {
            return Err(Diagnostic::new("play.project_start_level_missing")
                .with("path", start_level.display()));
        }
        crate::project::ProjectInspectionStatus::Refused { problem } => {
            return Err(problem);
        }
        crate::project::ProjectInspectionStatus::Missing
        | crate::project::ProjectInspectionStatus::SetupRequired { .. } => {
            return Err(Diagnostic::new("play.project_manifest_unavailable").with(
                "path",
                project_root.join(crate::project::PROJECT_FILE).display(),
            ));
        }
    }
    let config = crate::project::load_project(project_root)?;
    crate::project::look_profile(project_root, &config.rendering.profile).map_err(|problem| {
        Diagnostic::new("play.project_profile_unavailable")
            .with("profile", &config.rendering.profile)
            .with("cause", &problem.code)
    })?;
    Ok(())
}
/// Every emitter the cooked level places is still the file the level was cooked with, can
/// be read, and is one the game's particle runtime draws. The first that is not stops the
/// start, with the placement as the refusal's object: a file that changed since the cook
/// means the level is to be cooked again, not that the client finds out.
pub(crate) fn placed_emitters(
    emitters: &[CookedEmitter],
    project_root: &Path,
) -> Result<(), Diagnostic> {
    for emitter in emitters {
        let placed = |refusal: Diagnostic| {
            refusal
                .with("emitter_id", &emitter.emitter_id.0)
                .at(&emitter.id)
        };
        let now = crate::emitters::cooked_emitter(Some(project_root), &emitter.emitter_id.0)
            .map_err(placed)?;
        if now != emitter.emitter_sha256 {
            return Err(placed(Diagnostic::new("emitter.changed_since_cook")));
        }
    }
    Ok(())
}
/// What of a verified package decides whether the game runtime can run it. The
/// payloads themselves were validated by the same read that produced them.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Subset {
    spawns: usize,
    boxes: usize,
    /// Procedural terrain content across all visual terrain: rule definitions,
    /// scattered decoration instances and rule-derived layers.
    rules: usize,
    decoration: usize,
    derived_layers: usize,
}
#[cfg(test)]
impl Subset {
    fn of(simulation: &SimulationPayload, visual: &VisualPayload) -> Self {
        let sum = |count: fn(&scene_data::VisualTerrain) -> usize| -> usize {
            visual.terrain.iter().map(count).sum()
        };
        Self {
            spawns: simulation.spawns.len(),
            boxes: simulation.boxes.len(),
            rules: sum(|terrain| terrain.rules.len()),
            decoration: sum(|terrain| terrain.decoration.len()),
            derived_layers: sum(|terrain| terrain.derived_layers.len()),
        }
    }
}
/// The runtime runs cooked ground collision and authored terrain paint (blend
/// weights and masks). It does not run procedural terrain: a package carrying
/// rules, decoration or derived layers is refused before any child is spawned,
/// with the first unsupported kind as `reason`.
#[cfg(test)]
fn capabilities(subset: &Subset) -> Result<(), Diagnostic> {
    editor_core::play_problems(&editor_core::PlayCapabilityFacts {
        spawns: subset.spawns,
        boxes: subset.boxes,
        terrain: vec![editor_core::ProceduralTerrainFacts {
            object: scene_data::StableId::from_u128(1),
            rules: subset.rules,
            decoration: subset.decoration,
            derived_layers: subset.derived_layers,
        }],
    })
    .into_iter()
    .next()
    .map_or(Ok(()), |problem| Err(problem.diagnostic))
}
/// One verifier thread and one queued preparation per coordinator. Cancellation
/// cannot interrupt a blocked filesystem call, but repeated starts cannot create
/// an unbounded population of verifier threads.
pub(crate) struct Worker {
    sender: std::sync::mpsc::SyncSender<Work>,
}
struct Work {
    receipt: VerifiedPackageReceipt,
    project_root: Option<std::path::PathBuf>,
    standalone_content_root: std::path::PathBuf,
    cancel: std::sync::Arc<AtomicBool>,
    result: std::sync::mpsc::SyncSender<Result<(), Diagnostic>>,
}
impl Worker {
    pub fn new() -> Result<Self, Diagnostic> {
        let (sender, receiver) = std::sync::mpsc::sync_channel::<Work>(1);
        std::thread::Builder::new()
            .name("play-preflight".into())
            .spawn(move || {
                for work in receiver {
                    let _ = work.result.try_send(verify(
                        &work.receipt,
                        work.project_root.as_deref(),
                        &work.standalone_content_root,
                        &work.cancel,
                    ));
                }
            })
            .map_err(|e| Diagnostic::new("play.preflight_start_failed").with("error", e))?;
        Ok(Self { sender })
    }
    pub fn submit(
        &self,
        receipt: VerifiedPackageReceipt,
        project_root: Option<std::path::PathBuf>,
        standalone_content_root: std::path::PathBuf,
        cancel: std::sync::Arc<AtomicBool>,
    ) -> Result<std::sync::mpsc::Receiver<Result<(), Diagnostic>>, Diagnostic> {
        let (result, receiver) = std::sync::mpsc::sync_channel(1);
        self.sender
            .try_send(Work {
                receipt,
                project_root,
                standalone_content_root,
                cancel,
                result,
            })
            .map_err(|e| {
                Diagnostic::new(match e {
                    std::sync::mpsc::TrySendError::Full(_) => "play.preflight_busy",
                    std::sync::mpsc::TrySendError::Disconnected(_) => "play.preflight_closed",
                })
            })?;
        Ok(receiver)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scene_data::{DerivedTerrainLayer, StableId, VisualTerrain, COOK_SCHEMA};

    #[test]
    fn project_preflight_uses_the_unique_renamed_aproj() {
        let folder = tempfile::tempdir().unwrap();
        let setup = crate::project::create_project(folder.path(), "Test").unwrap();
        let renamed = folder.path().join("Test.aproj");
        std::fs::rename(&setup.config_path, &renamed).unwrap();
        assert!(check_project_play_inputs(folder.path()).is_ok());
        std::fs::write(&setup.config_path, b"another project").unwrap();
        assert_eq!(
            check_project_play_inputs(folder.path()).unwrap_err().code,
            "project.multiple_files"
        );
    }

    #[test]
    fn project_preflight_refuses_a_missing_configured_source_level() {
        let folder = tempfile::tempdir().unwrap();
        let setup = crate::project::create_project(folder.path(), "Missing Level").unwrap();
        std::fs::remove_file(&setup.start_level_path).unwrap();

        let problem = check_project_play_inputs(folder.path()).unwrap_err();
        assert_eq!(problem.code, "play.project_start_level_missing");
        let relative_level = setup.start_level_path.strip_prefix(folder.path()).unwrap();
        let expected_level = std::fs::canonicalize(folder.path())
            .unwrap()
            .join(relative_level);
        assert_eq!(
            problem.parameters.get("path"),
            Some(&expected_level.display().to_string())
        );
    }

    #[test]
    fn project_preflight_reads_the_configured_render_profile_before_play() {
        let folder = tempfile::tempdir().unwrap();
        crate::project::create_project(folder.path(), "Missing Look").unwrap();
        let profile = folder.path().join("assets/render/look-v1.json");
        std::fs::remove_file(&profile).unwrap();

        let problem = check_project_play_inputs(folder.path()).unwrap_err();
        assert_eq!(problem.code, "play.project_profile_unavailable");
        assert_eq!(
            problem.parameters.get("profile").unwrap(),
            "assets/render/look-v1.json"
        );
        assert_eq!(problem.parameters.get("cause").unwrap(), "look.missing");

        std::fs::write(&profile, b"not a look profile").unwrap();
        let problem = check_project_play_inputs(folder.path()).unwrap_err();
        assert_eq!(problem.code, "play.project_profile_unavailable");
        assert_eq!(problem.parameters.get("cause").unwrap(), "look.decode");
    }

    #[test]
    fn coordinator_preflight_rechecks_project_inputs_after_package_verification() {
        let folder = tempfile::tempdir().unwrap();
        let project = folder.path().join("project");
        let setup = crate::project::create_project(&project, "Missing Level").unwrap();
        let scene = scene_data::starter_level(StableId::from_u128(700));
        let cooked = editor_cook::cook(&scene).unwrap();
        let cancel = AtomicBool::new(false);
        let package_root = folder.path().join("packages");
        let package_path = crate::publish_package(&package_root, &cooked, &cancel).unwrap();
        let receipt = crate::verify_package(&package_path, None, &cancel).unwrap();
        assert!(verify(&receipt, None, folder.path(), &cancel).is_ok());
        std::fs::remove_file(&setup.start_level_path).unwrap();

        let problem = verify(&receipt, Some(&project), folder.path(), &cancel).unwrap_err();
        assert_eq!(problem.code, "play.project_start_level_missing");
    }

    #[test]
    fn preflight_refusal_is_the_same_object_scoped_problem_as_review() {
        let visual = VisualPayload {
            schema: COOK_SCHEMA,
            terrain: vec![terrain(vec![DerivedTerrainLayer {
                rule: StableId::from_u128(10),
                layer: StableId::from_u128(11),
                tile: StableId::from_u128(12),
                side_samples: 2,
                weights: vec![0.; 4],
            }])],
            emitters: Vec::new(),
        };
        let (simulation, _) = payloads(Vec::new());
        let problems = editor_core::review_play_payloads(&simulation, &visual);
        assert_eq!(problems.len(), 2);
        assert_eq!(problems[0].diagnostic.code, "play.terrain_unsupported");
        assert_eq!(problems[0].diagnostic.object, Some(StableId::from_u128(1)));
        assert_eq!(
            problems[0].diagnostic.parameters["reason"],
            "derived_layers"
        );
        assert_eq!(problems[1].diagnostic.code, "play.spawn_required");
    }

    fn playable() -> Subset {
        Subset {
            spawns: 1,
            boxes: 1,
            ..Subset::default()
        }
    }
    fn terrain(derived_layers: Vec<DerivedTerrainLayer>) -> VisualTerrain {
        VisualTerrain {
            rules: Vec::new(),
            decoration: Vec::new(),
            derived_layers,
            surface_group: None,
            object: StableId::from_u128(1),
            layers: Vec::new(),
            tiles: Vec::new(),
        }
    }
    fn payloads(terrain: Vec<VisualTerrain>) -> (SimulationPayload, VisualPayload) {
        (
            SimulationPayload {
                schema: COOK_SCHEMA,
                profile: String::new(),
                boxes: Vec::new(),
                spawns: Vec::new(),
                ground: Vec::new(),
            },
            VisualPayload {
                schema: COOK_SCHEMA,
                terrain,
                emitters: Vec::new(),
            },
        )
    }

    #[test]
    fn a_placed_emitter_must_still_be_the_file_the_level_was_cooked_with() {
        use crate::emitters::{cooked_emitter, save_emitter, OnDisk};
        let project = tempfile::tempdir().unwrap();
        let runtime = tempfile::tempdir().unwrap();
        let mut sparks = particle_spec::default_emitter();
        sparks.id = StableId::from_u128(5).0;
        save_emitter(project.path(), &sparks, &OnDisk::Absent).unwrap();
        let placed = |emitter: u128, hash| scene_data::CookedEmitter {
            id: StableId::from_u128(900 + emitter),
            emitter_id: StableId::from_u128(emitter),
            emitter_sha256: hash,
            transform: scene_data::RigidTransform::default(),
        };
        let as_cooked = cooked_emitter(Some(project.path()), &sparks.id).unwrap();
        assert_eq!(placed_emitters(&[], project.path()), Ok(()));
        assert_eq!(
            placed_emitters(&[placed(5, as_cooked)], project.path()),
            Ok(())
        );
        assert_eq!(
            placed_emitters(&[placed(5, as_cooked)], runtime.path())
                .unwrap_err()
                .code,
            "emitter.asset_missing"
        );

        // The emitter was saved again after the cook: the level is to be cooked again.
        let changed = placed_emitters(
            &[placed(5, scene_data::ContentHash([0; 32]))],
            project.path(),
        )
        .unwrap_err();
        assert_eq!(changed.code, "emitter.changed_since_cook");
        assert_eq!(changed.object, Some(StableId::from_u128(905)));
        assert_eq!(changed.parameters["emitter_id"], sparks.id);

        // A file that is gone is said as that, for its placement, and the first one counts.
        let missing = placed_emitters(
            &[placed(5, as_cooked), placed(6, as_cooked)],
            project.path(),
        )
        .unwrap_err();
        assert_eq!(missing.code, "emitter.asset_missing");
        assert_eq!(missing.object, Some(StableId::from_u128(906)));
    }

    #[test]
    fn runtime_subset_rejects_missing_spawns_and_limits() {
        assert!(capabilities(&playable()).is_ok());
        assert_eq!(
            capabilities(&Subset {
                spawns: 0,
                ..playable()
            })
            .unwrap_err()
            .code,
            "play.spawn_required"
        );
        assert_eq!(
            capabilities(&Subset {
                boxes: 100_001,
                ..playable()
            })
            .unwrap_err()
            .code,
            "play.runtime_limit"
        );
    }

    #[test]
    fn runtime_subset_rejects_each_procedural_terrain_kind_with_its_reason() {
        for (reason, subset) in [
            (
                "rules",
                Subset {
                    rules: 1,
                    ..playable()
                },
            ),
            (
                "decoration",
                Subset {
                    decoration: 3,
                    ..playable()
                },
            ),
            (
                "derived_layers",
                Subset {
                    derived_layers: 2,
                    ..playable()
                },
            ),
        ] {
            let refused = capabilities(&subset).unwrap_err();
            assert_eq!(refused.code, "play.terrain_unsupported", "{reason}");
            assert_eq!(
                refused.parameters.get("reason").map(String::as_str),
                Some(reason)
            );
        }
        // Unsupported terrain is reported before a missing spawn: it names what
        // must change in the level's landscape first.
        let refused = capabilities(&Subset {
            rules: 1,
            spawns: 0,
            ..playable()
        })
        .unwrap_err();
        assert_eq!(refused.code, "play.terrain_unsupported");
    }

    #[test]
    fn cooked_ground_and_authored_paint_are_inside_the_runtime_subset() {
        // Terrain with layers/tiles but nothing procedural counts as nothing
        // unsupported: only spawns and limits remain to be judged.
        let (simulation, visual) = payloads(vec![terrain(Vec::new()), terrain(Vec::new())]);
        let subset = Subset::of(&simulation, &visual);
        assert_eq!(subset, Subset::default());
        assert!(capabilities(&Subset {
            spawns: 1,
            ..subset
        })
        .is_ok());
    }

    #[test]
    fn procedural_content_is_counted_across_every_visual_terrain() {
        let derived = |tile: u128| DerivedTerrainLayer {
            rule: StableId::from_u128(10),
            layer: StableId::from_u128(11),
            tile: StableId::from_u128(tile),
            side_samples: 2,
            weights: vec![0.0; 4],
        };
        let (simulation, visual) = payloads(vec![
            terrain(Vec::new()),
            terrain(vec![derived(1), derived(2)]),
        ]);
        let subset = Subset::of(&simulation, &visual);
        assert_eq!(subset.derived_layers, 2);
        assert_eq!((subset.rules, subset.decoration), (0, 0));
    }
}
