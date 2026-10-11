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

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<&'static str> {
        None
    }

    #[test]
    fn a_known_problem_is_a_sentence_and_never_shows_its_code() {
        for code in [
            "save.conflict",
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
