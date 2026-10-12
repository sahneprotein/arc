//! The words the editor shows.
//!
//! The editor's core reports states, problems and fields as codes with parameters and never
//! as sentences. Every sentence a person reads is written here, in one place, so that the
//! wording follows the design system's rules: name the thing, say what happened, say how to
//! fix it. Nothing here depends on the toolkit or on the core's types.

/// A problem as a person reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// What happened, as a short sentence without a full stop.
    pub title: String,
    /// What to do about it. Empty when there is nothing to do but try again.
    pub remedy: String,
}

/// Turns a problem code of the core into words. `parameter` returns the value of a named
/// parameter of the diagnostic, if it has one.
///
/// A code this table does not know is still shown, with the code itself, so that nothing
/// fails silently and the code can be looked up.
pub fn problem<'a>(code: &str, parameter: impl Fn(&str) -> Option<&'a str>) -> Problem {
    // What stops a level's placed emitters is worded once, where the review words it: a
    // cook or a start that fails on one says the same.
    if matches!(
        code,
        "emitter.no_game"
            | "emitter.asset_missing"
            | "emitter.asset_refused"
            | "emitter.runtime_unsupported"
            | "emitter.changed_since_cook"
    ) {
        return review(code, parameter);
    }
    let detail = parameter("detail").unwrap_or_default();
    let (title, remedy): (&str, &str) = match code {
        // Saving and opening.
        "save.conflict" => (
            "The file changed on disk since it was opened",
            "Save under another name, or open the file again and redo your changes.",
        ),
        "save.locked" => (
            "The file is in use by another program",
            "Close the other program and save again.",
        ),
        "save.backup_exists" => (
            "A copy of this level as it was is in the way",
            "The level is now of a newer format. Before it is saved in it, the file as it \
             was is kept beside it, under its name with .schema-N.bak after it. A file of \
             that name is there already, and it is not written over. Nothing was saved. Move \
             that file away and save again.",
        ),
        "save.backup_failed" => (
            "The level as it was could not be kept before saving",
            "The level is now of a newer format, and it is saved in it only once the file \
             as it was is kept beside it. That copy could not be written. Nothing was saved. \
             Check that you may write to that folder and that the disk is not full.",
        ),
        "save.parent_create" => (
            "The folder for the level could not be made",
            "Check that you may write there and that no file has the folder's name.",
        ),
        "io.failed" => (
            "The file could not be read or written",
            "Check that you may write to that folder and that the disk is not full.",
        ),
        "path.invalid" => (
            "That is not a place a level can be saved",
            "Choose a file inside the project folder.",
        ),
        // Several objects at once, and the hierarchy.
        "snap.range" => (
            "That step is outside what snapping takes",
            "Give a grid step from 0.01 to 100 metres and a turn from 1 to 90 degrees.",
        ),
        "duplicate.terrain_unsupported" => (
            "A landscape cannot be duplicated",
            "The selection includes one. Change the selection and duplicate again.",
        ),
        "duplicate.order_limit" => (
            "The level has no number left for a copy",
            "The last place under that parent, or the last spawn point number, is taken. \
             Remove an object there and duplicate again.",
        ),
        "duplicate.incomplete"
        | "duplicate.sources"
        | "duplicate.append_order"
        | "duplicate.spawn_order"
        | "duplicate.child_changed" => (
            "The selection changed while it was being duplicated",
            "Nothing was copied. Duplicate again.",
        ),
        "selection.empty" => ("Nothing is selected", "Select an object first."),
        "selection.active_not_selected" | "inspector.revision_required" => (
            "The selection changed while the change was on its way",
            "Nothing was changed. Do it again.",
        ),
        "field.kind" => (
            "Not every selected object has that value",
            "A size belongs to boxes only. Select boxes alone to give them one size.",
        ),
        "hierarchy.order_limit" | "hierarchy.duplicate_order" => (
            "That object has no more room for children",
            "Put the object under another one.",
        ),
        // The game's look profile, which Game look in the 3D view is drawn from.
        "look.missing" => (
            "The look profile is not there",
            "A project's look profile can be made in the Project settings, with \"Make the \
             default look profile\". An editor that was started with --game looks in that \
             folder: it is not the game's, or the game is older than this editor.",
        ),
        "look.read_failed" => (
            "The game's look profile could not be read",
            "Check that the file can be read.",
        ),
        "look.limit" => (
            "The game's look profile is larger than a profile can be",
            "It may be another file under the profile's name.",
        ),
        "look.decode" => (
            "The game's look profile is not one this editor can read",
            "It lacks something, or names something this editor does not know. Bring the \
             editor and the game to the same version.",
        ),
        "look.schema" => (
            "The game's look profile is of another version than this editor reads",
            "Bring the editor and the game to the same version.",
        ),
        "look.value" => (
            "The game's look profile holds a value that cannot be drawn",
            "Correct it in the game's profile.",
        ),
        // Showing a file, the game's log for instance, in the desktop's program for it.
        "path.missing" => (
            "That file is no longer there",
            "It may have been removed since the game ended. Play again for a new log.",
        ),
        "path.launch_failed" => (
            "The file could not be opened from here",
            "Open it yourself: its path is in the notice of the run.",
        ),
        "path.platform_unsupported" => (
            "This desktop gives the editor no way to open a file",
            "Open it yourself: its path is in the notice of the run.",
        ),
        "source.decode" => (
            "This file is not a level the editor can read",
            "Open a level file saved by this editor.",
        ),
        "schema.unsupported" => (
            "This level was saved by a different version of the editor",
            "Open it with the version that saved it. The file was not changed.",
        ),
        "source.limit" | "scene.limit" | "payload.limit" => (
            "The level is larger than the editor allows",
            "Remove objects, or split the level.",
        ),
        // Editing.
        "name.invalid" => (
            "That name cannot be used",
            "Use letters, digits, spaces, '-' and '_', and do not leave it empty.",
        ),
        "box.extents" => (
            "A box needs a size above zero on every axis",
            "Enter a size greater than 0.",
        ),
        "transform.nonfinite" => (
            "That position or rotation is not a number",
            "Enter ordinary numbers.",
        ),
        "transform.rotation_not_unit" => {
            ("That rotation is not valid", "Enter the rotation again.")
        }
        "spawn.duplicate_order" => (
            "Two spawn points have the same number",
            "Give each spawn point its own number.",
        ),
        "hierarchy.cycle" => (
            "An object cannot be placed inside itself",
            "Choose another parent.",
        ),
        "hierarchy.depth_limit" => (
            "Objects are nested too deeply",
            "Move the object closer to the top of the hierarchy.",
        ),
        "object.missing" | "selection.missing" | "reference.missing_parent" => (
            "The object no longer exists",
            "It was removed by another change. Select something else.",
        ),
        "history.empty" => ("There is nothing to undo or redo", ""),
        "history.capacity" => (
            "This change is too large to be undone",
            "Make the change in smaller steps.",
        ),
        // The editor was busy or the view was out of date: nothing was changed.
        "revision.stale" | "inspector.stale" | "selection.stale" | "session.stale"
        | "preview.stale" | "transaction.stale" | "job.stale" | "request.reused" => (
            "The level changed before that could be applied",
            "Nothing was changed. Try again.",
        ),
        "transaction.busy" => (
            "Finish or cancel the drag first",
            "Release the mouse, or press Escape to cancel.",
        ),
        "queue.full" => (
            "The editor is still working on earlier changes",
            "Wait a moment and try again.",
        ),
        "job.cancelled" => ("Cancelled", ""),
        // Cooking and cooked levels.
        "cook.geometry_nonfinite" | "cook.normal_nonfinite" | "cook.degenerate_triangle" => (
            "The level has geometry that cannot be cooked",
            "Check the size and position of the object named below.",
        ),
        "payload.hash_mismatch"
        | "payload.decode"
        | "payload.schema_or_profile"
        | "package.decode"
        | "package.provenance" => (
            "The cooked level is damaged or was made by another version",
            "Cook the level again. If that finds the same, delete the cooked level's folder first.",
        ),
        "package.identity_mismatch" | "package.directory_identity" => (
            "The cooked level on disk is not the one this editor cooked",
            "Cook the level again. If that finds the same, delete the cooked level's folder first.",
        ),
        "package.manifest_limit" => (
            "The cooked level's description is larger than a cooked level's can be",
            "Cook the level again. If that finds the same, delete the cooked level's folder first.",
        ),
        "terrain.invalid"
        | "terrain.sample_missing"
        | "terrain.sample_invalid_or_duplicate"
        | "terrain.tile_duplicate"
        | "layer.missing"
        | "layer.duplicate"
        | "layer.index"
        | "layer.index_or_value"
        | "layer.missing_payload"
        | "layer.default"
        | "layer.defaults_not_normalized"
        | "layer.weights_not_normalized"
        | "layer.surface_group_missing"
        | "layer.surface_group_required"
        | "layer.group_resolution"
        | "layer.color"
        | "layer.invalid_payload" => (
            "The landscape data of this level is not valid",
            "Undo the last landscape change. If the file was edited by hand, restore it.",
        ),
        "layer.limit" | "scene.resource_limit" => (
            "The landscape cannot hold that many layers",
            "Remove a layer you do not need, or use fewer tiles.",
        ),
        "layer.derived_output" => (
            "A rule fills this layer, so it cannot be painted",
            "Paint another layer, or a mask the rule leaves out.",
        ),
        "layer.single_weight" => (
            "This is the ground's only layer",
            "It covers everything and cannot be thinned. Add a second layer and paint that one.",
        ),
        "rule.layer_binding" => (
            "A landscape rule uses this layer",
            "Change or remove the rule first.",
        ),
        "rule.output_layer" => (
            "A rule needs a layer of its own to fill",
            "Give the rule a layer that no other rule fills.",
        ),
        "rule.parameter_range" => ("From must be below To", "Change one of the two."),
        "rule.parameter" => (
            "A setting of the rule is outside what is allowed",
            "Enter a value inside the setting's range.",
        ),
        "rule.limit" => (
            "A landscape cannot have more than 64 rules",
            "Remove a rule you do not need.",
        ),
        "rule.candidate_limit" | "rule.output_limit" | "payload.derived_layer_limit" => (
            "The rules make more than the level can hold",
            "Lower a grass rule's density or its tries per cell, or remove a rule.",
        ),
        "rule.cycle" => (
            "Two rules depend on each other",
            "Change which layers one of them reads.",
        ),
        "rule.unsupported" => (
            "This rule comes from another version of the editor",
            "Remove it, or open the level with that version.",
        ),
        "rule.dependency_missing"
        | "rule.id"
        | "rule.duplicate_binding"
        | "rule.duplicate_dependency" => (
            "The landscape's rules are not valid",
            "Undo the last change to the rules.",
        ),
        "rule.not_implemented" => ("This landscape rule is not available yet", ""),
        "worker.start" | "worker.stopped" => (
            "The editor's background work has stopped",
            "Save your level under a new name and restart the editor.",
        ),
        _ => {
            return Problem {
                title: format!("Something went wrong ({code})"),
                remedy: if detail.is_empty() {
                    "Nothing was changed. Try again.".into()
                } else {
                    format!("Nothing was changed. Details: {detail}")
                },
            };
        }
    };
    Problem {
        title: title.into(),
        remedy: remedy.into(),
    }
}

