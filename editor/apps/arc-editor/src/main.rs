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
//! Started without arguments, the editor opens on the project browser: the projects that
//! were opened before, and the two ways to another one, each through the desktop's own
//! folder chooser (`editor_ui::chooser`). The editor is entered on the project that is
//! chosen, in the same window.
//!
//! `arc-editor <project.aproj>` starts on that project: the browser is entered on it at
//! once, as if it had been chosen there, and says why if it is refused.
//!
//! The other arguments are for development and for scripts, and each of them starts the
//! editor without the browser, as it always started:
//!
//! `arc-editor [--level <file>] [--package <directory>] [--game <folder>]`: the level file is
//! opened if it exists. Otherwise the editor starts with the game's current map, converted, and
//! saves it there. Without `--level` the file is the level the game's project starts with
//! (`project.aproj` in the game's folder), and without a project
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
    window::{PresentMode, PrimaryWindow, WindowResolution},
    winit::{EventLoopProxyWrapper, UpdateMode, WinitSettings, WinitUserEvent},
};
use editor_ui::{
    theme::{color, feathers::to_color},
    Editor, EditorUiPlugin, Launcher, Particles, Project,
};

/// Where the editor writes when it is not told otherwise, relative to the working directory.
const DEFAULT_LEVEL: &str = "levels/current-map.level.json";
const DEFAULT_PACKAGE: &str = "cooked";

/// What the window is called, and what it is called with a project.
const WINDOW_TITLE: &str = "Arc Editor";

struct Arguments {
    /// The level file, if one was named.
    level: Option<PathBuf>,
    package: PathBuf,
    game: Option<PathBuf>,
    /// Whether anything was named at all: a level, a package or a game.
    any: bool,
    /// The project file to start on, if one was named.
    project: Option<PathBuf>,
}

/// How the editor is started, as the error for wrong arguments says it.
const USAGE: &str =
    "usage: arc-editor [<project.aproj>] | [--level <file>] [--package <directory>] [--game <folder>]";

/// Whether the editor starts in the project browser: when a project file was named, which
/// the browser enters at once, or when nothing was named and no script checks it. A
/// scripted check that is about the browser asks for it by naming the file its list is kept
/// in.
fn starts_in_browser(
    any_argument: bool,
    project_file: bool,
    scripted: bool,
    scripted_browser: bool,
) -> bool {
    if project_file {
        true
    } else if scripted {
        scripted_browser
    } else {
        !any_argument
    }
}

/// The window's title with this project's name in it.
fn window_title(project: Option<&str>) -> String {
    match project {
        Some(name) => format!("{name} - {WINDOW_TITLE}"),
        None => WINDOW_TITLE.to_owned(),
    }
}

fn arguments() -> Result<Arguments, String> {
    let mut parsed = Arguments {
        level: None,
        package: DEFAULT_PACKAGE.into(),
        game: None,
        any: false,
        project: None,
    };
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        parsed.any = true;
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
            other if other.starts_with('-') => return Err(format!("unknown argument {other:?}")),
            // A path that is not an option is the project file to start on.
            _ if parsed.project.is_none() => parsed.project = Some(PathBuf::from(arg)),
            _ => return Err("only one project file can be opened".to_owned()),
        }
    }
    if parsed.project.is_some() {
        // The project file says what is opened; nothing else may say otherwise.
        if parsed.level.is_some()
            || parsed.game.is_some()
            || parsed.package != PathBuf::from(DEFAULT_PACKAGE)
        {
            return Err(
                "a project file is opened on its own, without --level, --package or --game"
                    .to_owned(),
            );
        }
        parsed.any = false;
    }
    Ok(parsed)
}

