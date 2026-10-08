//! Small headless conversion/cook entrypoint; no engine/UI dependency.
use editor_services::*;
use scene_data::*;
use std::{path::Path, sync::atomic::AtomicBool, time::Instant};
fn main() {
    if let Err(error) = run() {
        eprintln!("{}", serde_json::to_string(&error).unwrap());
        std::process::exit(1);
    }
}
fn run() -> Result<(), Diagnostic> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let cancel = AtomicBool::new(false);
    match args.as_slice() {
        [command, output] if command == "convert-current-map" => {
            let receipt = save_document(
                Path::new(output),
                &current_map(),
                DiskVersion::Absent,
                &cancel,
            )?;
            println!(
                "{}",
                serde_json::json!({"operation":"convert-current-map","run":RunMetadata::local("current-map-conversion"),"source_hash":receipt.source_hash.hex(),"save_us":receipt.total_us,"warning":receipt.durability_warning})
            );
            Ok(())
        }
        [command, input, output] if command == "cook" => {
            let loaded = load_document(Path::new(input), &cancel)?;
            let package = editor_cook::cook(&loaded.document)?;
            let publication = Instant::now();
            let path = publish_package(Path::new(output), &package, &cancel)?;
            let publication_us = publication.elapsed().as_micros();
            println!(
                "{}",
                serde_json::json!({"operation":"cook","run":RunMetadata::local("current-map-cook"),"publication_us":publication_us,"package":path,"source_hash":package.source_hash.hex(),"simulation_hash":package.simulation_hash.hex(),"visual_hash":package.visual_hash.hex(),
                "open":{"read_us":loaded.timings.read_us,"decode_us":loaded.timings.decode_us,"validation_us":loaded.timings.validation_us,"total_us":loaded.timings.total_us},
                "cook":{"validation_us":package.timings.validation_us,"geometry_us":package.timings.geometry_us,"encoding_us":package.timings.encoding_us,"rule_us":package.timings.rule_us,"total_us":package.timings.total_us},
                "simulation_bytes":package.simulation_bytes.len(),"visual_bytes":package.visual_bytes.len()})
            );
            Ok(())
        }
        [command, input] if command == "verify" => {
            let (simulation, _) = load_package(Path::new(input), &cancel)?;
            println!(
                "{}",
                serde_json::json!({"operation":"verify","run":RunMetadata::local("package-verify"),"boxes":simulation.boxes.len(),"spawns":simulation.spawns.len(),"tiles":simulation.ground.len()})
            );
            Ok(())
        }
        _ => Err(Diagnostic::new("cli.usage").with(
            "usage",
            "arc-scene convert-current-map FILE | cook FILE PACKAGE_ROOT | verify PACKAGE",
        )),
    }
}