/// A problem the core's review of the level lists, in words. What stops the game from running
/// the level is said as what to change in the level; anything else is a problem as
/// [`problem`] words it.
pub fn review<'a>(code: &str, parameter: impl Fn(&str) -> Option<&'a str>) -> Problem {
    let (title, remedy): (&str, String) = match code {
        "play.spawn_required" => (
            "The level has no spawn point",
            "The game needs a place to put the player. Add a spawn point from the Add menu.".into(),
        ),
        "play.terrain_unsupported" => {
            let has = match (parameter("reason"), parameter("count")) {
                (Some("rules") | None, Some("1")) => "It has 1 rule. ".to_owned(),
                (Some("rules") | None, Some(count)) => format!("It has {count} rules. "),
                _ => String::new(),
            };
            (
                "The game cannot run what this landscape's rules make",
                format!(
                    "{has}Grass and layers a rule fills are not run. Remove the landscape's \
                     rules to play the level; sculpted ground and painted layers are run."
                ),
            )
        }
        "play.runtime_limit" => (
            "The level has more boxes or spawn points than the game takes",
            "Remove objects, or split the level.".into(),
        ),
        // A placed emitter the game could not show. None of these is passed over: the
        // level is not cooked or played until the object or its emitter is put right.
        "emitter.no_game" => (
            "This emitter cannot be read: the editor has no game folder",
            "A placed emitter is a file of the game. Start the editor with --game and the \
             game's folder, or remove the object."
                .into(),
        ),
        "emitter.asset_missing" => (
            "The emitter this object places is not in the game's folder",
            "Its file was removed or never saved. Remove the object, or put the emitter \
             back and save it in the Particles tool."
                .into(),
        ),
        "emitter.asset_refused" => (
            "The emitter this object places cannot be read",
            "Its file is not an emitter this editor reads. The Particles tool lists the file \
             with the reason."
                .into(),
        ),
        "emitter.runtime_unsupported" => (
            "The game cannot draw the emitter this object places",
            match parameter("field") {
                Some(field) => {
                    let label = emitter_field(field);
                    let setting = label.split(" (").next().unwrap_or(label).to_lowercase();
                    format!(
                        "The game's particle runtime does not draw an emitter with this \
                         {setting} yet. Change it in the Particles tool and save, or remove \
                         the object."
                    )
                }
                None => "Change the emitter in the Particles tool and save, or remove the \
                         object."
                    .into(),
            },
        ),
        "emitter.changed_since_cook" => (
            "An emitter was changed after the level was cooked",
            "The cooked level names the emitter's file as it was. Cook the level again.".into(),
        ),
        other => return problem(other, parameter),
    };
    Problem {
        title: title.into(),
        remedy,
    }
}

