//! The arc level editor.
//!
//! - `viewport.rs`: the 3D view: camera, the level's objects as meshes, picking.
//! - `brush.rs`: where the landscape brush points, and its ring.
//! - `shortcuts.rs`: keyboard shortcuts.
//! - `check.rs`: start-up timings, and the scripted steps used to check a build.
//! - `input_script.rs`: plays input into the window for those checks.
//!
//! The panels, the theme and the link to the editor core are in the `editor-ui` crate.
//!
//! `arc-editor [--level <file>] [--package <directory>]`: the level file is opened if it exists.
//! Otherwise the editor starts with the game's current map, converted, and saves it there.
//! Without `--level` the file is `levels/current-map.level.json`.

mod brush;
mod check;
mod input_script;
mod shortcuts;
mod viewport;

use std::{path::PathBuf, time::Instant};

use bevy::{
    picking::mesh_picking::MeshPickingPlugin,
    prelude::*,
    render::RenderPlugin,
    window::{PresentMode, WindowResolution},
    winit::{UpdateMode, WinitSettings},
};
use editor_ui::{
    theme::{color, feathers::to_color},
    Editor, EditorUiPlugin,
};

/// Where the editor writes when it is not told otherwise, relative to the working directory.
const DEFAULT_LEVEL: &str = "levels/current-map.level.json";
const DEFAULT_PACKAGE: &str = "cooked";

struct Arguments {
    level: PathBuf,
    package: PathBuf,
}

fn arguments() -> Result<Arguments, String> {
    let mut parsed = Arguments {
        level: DEFAULT_LEVEL.into(),
        package: DEFAULT_PACKAGE.into(),
    };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let target = match arg.as_str() {
            "--level" => &mut parsed.level,
            "--package" => &mut parsed.package,
            other => return Err(format!("unknown argument {other:?}")),
        };
        *target = args
            .next()
            .ok_or_else(|| format!("{arg} needs a path"))?
            .into();
    }
    Ok(parsed)
}

fn main() -> AppExit {
    let started = Instant::now();
    let arguments = match arguments() {
        Ok(arguments) => arguments,
        Err(problem) => {
            eprintln!(
                "error: {problem}\nusage: arc-editor [--level <file>] [--package <directory>]"
            );
            return AppExit::from_code(2);
        }
    };
    // The editor starts with the game's current map, converted by the editor core. If the
    // level file exists it is opened in its place; if opening fails, the map stays and the
    // problem is shown.
    let saved_level = arguments.level.exists().then(|| arguments.level.clone());
    let editor = match Editor::new(
        scene_data::current_map(),
        arguments.level,
        arguments.package,
    ) {
        Ok(mut editor) => {
            if let Some(path) = saved_level {
                editor.open(path);
            }
            editor
        }
        Err(problem) => {
            eprintln!("error: the editor core did not start: {problem}");
            return AppExit::from_code(2);
        }
    };

    // A scripted check may ask for another window size, to see the layout at that width.
    let (width, height) = check::wanted_window().unwrap_or((1440, 900));
    let mut app = App::new();
    app.add_plugins((
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Arc Editor".into(),
                    resolution: WindowResolution::new(width, height),
                    // A scripted check must not wait for the desktop to show the window.
                    present_mode: if check::is_scripted() {
                        PresentMode::AutoNoVsync
                    } else {
                        PresentMode::AutoVsync
                    },
                    ..default()
                }),
                // The editor answers a request to close: it asks first when there are unsaved
                // changes (`editor_ui::panels::unsaved`).
                close_when_requested: false,
                ..default()
            })
            // The first frames must not go to the window half made. With pipelines compiled in
            // the background the window is shown for a moment before anything can be drawn into
            // it, and what the desktop shows of such a frame is not ours to choose.
            .set(RenderPlugin {
                synchronous_pipeline_compilation: std::env::var_os("ARC_EDITOR_ASYNC_PIPELINES")
                    .is_none(),
                ..default()
            }),
        MeshPickingPlugin,
        EditorUiPlugin,
        viewport::ViewportPlugin,
        brush::BrushPlugin,
        shortcuts::ShortcutsPlugin,
        check::CheckPlugin { started },
    ))
    // What the engine clears the window with while no camera draws into it, which is the
    // case in the window's first frames (`viewport::FRAMES_BEFORE_CAMERAS`): the editor's own
    // window colour, so that the window opens in it.
    .insert_resource(ClearColor(to_color(color::SURFACE_WINDOW)))
    // Draw only when something happened: an editor at rest costs nothing.
    .insert_resource(if check::is_reactive() {
        // Still drawing only on input and on request, as in real use; an idle editor just
        // wakes sooner, so that a reactive check need not wait five seconds a step.
        WinitSettings {
            focused_mode: UpdateMode::reactive(check::REACTIVE_WAKE),
            unfocused_mode: UpdateMode::reactive(check::REACTIVE_WAKE),
        }
    } else {
        WinitSettings::desktop_app()
    })
    .insert_non_send(editor);
    app.run()
}