fn main() -> AppExit {
    let started = Instant::now();
    let arguments = match arguments() {
        Ok(arguments) => arguments,
        Err(problem) => {
            eprintln!("error: {problem}\n{USAGE}");
            return AppExit::from_code(2);
        }
    };
    // The editor starts with the game's current map, converted by the editor core. If the
    // level file exists it is opened in its place; if opening fails, the map stays and the
    // problem is shown.
    let game = arguments.game.clone();
    // Without anything named, the editor opens on the project browser and is entered on the
    // project that is chosen there. Until then it holds no project: what is built below is
    // the editor as it is without one, behind the browser, and nothing of it is written or
    // read from where the editor happens to be started.
    let launcher = if starts_in_browser(
        arguments.any,
        arguments.project.is_some(),
        check::is_scripted(),
        check::scripted_browser().is_some(),
    ) {
        // Where the game is that plays every project is the editor's one setting for this
        // person, kept beside the list; a check keeps its own beside its own list, or has
        // none, and never reads or writes the person's.
        let (list, game_file) = match (check::scripted_browser(), check::is_scripted()) {
            (Some(list), _) => {
                let game_file = list.with_file_name("game-folder.json");
                (Some(list), Some(game_file))
            }
            (None, true) => (None, None),
            (None, false) => (
                editor_ui::launcher::default_list(),
                editor_ui::launcher::default_game_file(),
            ),
        };
        let mut launcher = Launcher::at_start(list).with_game_file(game_file);
        // A project file named on the command line is opened as if it were chosen in the
        // browser: entered in the first frame, or refused there, in words.
        if let Some(file) = &arguments.project {
            launcher.open_file(file);
        }
        launcher
    } else {
        // Without the browser at the start it is shown when the person leaves the open project
        // for another: with the person's own list and game setting, and for a scripted check
        // with none, so that no check reads or writes those of whoever is at the machine.
        if check::is_scripted() {
            Launcher::off().returning_to(None, None)
        } else {
            Launcher::off().returning_to(
                editor_ui::launcher::default_list(),
                editor_ui::launcher::default_game_file(),
            )
        }
    };
    // The project of the game's folder, as it is on disk: nothing is written by looking. A
    // folder without a project file is the person's to set up in the Project settings.
    // A scripted check may name another folder.
    let project = Project::of_folder(check::scripted_project().or_else(|| game.clone()));
    // The level: the one that was named; without a name, the one the project starts with;
    // without a project, the file an editor without a project always used. Behind the
    // project browser there is no project and so no level of one: the level is the game's
    // current map, unsaved, with a file and a cooked folder that are nowhere
    // (`no_project_paths`), until the project that is chosen takes its place. The working
    // directory is whatever the desktop made it, and nothing in it is opened or written.
    let (level, saved_level, package) = if launcher.shown() {
        let (level, package) =
            editor_ui::launcher::no_project_paths(&std::env::temp_dir(), std::process::id());
        (level, None, package)
    } else {
        let level = arguments
            .level
            .or_else(|| project.start_level())
            .unwrap_or_else(|| DEFAULT_LEVEL.into());
        let saved_level = level.exists().then(|| level.clone());
        (level, saved_level, arguments.package)
    };
    // The game's look profile, which Game look in the 3D view is drawn from: read before the
    // window opens. It is the profile the project names; a folder that is no project yet has
    // the one a game always had. It is read again whenever the Project settings are saved
    // (`look::follow_saved_settings`), so that a profile chosen there is the one at once. A
    // scripted check may name a file.
    let read_look = |path: PathBuf| {
        let read = editor_ui::look::load_look_profile(&path);
        (path, read)
    };
    let (look_status, game_look) = look::look_status(
        check::scripted_look()
            .map(read_look)
            .or_else(|| project.look_profile())
            .or_else(|| {
                game.as_deref()
                    .map(editor_ui::look::look_profile_path)
                    .map(read_look)
            }),
    );
    // The game's particle emitters, read from its folder once before the window opens and
    // again whenever the Particles tool is chosen. A scripted check may name another folder.
    // The same folder is where a level's cook reads the emitters the level places.
    let emitter_game = check::scripted_particles().or_else(|| game.clone());
    let particles = emitter_game
        .clone()
        .map_or_else(Particles::default, Particles::of_game);
    let mut editor = match Editor::new(scene_data::current_map(), level, package) {
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
        // A folder named on the command line is the game's and the project's at once.
        (None, Some(folder)) => editor.connect_game(folder.clone(), folder),
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
                    title: WINDOW_TITLE.into(),
                    resolution: WindowResolution::new(width, height),
                    // A scripted check keeps its window off the desktop unless it is asked
                    // to show it; in real use the window is always shown.
                    visible: check::window_shown(),
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
    .insert_resource(launcher)
    // A scripted check has no desktop to show a folder chooser on: it names the answers.
    .insert_resource(if check::is_scripted() {
        editor_ui::FolderChooser::scripted()
    } else {
        editor_ui::FolderChooser::default()
    })
    .insert_non_send(editor)
    .init_resource::<check::GameWakes>()
    .add_systems(Startup, wake_for_the_game)
    .add_systems(Update, follow_entered_project);
    app.run()
}

/// What is set up once for the editor's project is set up again for a project that is
/// entered from the browser: the Play coordinator of that project's game wakes the editor,
/// and the window is called by the project's name. A project that was left for the browser
/// takes its name from the window.
fn follow_entered_project(
    launcher: Res<Launcher>,
    project: Res<Project>,
    editor: NonSend<Editor>,
    proxy: Res<EventLoopProxyWrapper>,
    wakes: Res<check::GameWakes>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut entered: Local<(u64, u64)>,
) {
    let now = (launcher.times_entered(), launcher.times_left());
    if std::mem::replace(&mut *entered, now) == now {
        return;
    }
    if launcher.shown() {
        window.title = window_title(None);
        return;
    }
    wake_for_the_game(editor, proxy, wakes);
    window.title = window_title(project.config().map(|config| config.project.name.as_str()));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_browser_is_the_first_screen_unless_something_was_named_or_a_script_checks() {
        // A person starts the editor: nothing named, no script.
        assert!(starts_in_browser(false, false, false, false));
        // A level, a package or a game named on the command line goes straight in.
        assert!(!starts_in_browser(true, false, false, false));
        // A scripted check goes straight in, with or without arguments, unless it is about
        // the browser and says so.
        assert!(!starts_in_browser(false, false, true, false));
        assert!(!starts_in_browser(true, false, true, false));
        assert!(starts_in_browser(false, false, true, true));
        // A project file is entered through the browser, for a person and a script alike.
        assert!(starts_in_browser(false, true, false, false));
        assert!(starts_in_browser(false, true, true, false));
    }

    #[test]
    fn the_window_is_called_by_the_project_that_is_open() {
        assert_eq!(window_title(None), "Arc Editor");
        assert_eq!(window_title(Some("Harbour")), "Harbour - Arc Editor");
    }
}