/// How a run of the game ended, when it is an end the person did not ask for: a notice that
/// stays. `cause` is the code of what lay behind it, if the core gave one. `None` for an end
/// the person asked for, which is said in the status bar ([`play_end_asked`]).
pub fn play_end(code: &str, cause: Option<&str>) -> Option<Problem> {
    let (title, remedy): (&str, String) = match code {
        "play.crashed" => (
            "The game stopped unexpectedly",
            match cause {
                Some("play.unexpected_exit") => "It ended by itself while it was running.".into(),
                Some("play.exit_during_stop") => {
                    "It ended with an error while it was being stopped.".into()
                }
                Some("play.wait_failed") => "The editor lost track of the game's process.".into(),
                Some(other) => format!("Reason: {other}."),
                None => String::new(),
            },
        ),
        "play.start_failed" => (
            "The game did not start",
            match cause {
                Some("play.runtime_adapter_unavailable") => {
                    "This editor cannot launch the game yet: the part that starts the game's \
                     processes is not built. The cooked level itself is not at fault."
                        .into()
                }
                Some("play.readiness_timeout") => "It did not become ready in time.".into(),
                Some("play.terrain_unsupported") => {
                    "The game cannot run what the landscape's rules make: grass, and layers \
                     a rule fills. Remove the rules, cook the level again and play. Sculpted \
                     ground and painted layers are run."
                        .into()
                }
                Some("play.spawn_required") => {
                    "The level has no spawn point. Add one, cook the level again and play.".into()
                }
                Some("play.runtime_limit") => {
                    "The level has more boxes or spawn points than the game takes.".into()
                }
                Some("play.preflight_busy") => {
                    "The editor was still checking the last start. Try again.".into()
                }
                Some(
                    "play.invalid_listening"
                    | "play.invalid_close"
                    | "play.readiness_order"
                    | "play.server_not_ready",
                ) => "It reported its start in a way the editor does not accept.".into(),
                Some(other) => format!("Reason: {other}."),
                None => String::new(),
            },
        ),
        _ => return None,
    };
    Some(Problem {
        title: title.into(),
        remedy,
    })
}

/// Whether a start failed for something in the level itself, which the same cooked level
/// would fail for again: what Review lists, not what "Play again" mends.
pub fn play_end_is_of_the_level(cause: Option<&str>) -> bool {
    matches!(
        cause,
        Some("play.terrain_unsupported" | "play.spawn_required" | "play.runtime_limit")
    )
}

/// The notice that the level has moved on from the cooked level a run plays. `package` is
/// that cooked level's short name.
pub fn play_moved_on(package: &str) -> Problem {
    Problem {
        title: "The level has changed since the game started".into(),
        remedy: format!("The game runs cooked {package}, without those changes."),
    }
}

/// How a run ended, in the status bar's words, when the person asked for that end.
pub fn play_end_asked(code: &str) -> Option<&'static str> {
    match code {
        "play.stopped" => Some("Stopped"),
        "play.window_closed" => Some("The game window was closed"),
        _ => None,
    }
}

/// What a run left behind when it could not be cleaned up, from the core's cleanup code.
pub fn play_left_behind(code: &str) -> Option<Problem> {
    let title = match code {
        "play.server_cleanup_failed" => "The game's server did not shut down",
        "play.client_cleanup_failed" => "The game did not shut down",
        _ => return None,
    };
    Some(Problem {
        title: title.into(),
        remedy: "It may still be running.".into(),
    })
}

