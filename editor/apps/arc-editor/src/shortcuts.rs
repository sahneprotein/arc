//! Keyboard shortcuts.
//!
//! Ctrl+Z undoes, Ctrl+Shift+Z or Ctrl+Y redoes, Ctrl+S saves, Ctrl+Shift+S saves under
//! another name, Ctrl+O opens a level and Ctrl+N begins a new one. On macOS the Command key
//! works as well as Control. W, E and R choose the move, rotate and scale tool and T the
//! landscape tool, and Delete removes the selection, unless a field has the keyboard. Ctrl+D
//! duplicates the selection. F5
//! cooks the level if it has changed, checks the cooked level and plays it in the game's
//! window; Shift+F5 stops what F5 began, at whatever step it is. When the level has something
//! the game cannot run, F5 cooks nothing and chooses the Review tool, which lists it. While a
//! dialog or a menu is open none of them act.
//!
//! While the Particles tool is chosen the keys that change something act on the chosen
//! emitter, which is what the window shows then: Ctrl+Z and Ctrl+Shift+Z or Ctrl+Y undo and
//! redo its changes, Ctrl+S saves it, Ctrl+D makes a copy of it, and Delete asks whether to
//! remove it.

use bevy::{
    gizmos::transform_gizmo::{TransformGizmoMode, TransformGizmoSettings},
    input_focus::InputFocus,
    prelude::*,
    text::EditableText,
};
use editor_ui::{
    panels::toolbar, Editor, LevelDialog, Modal, Particles, Project, ProjectDialog, Tool,
};

pub struct ShortcutsPlugin;

impl Plugin for ShortcutsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, shortcuts);
    }
}

fn shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    fields: Query<(), With<EditableText>>,
    (mut gizmo, mut tool): (ResMut<TransformGizmoSettings>, ResMut<Tool>),
    (modal, mut levels, mut particles): (Res<Modal>, ResMut<LevelDialog>, ResMut<Particles>),
    (mut project, mut project_dialog): (ResMut<Project>, ResMut<ProjectDialog>),
    mut editor: NonSendMut<Editor>,
) {
    if modal.0 {
        return;
    }
    let command = keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    // A function key is no letter: it acts while a field has the keyboard too.
    if keys.just_pressed(KeyCode::F5) && !command {
        if shift {
            editor.stop_play();
        } else if editor_ui::panels::play::project_lets_play(
            &mut project,
            &mut project_dialog,
            &editor,
        ) {
            // A project the game cannot be started with has its settings opened instead,
            // and nothing is cooked.
            editor_ui::panels::play::cook_and_play(&mut editor, &mut tool);
        }
        return;
    }
    if !command {
        // Letters belong to a text or number field while one has the keyboard.
        let typing = focus.get().is_some_and(|entity| fields.contains(entity));
        if !typing {
            for (key, mode) in [
                (KeyCode::KeyW, TransformGizmoMode::Translate),
                (KeyCode::KeyE, TransformGizmoMode::Rotate),
                (KeyCode::KeyR, TransformGizmoMode::Scale),
            ] {
                if keys.just_pressed(key) {
                    gizmo.mode = mode;
                    *tool = Tool::Objects;
                }
            }
            if keys.just_pressed(KeyCode::KeyT) {
                *tool = Tool::Sculpt;
            }
            if keys.just_pressed(KeyCode::Delete) {
                toolbar::remove(*tool, &mut particles, &mut editor);
            }
        }
        return;
    }
    let emitters = *tool == Tool::Particles;
    if keys.just_pressed(KeyCode::KeyZ) {
        if shift {
            toolbar::redo(*tool, &mut particles, &mut editor);
        } else {
            toolbar::undo(*tool, &mut particles, &mut editor);
        }
    } else if keys.just_pressed(KeyCode::KeyY) {
        toolbar::redo(*tool, &mut particles, &mut editor);
    } else if keys.just_pressed(KeyCode::KeyS) {
        if shift {
            *levels = LevelDialog::SaveAs;
        } else if emitters {
            particles.save_chosen();
        } else {
            editor.save();
        }
    } else if keys.just_pressed(KeyCode::KeyD) {
        if emitters {
            particles.duplicate_chosen();
        } else {
            editor.duplicate_selected();
        }
    } else if keys.just_pressed(KeyCode::KeyO) {
        *levels = LevelDialog::Open;
    } else if keys.just_pressed(KeyCode::KeyN) {
        *levels = LevelDialog::New;
    }
}
