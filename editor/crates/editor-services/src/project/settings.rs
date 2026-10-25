//! Changing the settings of a project that is read: the keys the game uses, the look
//! profile it is drawn with, and how many clients Play starts on this machine.
//!
//! A save is asked for with the manifest as the editor read it and with what is to be
//! different. Everything else is taken from the file as it is on disk at that moment: the
//! schema, the name, the level the project starts with and the settings the game does not
//! read yet are never written from what an editor holds. So a save cannot set a setting
//! that is still planned, and it cannot put back something another program has changed
//! since. The number of local test clients is the one planned setting that is read, and so
//! the one a save sets; a manifest that names none keeps naming none until a number is set.
//!
//! Nothing is written over unseen. The manifest is held with the same lock file a conversion
//! makes (`project.locked`). If it is no longer what the editor read, the save is refused
//! (`project.changed_since_read`) and the file is left as it is. It is read once more just
//! before it is replaced, for a program that does not make the lock file. The new manifest
//! is written beside the old one and put in its place in one step. A save that would change
//! nothing writes nothing.
//!
//! What is saved is a manifest the game can play with: every key is one the game reads, no
//! two actions share a key (`project.value`, with the field), and a look profile that is
//! newly chosen is a file in the project's folder that the look reader takes
//! (`project.profile_unusable`).
//!
//! A project whose look profile is not in its folder can be given the default one
//! ([`install_default_look`]): on the person's word, at the path the manifest names, and
//! only where nothing has that name. The manifest is not written for it.

use super::{
    create_exclusive, hold, inside, project_error, read_limited, replace_atomic,
    resolve_project_path,
};
use crate::look::{load_look_profile, parse_look_profile, LoadedLook};
use project_spec::{ControlSettings, ProjectConfig};
use scene_data::Diagnostic;
use std::{fs, path::Path};

/// The folder of a project in which the look profiles to choose from are looked for.
pub const LOOK_FOLDER: &str = "assets/render";

/// How many look profiles are listed at most.
pub const MAX_LOOK_CHOICES: usize = 64;

/// The look profile a project is given when it has none: the game's first profile, byte
/// for byte, so that a project made here is drawn as the game has been drawn so far.
pub const DEFAULT_LOOK_PROFILE: &[u8] = include_bytes!("../../assets/look-v1.json");

/// What a save of the settings sets: the keys, what is looked with, the look profile, and
/// how many clients Play starts on this machine.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsEdit {
    pub controls: ControlSettings,
    /// The look profile, as a path in the project's folder with `/` between its parts.
    pub rendering_profile: String,
    /// How many clients Play starts on this machine, as the manifest has it: `None` while
    /// it names none, which is one client.
    pub local_clients: Option<u8>,
}

impl SettingsEdit {
    /// The settings a manifest has now.
    pub fn of(config: &ProjectConfig) -> Self {
        Self {
            controls: config.controls.clone(),
            rendering_profile: config.rendering.profile.clone(),
            local_clients: config.planned.multiplayer.clients,
        }
    }

    /// This manifest with these settings, and everything else as it is.
    pub fn applied_to(&self, config: &ProjectConfig) -> ProjectConfig {
        let mut changed = config.clone();
        changed.controls = self.controls.clone();
        changed.rendering.profile = self.rendering_profile.clone();
        changed.planned.multiplayer.clients = self.local_clients;
        changed
    }
}

/// A look profile a project could be drawn with: its path in the project's folder, and what
/// the look reader made of it, the SHA-256 of its bytes or its refusal.
#[derive(Clone, Debug, PartialEq)]
pub struct LookChoice {
    pub path: String,
    pub read: Result<String, Diagnostic>,
}

/// Reads the look profile at this path in the project's folder. Refused when the path leads
/// out of the folder through a link (`project.outside_folder`) and when the look reader
/// refuses the file (its own `look.*` code).
pub fn look_profile(root: &Path, relative: &str) -> Result<LoadedLook, Diagnostic> {
    let path = root.join(relative);
    inside(root, &path)?;
    load_look_profile(&path)
}