/// A refusal of the Play coordinator as one sentence. Nothing happened, so it is not a notice.
pub fn play_refusal(code: &str) -> String {
    match code {
        "play.already_active" => "A game is already running. Stop it first.".into(),
        "play.package_required" => "Cook the level before playing.".into(),
        "play.no_game" => {
            "This editor has no game to play with. Open a project and choose the game's \
             folder."
                .into()
        }
        "play.blocked" => {
            "The game cannot run this level as it is: Review says why. Nothing was cooked or \
             started."
                .into()
        }
        "play.mode_unsupported" => "The game cannot be played inside the editor yet.".into(),
        "play.queue_full" => "The editor is still dealing with the last request. Try again.".into(),
        "play.coordinator_closed" | "play.supervisor_start_failed" => {
            "The part of the editor that starts the game has stopped. Restart the editor.".into()
        }
        other => format!("The game could not be started ({other})."),
    }
}

/// How a process ended, as a fact for a notice: its exit code, or the signal that ended it.
pub fn exit_facts(code: Option<i32>, signal: Option<i32>) -> Option<String> {
    match (code, signal) {
        (_, Some(signal)) => Some(format!("ended by signal {signal}")),
        (Some(code), None) => Some(format!("exit code {code}")),
        (None, None) => None,
    }
}

/// The label of an inspector field, from the core's label key.
pub fn field_label(label_key: &str) -> &str {
    match label_key {
        "field.name" => "Name",
        "field.translation" => "Position",
        "field.rotation" => "Rotation",
        "field.half_extents" => "Half size",
        "field.emitter" => "Emitter",
        // An unknown field is shown under its own name instead of being hidden.
        other => other.strip_prefix("field.").unwrap_or(other),
    }
}

/// The short unit shown inside a number field.
pub fn unit(unit_key: &str) -> &str {
    match unit_key {
        "metres" => "m",
        "degrees" => "\u{b0}",
        other => other,
    }
}

/// What an undo or redo step is called in the menu and the status bar.
pub fn change_label(label_key: &str) -> &str {
    match label_key {
        "drag" => "Move",
        "edit" => "Edit",
        "rename" => "Rename",
        "create" => "Add",
        "delete" => "Remove",
        "layers" => "Layers",
        "reparent" => "Move in hierarchy",
        "duplicate" => "Duplicate",
        "rule" => "Rules",
        "paint" => "Paint",
        "sculpt" => "Sculpt",
        "branch" => "Change",
        other => other,
    }
}

/// The saved state shown in the status bar. The first is the text, the second says whether it
/// needs attention (shown in the warning colour, always together with the text).
pub fn saved_state(dirty: bool, cook_needed: bool) -> (&'static str, bool) {
    match (dirty, cook_needed) {
        (true, _) => ("Unsaved", true),
        (false, true) => ("Saved, cook needed", true),
        (false, false) => ("Saved", false),
    }
}

/// A content hash as the status bar shows it: its first eight hexadecimal digits.
pub fn short_hash(hex: &str) -> &str {
    hex.get(..8).unwrap_or(hex)
}

