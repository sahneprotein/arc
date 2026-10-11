//! The arc level editor.
//!
//! - `viewport.rs`: the 3D view: camera, the level's objects as meshes, picking.
//! - `brush.rs`: where the landscape brush points, and its ring.
//! - `shortcuts.rs`: keyboard shortcuts.
//! - `stage.rs`: what the 3D view shows while the Particles tool is chosen.
//! - `check.rs`: start-up timings, and the scripted steps used to check a build.
//! - `input_script.rs`: plays input into the window for those checks.
//!
//! The panels, the theme and the link to the editor core are in the `editor-ui` crate.
//!
//! `arc-editor [--level <file>] [--package <directory>] [--game <folder>]`: the level file is
//! opened if it exists. Otherwise the editor starts with the game's current map, converted, and
//! saves it there. Without `--level` the file is the level the game's project starts with
//! (`arc-project.json` in the game's folder), and without a project
//! `levels/current-map.level.json`.
//!
//! `--game` names the game's folder, the one its server and client are built in. With it the
//! Play control can ask the editor's core to start the game on a cooked level; without it the
//! control says that no game is set up. The game's particle emitters are kept in that folder
//! too: without it the Particles tool has nowhere to read or write them, and says so.

mod brush;
mod check;
mod input_script;
mod look;
mod shortcuts;
mod stage;
mod viewport;

use std::{path::PathBuf, time::Instant};

use bevy::{
    picking::mesh_picking::MeshPickingPlugin,
    prelude::*,
    render::RenderPlugin,
    window::{PresentMode, WindowResolution},
    winit::{EventLoopProxyWrapper, UpdateMode, WinitSettings, WinitUserEvent},
};
use editor_ui::{
    theme::{color, feathers::to_color},
    Editor, EditorUiPlugin, Particles, Project,
};

/// Where the editor writes when it is not told otherwise, relative to the working directory.
const DEFAULT_LEVEL: &str = "levels/current-map.level.json";
const DEFAULT_PACKAGE: &str = "cooked";

struct Arguments {
    /// The level file, if one was named.
    level: Option<PathBuf>,
    package: PathBuf,
    game: Option<PathBuf>,
}

fn arguments() -> Result<Arguments, String> {
    let mut parsed = Arguments {
        level: None,
        package: DEFAULT_PACKAGE.into(),
        game: None,
    };
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        let name = arg.to_string_lossy().into_owned();
        // A path is taken as the system gives it: it need not be text.
        let mut path = || -> Result<PathBuf, String> {
            args.next()
                .map(PathBuf::from)
                .ok_or_else(|| format!("{name} needs a path"))
        };
        match name.as_str() {
            "--level" => parsed.level = Some(path()?),
            "--package" => parsed.package = path()?,
            "--game" => parsed.game = Some(path()?),
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    Ok(parsed)
}

fn main() -> AppExit {
    let started = Instant::now();
    let arguments = match arguments() {
        Ok(arguments) => arguments,
        Err(problem) => {
            eprintln!(
                "error: {problem}\nusage: arc-editor [--level <file>] [--package <directory>] [--game <folder>]"
            );
            return AppExit::from_code(2);
        }
    };
    // The editor starts with the game's current map, converted by the editor core. If the
    // level file exists it is opened in its place; if opening fails, the map stays and the
    // problem is shown.
    let game = arguments.game.clone();
    // The project of the game's folder, as it is on disk: nothing is written by looking. A
    // folder without a project file, or with one of an older version, is the person's to
    // set up or to convert, in the Project settings. A scripted check may name another
    // folder.
    let project = Project::of_folder(check::scripted_project().or_else(|| game.clone()));
    // The level: the one that was named; without a name, the one the project starts with;
    // without a project, the file an editor without a project always used.
    let level = arguments
        .level
        .or_else(|| project.start_level())
        .unwrap_or_else(|| DEFAULT_LEVEL.into());
    let saved_level = level.exists().then(|| level.clone());
    // The game's look profile, which Game look in the 3D view is drawn from: looked for in
    // the game's folder, once, before the window opens. A scripted check may name a file.
    let (look_status, game_look) = look::look_status(
        check::scripted_look()
            .or_else(|| game.as_deref().map(editor_ui::look::look_profile_path))
            .map(|path| {
                let read = editor_ui::look::load_look_profile(&path);
                (path, read)
            }),
    );
    // The game's particle emitters, read from its folder once before the window opens and
    // again whenever the Particles tool is chosen. A scripted check may name another folder.
    // The same folder is where a level's cook reads the emitters the level places.
    let emitter_game = check::scripted_particles().or_else(|| game.clone());
    let particles = emitter_game
        .clone()
        .map_or_else(Particles::default, Particles::of_game);
    let mut editor = match Editor::new(scene_data::current_map(), level, arguments.package) {
        Ok(mut editor) => {
            editor.set_emitter_game(emitter_game);
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
    // The game to play from here, if one was named: the core's Play coordinator, which owns
    // the game's processes. A scripted check may put a stand-in in the game's place.
    let connected = match (check::scripted_game(), game) {
        (Some(script), _) => script.handle().map(|handle| editor.connect_play(handle)),
        (None, Some(folder)) => editor.connect_game(folder),
        (None, None) => Ok(()),
    };
    if let Err(problem) = connected {
        eprintln!("error: the part of the editor that starts the game did not start: {problem}");
        return AppExit::from_code(2);
    }
    // A scripted check has no desktop to show a log in: it notes what would have been shown.
    if let Some(opener) = check::scripted_opener() {
        editor.set_opener(opener);
    }

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
        look::LookPlugin,
        brush::BrushPlugin,
        shortcuts::ShortcutsPlugin,
        stage::StagePlugin,
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
    .insert_resource(look_status)
    .insert_resource(look::GameLook(game_look))
    .insert_resource(particles)
    .insert_resource(project)
    .insert_non_send(editor)
    .init_resource::<check::GameWakes>()
    .add_systems(Startup, wake_for_the_game);
    app.run()
}

/// Lets the Play coordinator wake the editor. The editor sleeps while nothing happens in its
/// window, and a game runs, becomes ready or ends on its own time: the coordinator calls this
/// from its own thread whenever a run changes, and the editor draws a frame and looks.
fn wake_for_the_game(
    editor: NonSend<Editor>,
    proxy: Res<EventLoopProxyWrapper>,
    wakes: Res<check::GameWakes>,
) {
    let proxy = (**proxy).clone();
    let wakes = wakes.0.clone();
    editor.set_play_waker(Box::new(move || {
        // The window may be gone when the last change of a run arrives; then nobody looks.
        if proxy.send_event(WinitUserEvent::WakeUp).is_ok() {
            wakes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    }));
}
