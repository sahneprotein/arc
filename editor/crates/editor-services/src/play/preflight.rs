use crate::{package::verify_package_payloads, VerifiedPackageReceipt};
use scene_data::{Diagnostic, SimulationPayload, VisualPayload};
use std::sync::atomic::AtomicBool;
pub(crate) fn verify(
    receipt: &VerifiedPackageReceipt,
    cancel: &AtomicBool,
) -> Result<(), Diagnostic> {
    let (_, simulation, visual) =
        verify_package_payloads(receipt.path(), Some(receipt.identity()), cancel)?;
    capabilities(&Subset::of(&simulation, &visual))
}
/// What of a verified package decides whether the game runtime can run it. The
/// payloads themselves were validated by the same read that produced them.
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
fn capabilities(subset: &Subset) -> Result<(), Diagnostic> {
    let unsupported = [
        ("rules", subset.rules),
        ("decoration", subset.decoration),
        ("derived_layers", subset.derived_layers),
    ];
    if let Some((reason, count)) = unsupported.into_iter().find(|(_, count)| *count != 0) {
        return Err(Diagnostic::new("play.terrain_unsupported")
            .with("reason", reason)
            .with("count", count));
    }
    if subset.spawns == 0 {
        return Err(Diagnostic::new("play.spawn_required"));
    }
    if subset.boxes > 100_000 || subset.spawns > 100_000 {
        return Err(Diagnostic::new("play.runtime_limit"));
    }
    Ok(())
}
/// One verifier thread and one queued preparation per coordinator. Cancellation
/// cannot interrupt a blocked filesystem call, but repeated starts cannot create
/// an unbounded population of verifier threads.
pub(crate) struct Worker {
    sender: std::sync::mpsc::SyncSender<Work>,
}
struct Work {
    receipt: VerifiedPackageReceipt,
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
                    let _ = work.result.try_send(verify(&work.receipt, &work.cancel));
                }
            })
            .map_err(|e| Diagnostic::new("play.preflight_start_failed").with("error", e))?;
        Ok(Self { sender })
    }
    pub fn submit(
        &self,
        receipt: VerifiedPackageReceipt,
        cancel: std::sync::Arc<AtomicBool>,
    ) -> Result<std::sync::mpsc::Receiver<Result<(), Diagnostic>>, Diagnostic> {
        let (result, receiver) = std::sync::mpsc::sync_channel(1);
        self.sender
            .try_send(Work {
                receipt,
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
            },
        )
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