/// A refusal about the project's file, in words: a setup or a conversion that could not be
/// done, or a file that cannot be used. `parameter` returns the value of a named parameter
/// of the diagnostic.
pub fn project<'a>(code: &str, parameter: impl Fn(&str) -> Option<&'a str>) -> Problem {
    let detail = parameter("detail").filter(|detail| !detail.is_empty());
    let with_detail = |words: &str| match detail {
        Some(detail) => format!("{words} What the reader said: {detail}."),
        None => words.to_owned(),
    };
    let (title, remedy): (&str, String) = match code {
        "project.convert_required" => (
            "The project file is of an older version",
            "Nothing was changed. Convert it in the Project settings.".into(),
        ),
        "project.schema_unsupported" => (
            "The project file is of a version this editor does not read",
            match parameter("schema") {
                Some(schema) => format!(
                    "It is of version {schema}, which this editor has no way to convert: it \
                     was written by a newer editor. Nothing was changed. Open the project \
                     with that editor."
                ),
                None => "It was written by another editor, and this one has no way to \
                         convert it. Nothing was changed."
                    .into(),
            },
        ),
        "project.locked" => (
            "Another editor is writing the project file",
            "Nothing was changed. Try again in a moment. If no other editor is open, one \
             that stopped unexpectedly left arc-project.json.lock in the folder: remove \
             that file."
                .into(),
        ),
        "project.changed" => (
            "The project file was changed while it was being converted",
            "Another program wrote it. Nothing was replaced, and no backup was kept of the \
             older content. Look at the file and convert again."
                .into(),
        ),
        "project.outside_folder" => (
            "A file of the project is a link to somewhere outside its folder",
            match parameter("path") {
                Some(path) => format!(
                    "The editor does not read or write through {path}. Put the file itself \
                     into the project's folder."
                ),
                None => "The editor does not read or write through it. Put the file itself \
                         into the project's folder."
                    .into(),
            },
        ),
        "project.levels_rejected" => (
            "The folder's level files cannot be read",
            "Nothing was made. The Project settings list each file with its reason; a new \
             starter level is made only when you choose it there."
                .into(),
        ),
        "project.conflict" => (
            "A file that would have been written is there already",
            match parameter("path") {
                Some(path) => format!(
                    "Nothing was changed. The editor does not write over {path}. Move that \
                     file away and try again."
                ),
                None => "Nothing was changed. Move that file away and try again.".into(),
            },
        ),
        "project.changed_since_read" => (
            "The project file was changed by another program",
            "Nothing was saved, and the file is as that program left it. Read it again \
             here; what was changed here is kept and can be saved then."
                .into(),
        ),
        "project.profile_unusable" => ("The look profile cannot be used", {
            let reason = parameter("reason")
                .map(|code| problem(code, |_| None).title)
                .unwrap_or_default();
            match parameter("profile") {
                Some(profile) => {
                    format!("{profile}: {reason}. Nothing was saved. Choose another profile.")
                }
                None => "Nothing was saved. Choose another profile.".into(),
            }
        }),
        "project.value" => {
            let action = |key: &str| {
                parameter(key)
                    .and_then(crate::project::KeyAction::of_field)
                    .map(crate::project::KeyAction::words)
            };
            match (action("field"), action("shares_with"), parameter("field")) {
                (Some(second), Some(first), _) => (
                    "Two actions are on one key",
                    format!(
                        "{first} and {second} are on the same key, and the game could not \
                         do both. Nothing was changed. Give one of them another key."
                    ),
                ),
                (Some(action), None, _) => (
                    "A key of the project is none the game reads",
                    format!(
                        "{action} is on a key the game does not know. Nothing was changed. \
                         Choose its key in the Project settings."
                    ),
                ),
                (None, _, Some("rendering.profile")) => (
                    "The look profile's path cannot be used",
                    with_detail("It must be a path in the project's folder. Nothing was changed."),
                ),
                _ => (
                    "The project cannot have that value",
                    with_detail(
                        "A project needs a name of 1 to 96 characters. Nothing was changed.",
                    ),
                ),
            }
        }
        "project.decode" | "project.schema_missing" => (
            "The project file is not one this editor can read",
            with_detail("It is not changed. Put right what is wrong with it, or move it away."),
        ),
        "project.limit" => (
            "The project file is larger than a project file can be",
            "It may be another file under the project file's name.".into(),
        ),
        "project.read" => (
            "The project file could not be read",
            with_detail("Check that the folder can be read."),
        ),
        "project.write" | "project.create_folder" => (
            "The project could not be written",
            with_detail("Check that the folder can be written to."),
        ),
        "project.start_level_missing" => (
            "The level the project starts with is not in the folder",
            "Save a level under that name, or put the file back.".into(),
        ),
        "project.folder_taken" => (
            "A folder or a file of the project's name is there already",
            "Nothing was made, and what is there was not changed. Give the project another \
             name, or choose another folder to make it in."
                .into(),
        ),
        "project.folder_name" => (
            "That name cannot be the name of the project's folder",
            "Nothing was made. The project's folder is named after the project: give it a \
             name with letters or digits in it, and none that the system keeps for itself."
                .into(),
        ),
        "project.parent_missing" => (
            "The folder to make the project in is not there",
            "Nothing was made. It was moved, renamed or removed, or it is no folder. Choose \
             another."
                .into(),
        ),
        "project.path_invalid" => (
            "The project file names a path that is not in the project's folder",
            "Nothing was made. Choose a look profile of the folder in the Project settings.".into(),
        ),
        "project.folder_missing" => (
            "The project's folder is not there",
            "Nothing was made. It was moved, renamed or removed, or it is on a disk that is \
             not connected."
                .into(),
        ),
        other => return problem(other, parameter),
    };
    Problem {
        title: title.into(),
        remedy,
    }
}

/// A refusal about the game's folder, in words: a folder that was chosen for the game and
/// is none, or the editor's setting for it that cannot be read or kept. Never the code: a
/// refusal this does not know is said as one about the folder. `parameter` returns the
/// value of a named parameter of the diagnostic.
pub fn game_folder<'a>(code: &str, parameter: impl Fn(&str) -> Option<&'a str>) -> Problem {
    let file = || match parameter("path") {
        Some(path) => format!(" The file is {path}."),
        None => String::new(),
    };
    let (title, remedy): (&str, String) = match code {
        "runtime.folder_missing" => (
            "The game folder is not there",
            "It was moved, renamed or removed, or it is on a disk that is not connected. \
             Choose the folder the game is built in."
                .into(),
        ),
        "runtime.not_a_folder" => (
            "That is not a folder",
            "Choose the folder the game is built in.".into(),
        ),
        "runtime.folder_read" | "runtime.file_read" => (
            "The game folder could not be read",
            "Check that the folder and what is in it can be read.".into(),
        ),
        "runtime.outside_folder" => (
            "Part of the game in that folder is a link that leads out of it",
            "The editor does not follow it. Choose a folder that holds the game itself.".into(),
        ),
        "runtime.requirements_missing" => (
            "That folder does not hold the whole game",
            "A game folder has the game's server and its client as they are built \
             (target/debug/server and target/debug/client) and its assets folder. Build \
             the game there, or choose the folder it is built in."
                .into(),
        ),
        "runtime.preference_read"
        | "runtime.preference_decode"
        | "runtime.preference_limit"
        | "runtime.preference_path" => (
            "The editor's setting for the game folder cannot be read",
            format!(
                "It is left as it is, and no game folder is used until it is put right or \
                 moved away.{}",
                file()
            ),
        ),
        "runtime.preference_schema" => (
            "The editor's setting for the game folder was written by a newer editor",
            format!(
                "It is left as it is, and this editor uses no game folder.{}",
                file()
            ),
        ),
        "runtime.preference_locked" => (
            "Another editor is changing the setting for the game folder",
            "Nothing was changed. Choose the folder again when it is done.".into(),
        ),
        "runtime.preference_write" | "runtime.preference_encode" => (
            "The game folder could not be kept as the editor's setting",
            "Nothing was changed. Check that the editor's own settings folder can be \
             written to."
                .into(),
        ),
        "runtime.preference_unavailable" => (
            "This system has no place to keep the game folder in",
            "None can be set. A project whose folder is the game's own can still be played.".into(),
        ),
        _ => (
            "The game folder cannot be used",
            "Choose the folder the game is built in.".into(),
        ),
    };
    Problem {
        title: title.into(),
        remedy,
    }
}