/// Makes the default look profile ([`DEFAULT_LOOK_PROFILE`]) at this path in the project's
/// folder, and returns it as the look reader takes it. For a project whose manifest names a
/// profile that is not there, and only on the person's word.
///
/// Nothing that is there is written over: anything of that name, a link that leads nowhere
/// too, refuses it (`project.conflict`). A path that leads out of the folder, by its parts
/// (`project.path_invalid`) or through a link (`project.outside_folder`), is refused, and so
/// is a folder that is not there (`project.folder_missing`). The folders on the way to the
/// file are made; the manifest is not touched.
pub fn install_default_look(root: &Path, relative: &str) -> Result<LoadedLook, Diagnostic> {
    let path = root.join(relative);
    let in_folder = !relative.trim().is_empty()
        && !relative.contains('\\')
        && Path::new(relative).components().all(|part| {
            matches!(
                part,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        });
    if !in_folder {
        return Err(Diagnostic::new("project.path_invalid").with("path", path.display()));
    }
    if !root.is_dir() {
        return Err(Diagnostic::new("project.folder_missing").with("path", root.display()));
    }
    // A dangling link still occupies this name. Check where its target would land so a
    // link outside the project keeps the containment refusal, while an in-project dangling
    // link is reported as the existing file that it is.
    if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink())
        && let Err(error) = fs::canonicalize(&path)
    {
        if error.kind() == std::io::ErrorKind::NotFound {
            let target = fs::read_link(&path)
                .map_err(|error| super::io_problem("project.read", &path, error))?;
            let target = if target.is_absolute() {
                target
            } else {
                path.parent().unwrap_or(root).join(target)
            };
            inside(root, &target).map_err(|_| {
                Diagnostic::new("project.outside_folder").with("path", path.display())
            })?;
            return Err(Diagnostic::new("project.conflict").with("path", path.display()));
        }
        return Err(super::io_problem("project.read", &path, error));
    }
    inside(root, &path)?;
    create_exclusive(&path, DEFAULT_LOOK_PROFILE)?;
    parse_look_profile(DEFAULT_LOOK_PROFILE, &path)
}

/// The look profiles to choose from: the `.json` files directly in [`LOOK_FOLDER`], by
/// name, each with what the look reader made of it. Reads only; at most
/// [`MAX_LOOK_CHOICES`]. A folder that is not there, or that is a link out of the project,
/// has none.
pub fn look_profiles(root: &Path) -> Vec<LookChoice> {
    let folder = root.join(LOOK_FOLDER);
    if inside(root, &folder).is_err() {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(&folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".json") && !name.starts_with('.'))
        .collect();
    names.sort();
    names.truncate(MAX_LOOK_CHOICES);
    names
        .into_iter()
        .map(|name| {
            let path = format!("{LOOK_FOLDER}/{name}");
            let read = look_profile(root, &path).map(|look| look.sha256_hex);
            LookChoice { path, read }
        })
        .collect()
}

/// Saves the settings of the project in `root`. `expected` is the manifest as the editor
/// read it; the module's documentation says what is refused and what is written. Returns
/// the manifest as it is on disk afterwards.
pub fn save_settings(
    root: &Path,
    expected: &ProjectConfig,
    edit: &SettingsEdit,
) -> Result<ProjectConfig, Diagnostic> {
    save_with(root, expected, edit, || {})
}

/// [`save_settings`], with something done just before the manifest is replaced: for a test
/// to stand in for another program that writes the manifest just then.
fn save_with(
    root: &Path,
    expected: &ProjectConfig,
    edit: &SettingsEdit,
    before_replace: impl FnOnce(),
) -> Result<ProjectConfig, Diagnostic> {
    let path = resolve_project_path(root)?;
    inside(root, &path)?;
    super::load_project(root)?;
    let _held = hold(&path)?;
    let changed = || Diagnostic::new("project.changed_since_read").with("path", path.display());
    let bytes = read_limited(&path, project_spec::MAX_FILE_BYTES)?;
    // A manifest that can no longer be read as one of this version has changed too, and
    // says what it is now.
    let on_disk = project_spec::decode(&bytes).map_err(|error| project_error(&path, error))?;
    if &on_disk != expected {
        return Err(changed());
    }
    let wanted = edit.applied_to(&on_disk);
    let written =
        project_spec::canonical_bytes(&wanted).map_err(|error| project_error(&path, error))?;
    // A profile that is newly chosen must be one the game can be drawn with. The one the
    // manifest names already is not looked at: a save of a key does not make it worse.
    if wanted.rendering.profile != on_disk.rendering.profile {
        look_profile(root, &wanted.rendering.profile).map_err(|refusal| {
            if refusal.code == "project.outside_folder" {
                return refusal;
            }
            Diagnostic::new("project.profile_unusable")
                .with("path", path.display())
                .with("profile", &wanted.rendering.profile)
                .with("reason", &refusal.code)
        })?;
    }
    if wanted == on_disk {
        return Ok(on_disk);
    }
    before_replace();
    let unchanged = read_limited(&path, project_spec::MAX_FILE_BYTES).is_ok_and(|now| now == bytes);
    if !unchanged || resolve_project_path(root)? != path {
        return Err(changed());
    }
    replace_atomic(&path, &written)?;
    Ok(wanted)
}

