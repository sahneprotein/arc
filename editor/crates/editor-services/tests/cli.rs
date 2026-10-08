use std::{fs, process::Command};
fn cli(args: &[&std::ffi::OsStr]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_arc-scene"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn convert_cook_verify_cli_roundtrip_and_repeatable_package() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("current.arcscene");
    let packages = root.path().join("packages");
    let conversion = cli(&["convert-current-map".as_ref(), source.as_os_str()]);
    assert!(conversion.status.success(), "{:?}", conversion);
    let converted: serde_json::Value = serde_json::from_slice(&conversion.stdout).unwrap();
    assert_eq!(
        converted["source_hash"],
        "72e78f8d0abb4e41e26067af3e94b3403946241d477e1085b93d15e19a6c8eb4"
    );
    let original = fs::read(&source).unwrap();
    let scene: scene_data::SceneDocument = serde_json::from_slice(&original).unwrap();
    assert_eq!(scene, scene_data::current_map());
    let duplicate = cli(&["convert-current-map".as_ref(), source.as_os_str()]);
    assert!(!duplicate.status.success());
    assert_eq!(fs::read(&source).unwrap(), original);
    let mut cooked = Vec::new();
    for _ in 0..2 {
        let output = cli(&["cook".as_ref(), source.as_os_str(), packages.as_os_str()]);
        assert!(output.status.success(), "{:?}", output);
        cooked.push(serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap());
    }
    for field in [
        "package",
        "source_hash",
        "simulation_hash",
        "visual_hash",
        "simulation_bytes",
        "visual_bytes",
    ] {
        assert_eq!(cooked[0][field], cooked[1][field]);
    }
    assert_eq!(fs::read_dir(&packages).unwrap().count(), 1);
    let package = std::path::Path::new(cooked[0]["package"].as_str().unwrap());
    let verified = cli(&["verify".as_ref(), package.as_os_str()]);
    assert!(verified.status.success());
    let facts: serde_json::Value = serde_json::from_slice(&verified.stdout).unwrap();
    assert_eq!(facts["boxes"], 14);
    assert_eq!(facts["spawns"], 8);
    assert_eq!(facts["tiles"], 0);
    fs::write(package.join("level.sim.json"), b"truncated").unwrap();
    let corrupted = cli(&["verify".as_ref(), package.as_os_str()]);
    assert!(!corrupted.status.success());
    let failure: serde_json::Value = serde_json::from_slice(&corrupted.stderr).unwrap();
    assert_eq!(failure["code"], "payload.hash_mismatch");
}