/// Why a level file of a folder cannot be what a project starts with, in words: the reader's
/// code and what it said.
pub fn level_rejection(code: &str, detail: &str) -> String {
    let with = |words: &str| {
        if detail.is_empty() {
            words.to_owned()
        } else {
            format!("{words} ({detail})")
        }
    };
    match code {
        "source.decode" => with("it is not a level this editor can read"),
        "schema.unsupported" => "it was saved by a different version of the editor".to_owned(),
        "project.outside_folder" => {
            "it is a link to a file outside the project's folder".to_owned()
        }
        "project.limit" => "it is larger than a level can be".to_owned(),
        "project.read" => with("it could not be read"),
        "project.not_a_file" => "it is not a file".to_owned(),
        other => format!("it is not a valid level ({other})"),
    }
}

/// What a number of an emitter is called in its panel, by the dotted name the shared
/// particle format uses for it. A name this table does not know is shown as it is.
pub fn emitter_field(field: &str) -> &str {
    match field {
        "name" => "Name",
        "emission.rate_per_second" => "Rate (per s)",
        "emission.burst_count" => "Burst",
        "emission.duration_seconds" => "Duration (s)",
        "lifetime.min_seconds" => "Shortest life (s)",
        "lifetime.max_seconds" => "Longest life (s)",
        "shape.radius_m" => "Radius (m)",
        "shape.angle_degrees" => "Angle (\u{b0})",
        "shape.size_m" => "Size (m)",
        "motion.speed_min_m_per_s" => "Slowest (m/s)",
        "motion.speed_max_m_per_s" => "Fastest (m/s)",
        "motion.gravity_m_per_s2" => "Gravity (m/s\u{b2})",
        "motion.drag_per_s" => "Drag (per s)",
        "look.size_start_m" => "Size at start (m)",
        "look.size_end_m" => "Size at end (m)",
        "look.color_start_linear_rgba" => "Colour at start",
        "look.color_end_linear_rgba" => "Colour at end",
        "limits.max_particles" => "Most particles",
        other => other,
    }
}