#[cfg(test)]
mod tests {
    use super::super::project_path;
    use super::*;
    use crate::look::tests::V1;

    /// A project folder with a manifest that has something in every part a save must keep,
    /// and one look profile where the manifest says it is.
    fn project() -> (tempfile::TempDir, ProjectConfig) {
        let folder = tempfile::tempdir().unwrap();
        let mut config = ProjectConfig::new("Arc Test");
        config.startup.start_level = "levels/arena.level.json".into();
        config.planned.multiplayer.clients = Some(2);
        config.planned.gameplay.mode = Some("duel".into());
        fs::write(
            project_path(folder.path()),
            project_spec::canonical_bytes(&config).unwrap(),
        )
        .unwrap();
        fs::create_dir_all(folder.path().join(LOOK_FOLDER)).unwrap();
        fs::write(folder.path().join(&config.rendering.profile), V1).unwrap();
        (folder, config)
    }

    /// The names of what is directly in the folder, sorted.
    fn names(folder: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(folder)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    fn manifest(root: &Path) -> Vec<u8> {
        fs::read(project_path(root)).unwrap()
    }

    #[test]
    fn a_save_writes_the_keys_and_the_profile_and_takes_the_rest_from_the_file() {
        let (folder, config) = project();
        let root = folder.path();
        fs::write(root.join("assets/render/dusk.json"), V1).unwrap();
        let before = names(root);

        let mut edit = SettingsEdit::of(&config);
        edit.controls.jump = "Enter".into();
        edit.controls.look.gamepad_right_stick = false;
        edit.rendering_profile = "assets/render/dusk.json".into();
        let saved = save_settings(root, &config, &edit).unwrap();

        assert_eq!(saved.controls.jump, "Enter");
        assert!(!saved.controls.look.gamepad_right_stick);
        assert_eq!(saved.rendering.profile, "assets/render/dusk.json");
        // What a save does not set is as the file had it, the planned settings too.
        assert_eq!(saved.project, config.project);
        assert_eq!(saved.startup, config.startup);
        assert_eq!(saved.planned, config.planned);
        assert_eq!(saved.schema, config.schema);
        // The file is that manifest in its canonical form, read back as the same.
        assert_eq!(
            manifest(root),
            project_spec::canonical_bytes(&saved).unwrap()
        );
        assert_eq!(super::super::load_project(root).unwrap(), saved);
        // Neither the lock nor a half-written file is left beside it.
        assert_eq!(names(root), before);
    }

    #[test]
    fn a_save_sets_the_local_test_clients_and_no_other_planned_setting() {
        let (folder, mut config) = project();
        let root = folder.path();
        // A manifest that names no number, and holds planned settings a save must keep.
        config.planned.multiplayer.clients = None;
        config.planned.multiplayer.bots = Some("idle".into());
        let unnamed = project_spec::canonical_bytes(&config).unwrap();
        fs::write(project_path(root), &unnamed).unwrap();

        // A save of something else leaves it unnamed: none is not written as one.
        let mut edit = SettingsEdit::of(&config);
        assert_eq!(edit.local_clients, None);
        edit.controls.jump = "Enter".into();
        let saved = save_settings(root, &config, &edit).unwrap();
        assert_eq!(saved.planned, config.planned);
        assert!(String::from_utf8(manifest(root))
            .unwrap()
            .contains("\"clients\": null"));

        // A number that is set is written, and read back; the rest of the planned settings
        // is the file's.
        let mut edit = SettingsEdit::of(&saved);
        edit.local_clients = Some(3);
        let three = save_settings(root, &saved, &edit).unwrap();
        assert_eq!(three.planned.multiplayer.clients, Some(3));
        assert_eq!(three.planned.multiplayer.bots.as_deref(), Some("idle"));
        assert_eq!(three.planned.gameplay, config.planned.gameplay);
        assert_eq!(three.controls.jump, "Enter");
        assert_eq!(super::super::load_project(root).unwrap(), three);
        assert_eq!(SettingsEdit::of(&three).local_clients, Some(3));

        // A number the game could not be started with is not written.
        let before = manifest(root);
        for clients in [0, 5] {
            let mut edit = SettingsEdit::of(&three);
            edit.local_clients = Some(clients);
            let refused = save_settings(root, &three, &edit).unwrap_err();
            assert_eq!(refused.code, "project.value", "{clients}");
            assert_eq!(refused.parameters["field"], "planned.multiplayer.clients");
        }
        assert_eq!(manifest(root), before);
    }

    #[test]
    fn a_save_that_changes_nothing_writes_nothing() {
        let (folder, config) = project();
        let root = folder.path();
        // The same manifest, written as another program might: not in the canonical form.
        let loose = serde_json::to_vec(&config).unwrap();
        fs::write(project_path(root), &loose).unwrap();
        let saved = save_settings(root, &config, &SettingsEdit::of(&config)).unwrap();
        assert_eq!(saved, config);
        assert_eq!(manifest(root), loose);
    }

    #[test]
    fn a_manifest_the_game_could_not_play_with_is_not_written() {
        let (folder, config) = project();
        let root = folder.path();
        let before = manifest(root);

        let mut edit = SettingsEdit::of(&config);
        edit.controls.jump = "Shift".into();
        let refused = save_settings(root, &config, &edit).unwrap_err();
        assert_eq!(refused.code, "project.value");
        assert_eq!(refused.parameters["field"], "controls.jump");

        let mut edit = SettingsEdit::of(&config);
        edit.controls.toggle_view = edit.controls.move_left.clone();
        let refused = save_settings(root, &config, &edit).unwrap_err();
        assert_eq!(refused.code, "project.value");
        assert_eq!(refused.parameters["field"], "controls.toggle_view");
        assert_eq!(refused.parameters["shares_with"], "controls.move_left");

        // A profile that is not there, one the look reader refuses, and a path that leads
        // up and out of the folder.
        for (profile, reason) in [
            ("assets/render/none.json", Some("look.missing")),
            ("assets/render/broken.json", Some("look.decode")),
            ("../elsewhere.json", None),
        ] {
            fs::write(root.join("assets/render/broken.json"), b"{").unwrap();
            let mut edit = SettingsEdit::of(&config);
            edit.rendering_profile = profile.into();
            let refused = save_settings(root, &config, &edit).unwrap_err();
            match reason {
                Some(reason) => {
                    assert_eq!(refused.code, "project.profile_unusable", "{profile}");
                    assert_eq!(refused.parameters["reason"], reason);
                    assert_eq!(refused.parameters["profile"], profile);
                }
                None => {
                    assert_eq!(refused.code, "project.value", "{profile}");
                    assert_eq!(refused.parameters["field"], "rendering.profile");
                }
            }
        }
        assert_eq!(manifest(root), before);
        assert!(!root.join("project.aproj.lock").exists());
    }

    #[test]
    fn a_profile_the_manifest_names_already_does_not_stop_a_save_of_a_key() {
        let (folder, config) = project();
        let root = folder.path();
        fs::remove_file(root.join(&config.rendering.profile)).unwrap();
        let mut edit = SettingsEdit::of(&config);
        edit.controls.jump = "Enter".into();
        let saved = save_settings(root, &config, &edit).unwrap();
        assert_eq!(saved.controls.jump, "Enter");
        assert_eq!(saved.rendering, config.rendering);
    }

    #[test]
    fn a_manifest_that_is_not_what_the_editor_read_is_not_written_over() {
        let (folder, config) = project();
        let root = folder.path();
        let mut edit = SettingsEdit::of(&config);
        edit.controls.jump = "Enter".into();

        // Another program changed it after the editor read it.
        let mut theirs = config.clone();
        theirs.project.name = "Renamed".into();
        let their_bytes = project_spec::canonical_bytes(&theirs).unwrap();
        fs::write(project_path(root), &their_bytes).unwrap();
        let refused = save_settings(root, &config, &edit).unwrap_err();
        assert_eq!(refused.code, "project.changed_since_read");
        assert_eq!(manifest(root), their_bytes);

        // Read again, the save goes through and keeps what the other program wrote.
        let saved = save_settings(root, &theirs, &edit).unwrap();
        assert_eq!(saved.project.name, "Renamed");
        assert_eq!(saved.controls.jump, "Enter");

        // Another program writes it between the look and the replacement.
        let mut edit = SettingsEdit::of(&saved);
        edit.controls.jump = "Tab".into();
        let refused = save_with(root, &saved, &edit, || {
            fs::write(project_path(root), &their_bytes).unwrap();
        })
        .unwrap_err();
        assert_eq!(refused.code, "project.changed_since_read");
        assert_eq!(manifest(root), their_bytes);

        // A manifest that has become one of another version says so.
        let mut value = serde_json::to_value(&theirs).unwrap();
        value["schema"] = (project_spec::SCHEMA + 1).into();
        fs::write(project_path(root), serde_json::to_vec(&value).unwrap()).unwrap();
        let refused = save_settings(root, &theirs, &edit).unwrap_err();
        assert_eq!(refused.code, "project.schema_unsupported");
    }

    #[test]
    fn a_manifest_another_editor_holds_is_not_written() {
        let (folder, config) = project();
        let root = folder.path();
        let before = manifest(root);
        let mut edit = SettingsEdit::of(&config);
        edit.controls.jump = "Enter".into();
        {
            let _theirs = hold(&project_path(root)).unwrap();
            let refused = save_settings(root, &config, &edit).unwrap_err();
            assert_eq!(refused.code, "project.locked");
            assert_eq!(manifest(root), before);
        }
        // Their lock is gone with them, and was not taken away by the refused save.
        assert!(save_settings(root, &config, &edit).is_ok());
    }

    #[test]
    fn the_profiles_to_choose_from_are_the_json_files_of_the_look_folder() {
        let (folder, config) = project();
        let root = folder.path();
        fs::write(root.join("assets/render/dusk.json"), V1).unwrap();
        fs::write(root.join("assets/render/broken.json"), b"{").unwrap();
        fs::write(root.join("assets/render/README.md"), b"notes").unwrap();
        fs::create_dir_all(root.join("assets/render/more")).unwrap();
        fs::write(root.join("assets/render/more/deep.json"), V1).unwrap();

        let choices = look_profiles(root);
        let paths: Vec<&str> = choices.iter().map(|choice| choice.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "assets/render/broken.json",
                "assets/render/dusk.json",
                "assets/render/look-v1.json"
            ]
        );
        assert_eq!(choices[0].read.as_ref().unwrap_err().code, "look.decode");
        // The same bytes are the same profile.
        assert!(choices[1].read.is_ok());
        assert_eq!(choices[1].read, choices[2].read);
        assert_eq!(choices[2].path, config.rendering.profile);
        // A folder without one has none, and nothing is made by looking.
        let empty = tempfile::tempdir().unwrap();
        assert!(look_profiles(empty.path()).is_empty());
        assert_eq!(fs::read_dir(empty.path()).unwrap().count(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn a_profile_that_is_a_link_out_of_the_folder_is_not_chosen_or_saved() {
        use std::os::unix::fs::symlink;
        let (folder, config) = project();
        let root = folder.path();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("look.json"), V1).unwrap();
        symlink(
            outside.path().join("look.json"),
            root.join("assets/render/linked.json"),
        )
        .unwrap();

        let choices = look_profiles(root);
        let linked = choices
            .iter()
            .find(|choice| choice.path == "assets/render/linked.json")
            .unwrap();
        assert_eq!(
            linked.read.as_ref().unwrap_err().code,
            "project.outside_folder"
        );
        let before = manifest(root);
        let mut edit = SettingsEdit::of(&config);
        edit.rendering_profile = "assets/render/linked.json".into();
        let refused = save_settings(root, &config, &edit).unwrap_err();
        assert_eq!(refused.code, "project.outside_folder");
        assert_eq!(manifest(root), before);
    }

    /// The SHA-256 of the game's first look profile, which is the default one.
    const DEFAULT_LOOK_SHA256: &str =
        "3abf9a2fbcb48b748607934137653f334e85ab9a226bfb3383e92c78387715e6";

    #[test]
    fn the_default_profile_is_the_games_first_one_and_the_look_reader_takes_it() {
        let read = parse_look_profile(DEFAULT_LOOK_PROFILE, Path::new("default")).unwrap();
        assert_eq!(read.sha256_hex, DEFAULT_LOOK_SHA256);
    }

    #[test]
    fn a_profile_that_is_not_there_is_made_from_the_default_and_nothing_else_is_written() {
        let (folder, config) = project();
        let root = folder.path();
        let named = &config.rendering.profile;
        // Not even the folder the profiles are in.
        fs::remove_dir_all(root.join("assets")).unwrap();
        assert_eq!(look_profile(root, named).unwrap_err().code, "look.missing");
        let before = manifest(root);

        let made = install_default_look(root, named).unwrap();
        assert_eq!(made.sha256_hex, DEFAULT_LOOK_SHA256);
        assert_eq!(fs::read(root.join(named)).unwrap(), DEFAULT_LOOK_PROFILE);
        assert_eq!(look_profile(root, named).unwrap(), made);
        assert_eq!(manifest(root), before);
        assert_eq!(names(root), ["assets", "project.aproj"]);
        assert_eq!(names(&root.join(LOOK_FOLDER)), ["look-v1.json"]);

        // Another name the manifest could have for it, in a folder of its own.
        install_default_look(root, "looks/dusk.json").unwrap();
        assert_eq!(
            fs::read(root.join("looks/dusk.json")).unwrap(),
            DEFAULT_LOOK_PROFILE
        );
    }

    #[test]
    fn a_profile_that_is_there_is_not_written_over_whatever_it_is() {
        let (folder, config) = project();
        let root = folder.path();
        let named = &config.rendering.profile;
        // One the look reader refuses is still the person's file.
        fs::write(root.join(named), b"not a profile").unwrap();
        let refused = install_default_look(root, named).unwrap_err();
        assert_eq!(refused.code, "project.conflict");
        assert_eq!(fs::read(root.join(named)).unwrap(), b"not a profile");

        #[cfg(unix)]
        {
            // A link that leads nowhere has the name too.
            let dangling = root.join(LOOK_FOLDER).join("gone.json");
            std::os::unix::fs::symlink(root.join("nowhere.json"), &dangling).unwrap();
            let refused = install_default_look(root, "assets/render/gone.json").unwrap_err();
            assert_eq!(refused.code, "project.conflict");
            assert!(!root.join("nowhere.json").exists());
        }
    }

    #[test]
    fn a_profile_is_made_in_the_projects_folder_or_not_at_all() {
        let (folder, _) = project();
        let root = folder.path();
        let outside = tempfile::tempdir().unwrap();
        for path in [
            "",
            " ",
            "../look.json",
            "assets/../../look.json",
            "a\\b.json",
        ] {
            let refused = install_default_look(root, path).unwrap_err();
            assert_eq!(refused.code, "project.path_invalid", "{path:?}");
        }
        let absolute = outside.path().join("look.json");
        let refused = install_default_look(root, absolute.to_str().unwrap()).unwrap_err();
        assert_eq!(refused.code, "project.path_invalid");
        assert!(names(outside.path()).is_empty());

        #[cfg(unix)]
        {
            // The folder of the profiles is a link to somewhere else.
            fs::remove_dir_all(root.join(LOOK_FOLDER)).unwrap();
            std::os::unix::fs::symlink(outside.path(), root.join(LOOK_FOLDER)).unwrap();
            let refused = install_default_look(root, "assets/render/look-v1.json").unwrap_err();
            assert_eq!(refused.code, "project.outside_folder");
            assert!(names(outside.path()).is_empty());
        }

        // A folder that is not there is not made for it.
        let gone = outside.path().join("gone");
        let refused = install_default_look(&gone, "assets/render/look-v1.json").unwrap_err();
        assert_eq!(refused.code, "project.folder_missing");
        assert!(!gone.exists());
    }
}
