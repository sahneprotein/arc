use crate::{package::verify_package_payloads, VerifiedPackageReceipt};
use scene_data::Diagnostic;
use std::sync::atomic::AtomicBool;
pub(crate) fn verify(
    receipt: &VerifiedPackageReceipt,
    cancel: &AtomicBool,
) -> Result<(), Diagnostic> {
    let (_, simulation, visual) =
        verify_package_payloads(receipt.path(), Some(receipt.identity()), cancel)?;
    capabilities(
        simulation.ground.len(),
        visual.terrain.len(),
        simulation.spawns.len(),
        simulation.boxes.len(),
    )
}
fn capabilities(
    ground: usize,
    terrain: usize,
    spawns: usize,
    boxes: usize,
) -> Result<(), Diagnostic> {
    if ground != 0 || terrain != 0 {
        return Err(Diagnostic::new("play.terrain_unsupported"));
    }
    if spawns == 0 {
        return Err(Diagnostic::new("play.spawn_required"));
    }
    if boxes > 100_000 || spawns > 100_000 {
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
    #[test]
    fn runtime_subset_rejects_ground_visual_terrain_missing_spawns_and_limits() {
        assert!(capabilities(0, 0, 1, 1).is_ok());
        for (ground, terrain) in [(1, 0), (0, 1), (1, 1)] {
            assert_eq!(
                capabilities(ground, terrain, 1, 1).unwrap_err().code,
                "play.terrain_unsupported"
            );
        }
        assert_eq!(
            capabilities(0, 0, 0, 1).unwrap_err().code,
            "play.spawn_required"
        );
        assert_eq!(
            capabilities(0, 0, 1, 100_001).unwrap_err().code,
            "play.runtime_limit"
        );
    }
}