/// A refusal about an emitter, in words: a value the shared particle format does not take,
/// a file that cannot be read, or one that changed on disk. `parameter` returns the value of
/// a named parameter of the diagnostic; `range` the least and the most the field at fault
/// may be, where the format bounds it.
pub fn emitter<'a>(
    code: &str,
    parameter: impl Fn(&str) -> Option<&'a str>,
    range: Option<(f64, f64)>,
) -> Problem {
    // A field of the format by the name its panel row has, without the unit.
    let field = parameter("field").map(|field| {
        let label = emitter_field(field);
        label.split(" (").next().unwrap_or(label).to_lowercase()
    });
    let (title, remedy): (String, String) = match code {
        // Of a file: its name and the ID written in it are not one valid ID.
        "particle.value" if parameter("field") == Some("id") => (
            "The file's name is not the ID of the emitter in it".into(),
            "An emitter's file is named after its ID. The file was renamed or copied outside \
             the editor. Give it back its name, or remove it in the game's folder."
                .into(),
        ),
        "particle.value" => (
            match &field {
                Some(field) => format!("An emitter cannot have that value for its {field}"),
                None => "An emitter cannot have that value".into(),
            },
            match (range, parameter("detail")) {
                // Through the format's own width of number: a bound reads as it is written.
                (Some((least, most)), _) => format!(
                    "It may be from {} to {}. Nothing was changed.",
                    least as f32, most as f32
                ),
                (None, Some(detail)) if !detail.is_empty() => {
                    format!("Nothing was changed. Details: {detail}")
                }
                _ => "Nothing was changed.".into(),
            },
        ),
        "particle.conflict" => (
            "The emitter's file changed on disk since the editor read it".into(),
            "Another program wrote it. Nothing was written or removed here. Duplicate the \
             emitter to keep your changes under a new one."
                .into(),
        ),
        "emitter.placed" => (
            "The open level places this emitter".into(),
            match parameter("count") {
                Some("1") => "It is placed once. Remove that object from the level before \
                              removing the emitter."
                    .into(),
                Some(count) => format!(
                    "It is placed {count} times. Remove those objects from the level before \
                     removing the emitter."
                ),
                None => "Remove those objects from the level before removing the emitter.".into(),
            },
        ),
        "particle.locked" => (
            "Another editor is writing the emitter's file".into(),
            "Nothing was written or removed here. Try again in a moment. If no other editor \
             is open, one that stopped unexpectedly left a file ending in .lock beside the \
             emitter's file: remove that file."
                .into(),
        ),
        "particle.missing" => (
            "The emitter's file is no longer there".into(),
            "It was removed outside the editor. Save the emitter to write it again.".into(),
        ),
        "particle.read_failed" => (
            "The emitter's file could not be read".into(),
            "Check that the game's assets/particles folder can be read.".into(),
        ),
        "particle.write_failed" => (
            "The emitter could not be written".into(),
            "Check that the game's assets/particles folder can be written to.".into(),
        ),
        "particle.limit" => (
            "The file is larger than an emitter can be".into(),
            "It may be another file under an emitter's name.".into(),
        ),
        "particle.decode" => (
            "The file is not an emitter this editor can read".into(),
            "It lacks something, or names something this editor does not know. Bring the \
             editor and the game to the same version."
                .into(),
        ),
        "particle.schema" => (
            "The emitter is of another version than this editor reads".into(),
            "Bring the editor and the game to the same version.".into(),
        ),
        _ => return problem(code, parameter),
    };
    Problem { title, remedy }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<&'static str> {
        None
    }

    #[test]
    fn every_refusal_about_the_game_folder_is_a_sentence_and_never_its_code() {
        for code in [
            "runtime.folder_missing",
            "runtime.not_a_folder",
            "runtime.folder_read",
            "runtime.file_read",
            "runtime.outside_folder",
            "runtime.requirements_missing",
            "runtime.preference_read",
            "runtime.preference_decode",
            "runtime.preference_limit",
            "runtime.preference_path",
            "runtime.preference_schema",
            "runtime.preference_locked",
            "runtime.preference_write",
            "runtime.preference_encode",
            "runtime.preference_unavailable",
            "runtime.something_new",
        ] {
            let words = game_folder(code, none);
            let said = format!("{} {}", words.title, words.remedy);
            assert!(!said.contains("runtime."), "{code}: {said}");
            assert!(!said.contains("--game"), "{code}: {said}");
            assert!(
                !words.title.is_empty() && !words.remedy.is_empty(),
                "{code}"
            );
        }
        // A setting that cannot be read names its file, which is what is to be put right.
        let file = |key: &str| (key == "path").then_some("/settings/runtime-install.json");
        let words = game_folder("runtime.preference_decode", file);
        assert!(words.remedy.contains("left as it is"), "{}", words.remedy);
        assert!(words.remedy.contains("/settings/runtime-install.json"));
        assert!(game_folder("runtime.requirements_missing", none)
            .remedy
            .contains("target/debug/server"));
    }

    #[test]
    fn every_refusal_about_the_project_is_a_sentence_that_says_nothing_was_lost() {
        for code in [
            "project.convert_required",
            "project.schema_unsupported",
            "project.locked",
            "project.changed",
            "project.changed_since_read",
            "project.profile_unusable",
            "project.outside_folder",
            "project.levels_rejected",
            "project.conflict",
            "project.value",
            "project.decode",
            "project.schema_missing",
            "project.limit",
            "project.read",
            "project.write",
            "project.create_folder",
            "project.start_level_missing",
            "project.folder_taken",
            "project.folder_name",
            "project.parent_missing",
            "project.path_invalid",
            "project.folder_missing",
        ] {
            let words = project(code, none);
            assert!(!words.title.contains("project."), "{code}: {}", words.title);
            assert!(!words.title.contains("Something went wrong"), "{code}");
            assert!(!words.remedy.is_empty(), "{code}");
        }
        // A backup that is there already: the file is named, and it is not written over.
        let backup = |key: &str| (key == "path").then_some("/game/arc-project.json.schema-1.bak");
        let words = project("project.conflict", backup);
        assert!(words
            .remedy
            .contains("does not write over /game/arc-project.json.schema-1.bak"));
        // A newer file says which version it is, and that it is not one to convert.
        let newer = |key: &str| (key == "schema").then_some("9");
        let words = project("project.schema_unsupported", newer);
        assert!(words.remedy.contains("version 9") && words.remedy.contains("newer editor"));
        // A save of the settings that is refused says which setting, in the dialog's words.
        let shared = |key: &str| match key {
            "field" => Some("controls.jump"),
            "shares_with" => Some("controls.move_forward"),
            _ => None,
        };
        let words = project("project.value", shared);
        assert_eq!(words.title, "Two actions are on one key");
        assert!(words
            .remedy
            .starts_with("Move forward and Jump are on the same key"));
        let unknown = |key: &str| (key == "field").then_some("controls.toggle_view");
        let words = project("project.value", unknown);
        assert!(words
            .remedy
            .starts_with("Change view is on a key the game does not know"));
        let path = |key: &str| (key == "field").then_some("rendering.profile");
        assert_eq!(
            project("project.value", path).title,
            "The look profile's path cannot be used"
        );
        // A name is what it was said of before.
        assert!(project("project.value", none)
            .remedy
            .contains("name of 1 to 96"));
        let profile = |key: &str| match key {
            "profile" => Some("assets/render/dusk.json"),
            "reason" => Some("look.missing"),
            _ => None,
        };
        let words = project("project.profile_unusable", profile);
        assert!(words.remedy.starts_with("assets/render/dusk.json: "));
        assert!(!words.remedy.contains("look.missing"), "{}", words.remedy);
        assert!(words.remedy.contains("Nothing was saved"));
        let words = project("project.changed_since_read", none);
        assert!(words.remedy.contains("Nothing was saved") && words.remedy.contains("kept"));
        // Each reason a level file is passed over for is said in words.
        assert_eq!(
            level_rejection("source.decode", "expected value at line 1"),
            "it is not a level this editor can read (expected value at line 1)"
        );
        assert!(level_rejection("project.outside_folder", "").contains("outside"));
        assert!(level_rejection("schema.unsupported", "x").contains("different version"));
        assert!(level_rejection("box.extents", "").contains("box.extents"));
    }

    #[test]
    fn a_refused_emitter_value_names_its_field_and_the_range_it_may_be_in() {
        let refused = |key: &str| (key == "field").then_some("emission.duration_seconds");
        let words = emitter("particle.value", refused, Some((0.001_f32 as f64, 3600.0)));
        assert_eq!(
            words.title,
            "An emitter cannot have that value for its duration"
        );
        assert_eq!(
            words.remedy,
            "It may be from 0.001 to 3600. Nothing was changed."
        );
        // A file whose name is no ID, or not the ID in it, is said as that.
        let named = |key: &str| (key == "field").then_some("id");
        let words = emitter("particle.value", named, None);
        assert_eq!(
            words.title,
            "The file's name is not the ID of the emitter in it"
        );
        // Without a range the format's own detail is given, and never the code.
        let odd = |key: &str| (key == "detail").then_some("least is above most");
        let words = emitter("particle.value", odd, None);
        assert_eq!(words.title, "An emitter cannot have that value");
        assert!(words.remedy.contains("least is above most"));
    }

    #[test]
    fn every_emitter_refusal_is_a_sentence_and_every_number_has_a_label() {
        for code in [
            "particle.value",
            "emitter.placed",
            "particle.conflict",
            "particle.locked",
            "particle.missing",
            "particle.read_failed",
            "particle.write_failed",
            "particle.limit",
            "particle.decode",
            "particle.schema",
        ] {
            let words = emitter(code, none, None);
            assert!(
                !words.title.contains("particle."),
                "{code}: {}",
                words.title
            );
            assert!(!words.title.contains("Something went wrong"), "{code}");
            assert!(!words.remedy.is_empty(), "{code}");
        }
        // A file that changed on disk: nothing was written, and how to keep the changes.
        assert!(emitter("particle.conflict", none, None)
            .remedy
            .contains("Duplicate"));
        for field in crate::particles::NUMBER_FIELDS {
            let label = emitter_field(field.field);
            assert_ne!(label, field.field, "no label for {}", field.field);
        }
        assert_eq!(emitter_field("something.else"), "something.else");
    }

    #[test]
    fn a_known_problem_is_a_sentence_and_never_shows_its_code() {
        for code in [
            "save.conflict",
            "save.backup_exists",
            "save.backup_failed",
            "save.parent_create",
            "io.failed",
            "name.invalid",
            "box.extents",
            "revision.stale",
            "payload.hash_mismatch",
            "package.identity_mismatch",
            "package.directory_identity",
            "package.manifest_limit",
        ] {
            let problem = problem(code, none);
            assert!(!problem.title.is_empty(), "{code}");
            assert!(!problem.title.contains(code), "{code}: {}", problem.title);
            assert!(
                !problem.title.ends_with('.'),
                "{code}: a title has no full stop"
            );
        }
    }

    #[test]
    fn an_unknown_problem_is_still_shown_with_its_code_and_detail() {
        let shown = problem("something.new", |key| (key == "detail").then_some("disk 7"));
        assert!(shown.title.contains("something.new"));
        assert!(shown.remedy.contains("disk 7"));
    }

    #[test]
    fn a_stale_request_says_that_nothing_was_changed() {
        assert!(problem("inspector.stale", none)
            .remedy
            .contains("Nothing was changed"));
    }

    #[test]
    fn fields_units_and_changes_have_plain_words() {
        assert_eq!(field_label("field.translation"), "Position");
        assert_eq!(field_label("field.new_thing"), "new_thing");
        assert_eq!(unit("metres"), "m");
        assert_eq!(change_label("drag"), "Move");
    }

    #[test]
    fn unsaved_wins_over_cook_needed_and_both_need_attention() {
        assert_eq!(saved_state(true, true), ("Unsaved", true));
        assert_eq!(saved_state(false, true), ("Saved, cook needed", true));
        assert_eq!(saved_state(false, false), ("Saved", false));
    }

    #[test]
    fn an_end_the_person_asked_for_is_a_word_and_any_other_end_is_a_notice() {
        assert_eq!(play_end_asked("play.stopped"), Some("Stopped"));
        assert_eq!(
            play_end_asked("play.window_closed"),
            Some("The game window was closed")
        );
        // A level the game cannot run is refused before it starts, in words and not a code.
        for cause in [
            "play.terrain_unsupported",
            "play.spawn_required",
            "play.runtime_limit",
            "play.preflight_busy",
        ] {
            let shown = play_end("play.start_failed", Some(cause)).unwrap();
            assert_eq!(shown.title, "The game did not start");
            assert!(!shown.remedy.is_empty(), "{cause}");
            assert!(!shown.remedy.contains(cause), "{cause}: {}", shown.remedy);
        }
        assert_eq!(play_end("play.stopped", None), None);
        assert_eq!(play_end("play.window_closed", None), None);
        for code in ["play.crashed", "play.start_failed"] {
            assert_eq!(play_end_asked(code), None, "{code}");
            assert!(play_end(code, None).is_some(), "{code}");
        }
    }

    #[test]
    fn a_game_that_cannot_be_launched_yet_is_said_so_and_not_blamed_on_the_level() {
        let shown = play_end(
            "play.start_failed",
            Some("play.runtime_adapter_unavailable"),
        )
        .unwrap();
        assert_eq!(shown.title, "The game did not start");
        assert!(shown.remedy.contains("cannot launch the game yet"));
        assert!(shown.remedy.contains("not at fault"));
        assert!(!shown.remedy.contains("play."), "no code in the words");
    }

    #[test]
    fn a_cause_the_table_does_not_know_is_still_named() {
        let shown = play_end("play.crashed", Some("runtime.panic")).unwrap();
        assert!(shown.remedy.contains("runtime.panic"));
    }

    #[test]
    fn what_a_run_left_behind_names_the_server_or_the_game() {
        assert!(play_left_behind("play.server_cleanup_failed")
            .unwrap()
            .title
            .contains("server"));
        assert!(play_left_behind("play.client_cleanup_failed").is_some());
        assert_eq!(play_left_behind("play.stopped"), None);
    }

    #[test]
    fn a_refusal_is_one_sentence_and_an_unknown_one_keeps_its_code() {
        assert_eq!(
            play_refusal("play.already_active"),
            "A game is already running. Stop it first."
        );
        assert!(play_refusal("play.something_new").contains("play.something_new"));
    }

    #[test]
    fn how_a_process_ended_is_its_signal_or_its_exit_code() {
        assert_eq!(
            exit_facts(Some(101), None).as_deref(),
            Some("exit code 101")
        );
        assert_eq!(
            exit_facts(None, Some(9)).as_deref(),
            Some("ended by signal 9")
        );
        assert_eq!(exit_facts(None, None), None);
    }

    #[test]
    fn a_hash_is_shortened_to_eight_digits() {
        assert_eq!(short_hash("9f3a61c2aabbccdd"), "9f3a61c2");
        assert_eq!(short_hash("9f3a"), "9f3a");
    }
}
