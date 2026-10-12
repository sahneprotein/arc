//! Start-up timings, and what is needed to check a build without a person at the window.
//!
//! Always: one log line with the time from process start to the first frame, to the first
//! frame with the level in the hierarchy, and to the first frame with the level in the 3D view.
//!
//! Only when `ARC_EDITOR_CHECK` names a file: the editor runs the steps in
//! `ARC_EDITOR_CHECK_STEPS` (comma separated), saves one picture of the window to that file,
//! prints one line that starts with `ARC_EDITOR_CHECK_RESULT` and exits. A person never
//! needs this.
//!
//! Steps that play input the way the desktop delivers it (`input_script.rs`), so that picking,
//! focus, the widgets, the gizmo and the editor's handlers all run for real:
//!
//! | Step | What is played |
//! |---|---|
//! | `click-row=N` | a click on hierarchy row N |
//! | `click-object=N` | a click in the 3D view on the object of hierarchy row N |
//! | `shift-click-row=N`, `shift-click-object=N` | the same two with Shift held from before the click until after it |
//! | `click-button=Caption` | a click on the button with that caption |
//! | `click-checkbox=Caption` | a click on the checkbox with that caption |
//! | `choose=Menu>Caption` | a click on the button with that caption if one is on the window, as the tools and panel switches are in a wide window; otherwise a click on the button of that toolbar menu and then on its row with that caption |
//! | `click-item=Caption` | a click on the row with that caption of the menu that is open |
//! | `drag-gizmo=P` | a drag of P pixels along the gizmo's X handle |
//! | `rotate-gizmo=D` | a drag along the gizmo's ring about Y, D degrees round it |
//! | `hold-gizmo=P` | the same as `drag-gizmo` without the release; `release` ends it |
//! | `leave` | the cursor leaves the window and comes back to the middle of the view |
//! | `drag-camera=X:Y` | a right-button drag in the 3D view |
//! | `wheel=L` | L lines of the wheel over the 3D view |
//! | `stroke=X1:Z1:X2:Z2` | a drag with the brush between two points of the ground plane |
//! | `hold-stroke=…` | the same without the release; `release` ends it |
//! | `erase-stroke=…` | the same as `stroke` with Shift held throughout |
//! | `aim-ground=X:Z` | the cursor moves to the point of the ground plane at world X, Z and rests |
//! | `key=K` | `ctrl+z`, `ctrl+shift+z`, `ctrl+y`, `ctrl+s`, `ctrl+shift+s`, `ctrl+o`, `ctrl+n`, `escape`, `enter`, `delete`, `down`, `w`, `e`, `r`, `t`, `f5`, `shift+f5`, `ctrl+d` |
//! | `type=TEXT` | TEXT typed into whatever has the keyboard |
//! | `type-field=F.A:TEXT` | a click into component A of field F, select all, TEXT, Enter |
//! | `unfocus` | the window loses the keyboard focus |
//! | `click-ground=X:Z` | a click on the point of the ground plane at world X, Z |
//! | `type-name=TEXT` | a click into the name field, select all, TEXT, Enter |
//! | `close` | the desktop asks the window to close |
//! | `click-toggle=N` | a click on the fold toggle of hierarchy row N |
//! | `drag-row=A:B` | a drag of hierarchy row A onto row B, or with `top` for B onto the list where no row is |
//! | `hold-row=A:B` | the same without the release; `release` ends it |
//! | `click-rule=N` | a click on the Nth rule of the landscape tool's list |
//! | `step-rule-layer=P` | a click on the chosen rule's layer button: `grows-on`, `kept-out-by`, `fills` |
//! | `type-rule=S:TEXT` | a click into setting S of the chosen rule (counted from 0), select all, TEXT, Enter |
//! | `click-layer=N` | a click on the Nth layer of the landscape tool's list |
//! | `type-layer=TEXT` | a click into the layer's name field, select all, TEXT, Enter |
//! | `click-emitter=N` | a click on row N of the emitters' list, counted from 0 |
//! | `type-emitter=F.C:TEXT` | a click into number C of field F of the chosen emitter, by the particle format's dotted name (`emission.rate_per_second.0`), select all, TEXT, Enter |
//! | `type-emitter-name=TEXT` | a click into the emitter's name field, select all, TEXT, Enter |
//!
//! Steps that call the editor directly, for set-up: `select=N`, `reparent=CHILD:PARENT` (row
//! numbers), `look-at=X:Y:Z` (the point the view turns about), `tilt=ROW:DEGREES` (turn an
//! object about X), `inspector` (show the
//! inspector, hide the hierarchy), `save`, `cook`, `spoil-cooked` (something else writes into
//! the cooked level on disk), `preview-steps=N` (N whole steps of the particle preview's
//! reference simulation, whether it runs or not).
//!
//! `wait-gpu` plays nothing either: the script waits until the Particles tool's GPU preview
//! is no longer starting, which is when the graphics card has said that it draws the effect,
//! or the stage has said why it does not.
//!
//! `ARC_EDITOR_CHECK_PROJECT` names a folder that stands in the game's place for the project
//! file; the result line says what the editor found of it (`project=`) and whether the
//! Project settings are open (`project_dialog=`). At its end it says what the settings are
//! as the dialog shows them (`settings_words`).
//!
//! `scroll-project=PIXELS` plays nothing: it puts the content of the Project settings that
//! far up, so that a step after it finds a setting that is lower than the dialog is tall. A
//! click on something the dialog does not show would go to whatever is there.
//!
//! `ARC_EDITOR_CHECK_PARTICLES` names a folder that stands in the game's place for the
//! particle emitters; what the result line says of them is at `emitters_words`.
//!
//! The result line ends with how the processes of the last run left (`play_exit_words`).
//!
//! `wait-run=STATE` plays nothing: the script waits until the game the editor started is in
//! that state (`none`, `starting`, `running`, `stopping`). A wait for a state of a run that
//! there is none of fails at once, with how the last run ended: a game that did not start,
//! or that ended before it got there, is not waited for. A game runs on its own time, and
//! the editor does not count it as work it waits for, so a step that follows a start or a
//! stop says what it waits for. A run that is being stopped to be cooked and played again is
//! in none of them (`restarting`), so that `wait-run=running` after "Cook and play again"
//! waits for the second run. `ARC_EDITOR_CHECK_GAME` puts a scripted stand-in in the game's
//! place (`editor_ui::play_script`).
//!
//! A check opens no file on the desktop: "Open log" only notes which file it would have shown
//! (`opened=` in the result). With `ARC_EDITOR_CHECK_OPEN=missing` it fails as it does for a
//! log that is gone.
//!
//! The result line says what the saved picture is (`picture=`): `nonblank`, or `blank` when
//! every pixel of it has the same colour, as happens when the display is asleep and the
//! desktop gives the window nothing drawn. A blank picture is evidence of nothing: neither
//! that a look is right nor that it is wrong. `none` when no picture was taken, `unknown`
//! when its pixels could not be read.
//!
//! `ARC_EDITOR_CHECK_WINDOW=WIDTHxHEIGHT` asks the desktop for a window of that size in
//! logical pixels, for the cases that check the layout at a given width. The desktop may give
//! another.
//!
//! A check's window is not put on the desktop: a run of checks must not cover the screen of
//! whoever is at the machine (`window_shown=false` in the result). The steps and the result
//! are the same as with a shown window. No picture is taken of a hidden window, and the
//! result says `picture=none`: the desktop gives such a window nothing drawn, and a picture
//! of it would be of one colour whatever the editor shows. `ARC_EDITOR_CHECK_VISIBLE=1` shows
//! the window, for a picture and for looking at a check while it runs.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use bevy::{
    gizmos::transform_gizmo::{TransformGizmoSettings, TransformGizmoState},
    input::keyboard::Key,
    picking::PickingSystems,
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured},
    ui::ScrollPosition,
    window::{PrimaryWindow, RequestRedraw},
};
use editor_ui::{
    panels::hierarchy::{RowDrag, RowObject},
    panels::project::ProjectBody,
    Brush, BrushAim, Editor, PanelChoice, Tool,
};

use crate::{
    brush::Stroke,
    input_script::{ignore_the_desktop, play_input, InputScript, Screen},
    viewport::{GroundShown, Orbit, Shown},
};

/// Frames before the first step, between steps, and to settle before the picture and the exit.
const FIRST_STEP_AT: u32 = 40;
const BETWEEN_STEPS: u32 = 10;
const SETTLE: u32 = 40;

/// What the picture a scripted check saved turned out to be.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Picture {
    /// No picture was taken, or it has not arrived.
    #[default]
    None,
    /// Every pixel has the same colour: the window was given nothing drawn.
    Blank,
    Nonblank,
    /// The picture's pixels are not four bytes each, or are not there.
    Unknown,
}

impl Picture {
    fn word(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Blank => "blank",
            Self::Nonblank => "nonblank",
            Self::Unknown => "unknown",
        }
    }
}

/// What a picture of this size with these bytes is, four bytes a pixel. Only the three
/// colour bytes are compared: a picture of one colour is blank whatever its alpha says.
pub fn picture_of(width: u32, height: u32, data: Option<&[u8]>) -> Picture {
    let pixels = width as usize * height as usize;
    let Some(data) = data.filter(|data| pixels > 0 && data.len() == pixels * 4) else {
        return Picture::Unknown;
    };
    let first = &data[..3];
    if data.chunks_exact(4).all(|pixel| &pixel[..3] == first) {
        Picture::Blank
    } else {
        Picture::Nonblank
    }
}

/// Notes what the saved picture is, for the result line.
fn note_picture(captured: On<ScreenshotCaptured>, mut picture: ResMut<Picture>) {
    let image = &captured.image;
    *picture = picture_of(image.width(), image.height(), image.data.as_deref());
}

/// Whether this run is a scripted check.
pub fn is_scripted() -> bool {
    std::env::var_os("ARC_EDITOR_CHECK").is_some()
}

/// How often the Play coordinator has woken the editor. A check reads it to see that the
/// editor is told of a run's changes and does not merely come across them.
#[derive(Resource, Default, Clone)]
pub struct GameWakes(pub std::sync::Arc<std::sync::atomic::AtomicU64>);

/// The stand-in for the game a scripted check asks for with `ARC_EDITOR_CHECK_GAME`: `runs`,
/// `hangs`, `crashes`, `window-closed`, `server-stays`, `needs-kill` or `unavailable`. Without it, and
/// outside a check, there is none: the editor's game is the real one or not there.
pub fn scripted_game() -> Option<editor_ui::play_script::Script> {
    if !is_scripted() {
        return None;
    }
    editor_ui::play_script::Script::named(&std::env::var("ARC_EDITOR_CHECK_GAME").ok()?)
}

/// What stands in for the desktop's way to show a file in a scripted check: nothing is
/// opened. It succeeds, or with `ARC_EDITOR_CHECK_OPEN=missing` fails as for a file that is
/// gone.
pub fn scripted_opener() -> Option<editor_ui::session::Opener> {
    if !is_scripted() {
        return None;
    }
    let missing = std::env::var("ARC_EDITOR_CHECK_OPEN").is_ok_and(|how| how == "missing");
    Some(Box::new(move |_| {
        if missing {
            Err(editor_core::Diagnostic::new("path.missing"))
        } else {
            Ok(())
        }
    }))
}

/// The look profile a scripted check names with `ARC_EDITOR_CHECK_LOOK`: a file that is
/// looked in for the game's look profile, in place of the game's folder. A file that is not
/// there, or is no profile, is what a check of a refusal names.
pub fn scripted_look() -> Option<PathBuf> {
    if !is_scripted() {
        return None;
    }
    std::env::var_os("ARC_EDITOR_CHECK_LOOK").map(PathBuf::from)
}

/// The folder a scripted check names with `ARC_EDITOR_CHECK_PARTICLES`: it stands in the
/// game's place for the particle emitters, which are read from and written to its
/// `assets/particles` folder. Without it a check has no game to keep emitters in.
pub fn scripted_particles() -> Option<PathBuf> {
    if !is_scripted() {
        return None;
    }
    std::env::var_os("ARC_EDITOR_CHECK_PARTICLES").map(PathBuf::from)
}

/// The folder a scripted check names with `ARC_EDITOR_CHECK_PROJECT`: it stands in the
/// game's place for the project file, `arc-project.json`, and for what a setup makes there.
pub fn scripted_project() -> Option<PathBuf> {
    if !is_scripted() {
        return None;
    }
    std::env::var_os("ARC_EDITOR_CHECK_PROJECT").map(PathBuf::from)
}

/// Whether the editor's window is put on the desktop: always in real use, and in a scripted
/// check only when `ARC_EDITOR_CHECK_VISIBLE` asks for it.
pub fn window_shown() -> bool {
    shown(
        is_scripted(),
        std::env::var("ARC_EDITOR_CHECK_VISIBLE").ok().as_deref(),
    )
}

/// The rule of [`window_shown`]: `asked` is what `ARC_EDITOR_CHECK_VISIBLE` holds, and an
/// empty value or `0` asks for nothing.
fn shown(scripted: bool, asked: Option<&str>) -> bool {
    !scripted || asked.is_some_and(|value| !matches!(value.trim(), "" | "0"))
}

/// Whether a check takes its picture: a reactive check takes none, since a picture needs
/// frames, and a hidden window gives none that shows anything.
fn takes_picture(reactive: bool, shown: bool) -> bool {
    !reactive && shown
}

/// The window size a scripted check asks for, if it names one.
pub fn wanted_window() -> Option<(u32, u32)> {
    if !is_scripted() {
        return None;
    }
    let size = std::env::var("ARC_EDITOR_CHECK_WINDOW").ok()?;
    let (width, height) = size.split_once('x')?;
    Some((width.parse().ok()?, height.parse().ok()?))
}

/// Whether the check leaves the editor to ask for its own frames, as in real use: with
/// `ARC_EDITOR_CHECK_REACTIVE` set, the script wakes the editor only with the input events it
/// plays, one frame an event, and never afterwards. A step begins, and the result is read,
/// when the editor has been at rest for [`REST`]: what is laid out then is what a person who
/// does not touch the mouse is looking at.
pub fn is_reactive() -> bool {
    std::env::var_os("ARC_EDITOR_CHECK_REACTIVE").is_some()
}

/// How long the editor must have drawn nothing to count as at rest, in a reactive check. The
/// application lets an idle editor wake once a second in such a check ([`REACTIVE_WAKE`]).
const REST: Duration = Duration::from_millis(800);

/// How often an idle editor wakes by itself in a reactive check. In real use it is five
/// seconds; a shorter wait only makes the check faster, it adds no frame while work is shown.
pub const REACTIVE_WAKE: Duration = Duration::from_secs(1);

/// A digest of everything authored on the level's landscapes: heights, layers, painted
/// weights and rules, independent of the order they are stored in. Two levels with the same
/// landscapes have the same digest.
fn authored_digest(document: &editor_core::SceneDocument) -> String {
    let mut lines: Vec<String> = Vec::new();
    for object in &document.objects {
        let scene_data::ObjectData::Terrain(terrain) = &object.data else {
            continue;
        };
        let id = &object.id.0;
        lines.push(format!(
            "{id} grid {} {:?} {:?}",
            terrain.cells_per_tile, terrain.spacing_xz_m, terrain.surface_group
        ));
        for sample in &terrain.samples {
            lines.push(format!(
                "{id} height {:?} {:08x}",
                sample.key,
                sample.height_m.to_bits()
            ));
        }
        for layer in &terrain.layers {
            lines.push(format!("{id} layer {layer:?}"));
        }
        for rule in &terrain.rules {
            lines.push(format!("{id} rule {rule:?}"));
        }
        for tile in &terrain.tiles {
            lines.push(format!("{id} tile {} {:?}", tile.id.0, tile.key));
            for payload in &tile.layers {
                let weights: String = payload
                    .weights
                    .iter()
                    .map(|weight| format!("{:08x}", weight.to_bits()))
                    .collect();
                lines.push(format!(
                    "{id} weights {} {} {} {weights}",
                    tile.id.0, payload.layer.0, payload.side_samples
                ));
            }
        }
    }
    lines.sort();
    // FNV-1a, 64 bits: enough to tell two levels apart, and no dependency.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in lines.iter().flat_map(|line| line.bytes().chain(*b"\n")) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

pub struct CheckPlugin {
    pub started: Instant,
}

impl Plugin for CheckPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Startup {
            started: self.started,
            first_frame: None,
            hierarchy: None,
            view: None,
            reported: false,
        })
        .add_systems(Update, (startup_timings, early_frames));
        if let Some(picture) = std::env::var_os("ARC_EDITOR_CHECK") {
            let steps = std::env::var("ARC_EDITOR_CHECK_STEPS")
                .map(|steps| steps.split(',').map(str::to_owned).rev().collect())
                .unwrap_or_default();
            app.insert_resource(Scripted {
                picture: picture.into(),
                steps,
                frame: 0,
                next_step_at: FIRST_STEP_AT,
                finished_at: None,
                failed: Vec::new(),
                last_frame: Instant::now(),
                waiting_for: None,
                no_run_frames: 0,
            })
            .init_resource::<InputScript>()
            .init_resource::<Picture>()
            .add_observer(log_clicks)
            .add_systems(
                First,
                (ignore_the_desktop, play_input)
                    .chain()
                    .before(PickingSystems::Input),
            )
            .add_systems(Update, scripted);
        }
    }
}

#[derive(Resource)]
struct Startup {
    started: Instant,
    first_frame: Option<Duration>,
    hierarchy: Option<Duration>,
    view: Option<Duration>,
    reported: bool,
}

/// What the check reads of the dialogs and of the hint in an empty level.
type Dialogs<'w, 's> = (
    Res<'w, editor_ui::Unsaved>,
    Res<'w, editor_ui::LevelDialog>,
    Query<'w, 's, &'static Node, With<editor_ui::panels::empty::EmptyHint>>,
    Res<'w, editor_ui::PaintLayer>,
    Res<'w, editor_ui::panels::hierarchy::Collapsed>,
    Res<'w, editor_ui::ChosenRule>,
    Res<'w, editor_ui::MenuOpen>,
    Res<'w, editor_ui::Shutdown>,
    Res<'w, GameWakes>,
);

/// What the check reads of the tools and of a row that is dragged in the hierarchy.
type Tools<'w> = (
    Res<'w, Tool>,
    Res<'w, Brush>,
    Res<'w, BrushAim>,
    Res<'w, Stroke>,
    Res<'w, RowDrag>,
    Res<'w, editor_ui::ViewMode>,
    Res<'w, editor_ui::LookStatus>,
    Res<'w, Picture>,
);

/// What the check reads and sets of the Particles tool: the game's emitters, the preview,
/// and the stage in the 3D view.
type Emitters<'w> = (
    ResMut<'w, editor_ui::Particles>,
    ResMut<'w, editor_ui::Preview>,
    Res<'w, crate::stage::Stage>,
    Res<'w, editor_ui::GpuPreview>,
    Res<'w, editor_ui::Project>,
    Res<'w, editor_ui::ProjectDialog>,
);

/// What the result line says of the project's settings as the dialog shows them:
///
/// `project_draft=Unchanged project_saved=false project_keys=W,S,A,D,Space,V,Escape project_look=mouse+stick project_profile=assets/render/look-v1.json project_clients=unnamed`
///
/// `project_draft` is whether they are the file's (`Unchanged`), were changed and can be
/// saved (`Changed`), or cannot be (`SharedKey`, `Refused`); `project_saved` whether the
/// last thing done to them was a save that went through. The keys are in the manifest's
/// order: forward, backward, left, right, jump, view, cursor. Without a project that is
/// read there are none (`project_draft=None`).
fn settings_words(project: &editor_ui::Project) -> String {
    use editor_ui::project::{DraftState, KeyAction};
    let Some(settings) = project.settings() else {
        return "project_draft=None project_saved=false project_keys= project_look= \
                project_profile= project_clients=none"
            .to_owned();
    };
    let draft = match project.draft_state() {
        DraftState::Unchanged => "Unchanged",
        DraftState::Changed => "Changed",
        DraftState::SharedKey { .. } => "SharedKey",
        DraftState::Refused(_) => "Refused",
    };
    let keys = KeyAction::ALL.map(|action| action.key(&settings.controls).to_owned());
    let look = &settings.controls.look;
    let look = match (look.mouse, look.gamepad_right_stick) {
        (true, true) => "mouse+stick",
        (true, false) => "mouse",
        (false, true) => "stick",
        (false, false) => "none",
    };
    // The local test clients as the dialog shows them: the number, or `unnamed` while the
    // manifest names none and none was set.
    let clients = settings
        .local_clients
        .map_or_else(|| "unnamed".to_owned(), |clients| clients.to_string());
    format!(
        "project_draft={draft} project_saved={} project_keys={} project_look={look} \
         project_profile={} project_clients={clients}",
        project.settings_saved(),
        keys.join(","),
        settings.rendering_profile
    )
}

/// What the result line says of the Particles tool:
///
/// `emitters=[("Sparks", false, false)] emitter=Some("Sparks") emitter_game=true emitter_files=1 emitter_rows=1 emitter_fields=22 emitter_rate=Some(10.0) emitter_shape=Some(Point) emitter_problem=none emitter_refusal=none emitter_history=(false, false) remove_asked=false preview=(0, 0.00, true) stage=(false, 0)`
///
/// Each emitter of the list is its name, whether it has changes that are not saved, and
/// whether its file cannot be read. `emitter_files` is how many emitters another editor
/// would find in the game's folder now. `emitter_history` is whether the chosen emitter has
/// a change to undo and one to redo. `preview` is how many particles are alive, the time the
/// preview has come to, and whether it runs; `stage` whether the 3D view shows the stage
/// and how many particles of the reference simulation it drew in the last frame. After
/// these come `preview_mode=Gpu gpu=Running`: which picture is asked for, and what the stage
/// says became of the GPU preview. Last, `emitter_placed=1 level_schema=2`: how many objects
/// of the level place an emitter, and the schema the level is of.
fn emitters_words(
    particles: &editor_ui::Particles,
    preview: &editor_ui::Preview,
    stage: &crate::stage::Stage,
    gpu: &editor_ui::GpuPreview,
    screen: &Screen,
    level: &editor_core::SceneDocument,
) -> String {
    let rows = particles.rows();
    let code = |refusal: Option<&editor_core::Diagnostic>| {
        refusal.map_or_else(|| "none".to_owned(), |refusal| refusal.code.clone())
    };
    let simulation = preview.simulation();
    format!(
        "emitters={:?} emitter={:?} emitter_game={} emitter_files={} emitter_rows={} emitter_fields={} emitter_rate={:?} emitter_shape={:?} emitter_problem={} emitter_refusal={} emitter_history=({}, {}) remove_asked={} preview=({}, {:.2}, {}) stage=({}, {}) preview_mode={:?} gpu={:?} emitter_placed={} level_schema={}",
        rows.iter()
            .map(|row| (row.name.as_str(), row.unsaved, row.refused()))
            .collect::<Vec<_>>(),
        rows.iter().find(|row| row.chosen).map(|row| row.name.as_str()),
        particles.has_game(),
        particles.game().map_or(0, |game| {
            editor_ui::Particles::of_game(game.to_path_buf()).rows().len()
        }),
        screen.emitter_rows(),
        screen.emitter_fields(),
        particles
            .chosen()
            .map(|emitter| emitter.emission.rate_per_second),
        particles
            .chosen()
            .map(|emitter| editor_ui::particles::ShapeKind::of(&emitter.shape)),
        code(particles.problem()),
        code(particles.chosen_refusal().map(|(refusal, _)| refusal)),
        particles.can_undo(),
        particles.can_redo(),
        particles.removing().is_some(),
        simulation.map_or(0, |simulation| simulation.live()),
        simulation.map_or(0.0, |simulation| simulation.time_seconds()),
        preview.running,
        stage.on(),
        stage.drawn,
        preview.mode,
        gpu,
        level
            .objects
            .iter()
            .filter(|object| matches!(object.data, scene_data::ObjectData::Emitter { .. }))
            .count(),
        level.schema,
    )
}

/// Saves the first frames as pictures into the folder named by `ARC_EDITOR_CHECK_EARLY`, to
/// see what the window shows before the level is drawn.
fn early_frames(mut commands: Commands, mut frame: Local<u32>) {
    *frame += 1;
    let Some(folder) = std::env::var_os("ARC_EDITOR_CHECK_EARLY") else {
        return;
    };
    if matches!(*frame, 1..=12 | 16 | 20 | 30) {
        let path = PathBuf::from(folder).join(format!("frame-{:02}.png", *frame));
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path));
    }
}

fn startup_timings(
    mut startup: ResMut<Startup>,
    rows: Query<(), With<RowObject>>,
    view: Res<Shown>,
) {
    if startup.reported {
        return;
    }
    let now = startup.started.elapsed();
    startup.first_frame.get_or_insert(now);
    if !rows.is_empty() {
        startup.hierarchy.get_or_insert(now);
    }
    if view.ready {
        startup.view.get_or_insert(now);
    }
    if let (Some(first), Some(hierarchy), Some(view)) =
        (startup.first_frame, startup.hierarchy, startup.view)
    {
        startup.reported = true;
        info!(
            "ARC_EDITOR_STARTUP first_frame_ms={} level_in_hierarchy_ms={} level_in_view_ms={}",
            first.as_millis(),
            hierarchy.as_millis(),
            view.as_millis()
        );
    }
}

#[derive(Resource)]
struct Scripted {
    picture: PathBuf,
    /// The steps still to run, last first.
    steps: Vec<String>,
    frame: u32,
    next_step_at: u32,
    /// The frame at which the last step's input had been played.
    finished_at: Option<u32>,
    /// Steps that could not be played, with the reason.
    failed: Vec<String>,
    /// When the check last ran, which is when the editor last drew.
    last_frame: Instant,
    /// The state of the game's run a `wait-run` step waits for, and the frame it gives up at.
    waiting_for: Option<(String, u32)>,
    /// For how many frames in a row a wait has found no run.
    no_run_frames: u32,
}

/// What a `wait-gpu` step waits for: the GPU preview being anything but starting.
const GPU_SETTLED: &str = "gpu-settled";

/// How many frames a `wait-run` step waits before it counts as failed.
const WAIT_FOR_THE_GAME: u32 = 20_000;

/// How many frames in a row there may be no run before a `wait-run` step for a state of a
/// run gives up. A request to play and the run it makes are not told of in the same frame.
const NO_RUN_GRACE: u32 = 30;

/// What a `wait-run` or `wait-gpu` step does in a frame.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Wait {
    /// What was waited for is there.
    Reached,
    /// Not yet.
    GoOn,
    /// It is not waited for any longer, and the step failed for this reason.
    Failed(String),
}

/// What a wait does in a frame. `wanted` is what is waited for and `now` what there is, as
/// the check names the states. `frames_without_a_run` is for how many frames in a row there
/// has been no run, `ended` how the last run ended if one did, and `timed_out` whether the
/// wait has gone on for as long as it may.
///
/// A wait for no run only ends when there is none, or times out. A wait for a state of a
/// run fails once there has been no run for [`NO_RUN_GRACE`] frames: the game did not
/// start, or ended before it got there, and waiting longer would not bring it.
fn wait_for_run(
    wanted: &str,
    now: &str,
    frames_without_a_run: u32,
    ended: Option<&str>,
    timed_out: bool,
) -> Wait {
    if now.eq_ignore_ascii_case(wanted) {
        return Wait::Reached;
    }
    let no_run = now.eq_ignore_ascii_case("none");
    if no_run && frames_without_a_run >= NO_RUN_GRACE {
        return Wait::Failed(match ended {
            Some(ended) => format!("wait-run={wanted}: the run ended as {ended}"),
            None => format!("wait-run={wanted}: no run was started"),
        });
    }
    if timed_out {
        return Wait::Failed(format!("wait-run={wanted}: the game stayed {now}"));
    }
    Wait::GoOn
}

/// How a process of a run left, in one word: `none` if it was never seen to end, its exit
/// code, or `signal` and the signal's number; `,forced` after it if the editor had to end it.
/// `exit` is the code, the signal and whether it was forced.
fn exit_word(exit: Option<(Option<i32>, Option<i32>, bool)>) -> String {
    let Some((code, signal, forced)) = exit else {
        return "none".to_owned();
    };
    let how = match (code, signal) {
        (_, Some(signal)) => format!("signal{signal}"),
        (Some(code), None) => code.to_string(),
        (None, None) => "unknown".to_owned(),
    };
    if forced {
        format!("{how},forced")
    } else {
        how
    }
}

/// What went wrong with cleaning up after a run, in one word: the code, and the reason it
/// gives if it gives one.
fn cleanup_word(code: &str, reason: Option<&str>) -> String {
    match reason {
        Some(reason) => format!("{code}:{reason}"),
        None => code.to_owned(),
    }
}

/// What the result line says of the local test clients of the coordinator's run, the one
/// under way or the last:
///
/// `play_clients=3 play_client_ids=[1,2,3] play_client_pids=3 play_client_exits=[1:0,2:0,3:0]`
///
/// `play_clients` is how many clients the run was started with, `play_client_ids` the
/// number each client process has among them, and `play_client_pids` how many different
/// processes those are. `play_client_exits` is how each client left, by its number, as
/// [`exit_word`] says it, once the run has ended. Without a run, and the exits without an
/// end, they say `none`.
fn play_clients_words(
    run: Option<(u8, &[(Option<u8>, Option<u32>)])>,
    exits: Option<&[(u8, String)]>,
) -> String {
    let exits = exits.map_or_else(
        || "none".to_owned(),
        |exits| {
            let each: Vec<String> = exits
                .iter()
                .map(|(client, exit)| format!("{client}:{exit}"))
                .collect();
            format!("[{}]", each.join(","))
        },
    );
    let Some((asked, clients)) = run else {
        return format!(
            "play_clients=none play_client_ids=none play_client_pids=none play_client_exits={exits}"
        );
    };
    let ids: Vec<String> = clients
        .iter()
        .map(|(id, _)| id.map_or_else(|| "?".to_owned(), |id| id.to_string()))
        .collect();
    let mut pids: Vec<u32> = clients.iter().filter_map(|(_, pid)| *pid).collect();
    pids.sort_unstable();
    pids.dedup();
    format!(
        "play_clients={asked} play_client_ids=[{}] play_client_pids={} play_client_exits={exits}",
        ids.join(","),
        pids.len()
    )
}

/// [`play_clients_words`] of this editor's run.
fn play_clients_of(editor: &Editor) -> String {
    let run = editor.run_clients();
    let exits: Option<Vec<(u8, String)>> = editor.last_end().map(|end| {
        end.cleanup
            .clients
            .iter()
            .map(|client| {
                let exit = client
                    .exit
                    .as_ref()
                    .map(|exit| (exit.code, exit.signal, exit.forced));
                (client.client_id, exit_word(exit))
            })
            .collect()
    });
    play_clients_words(
        run.as_ref()
            .map(|(asked, clients)| (*asked, clients.as_slice())),
        exits.as_deref(),
    )
}

/// What the result line says of how the last run ended and its processes left:
///
/// `play_cause=none play_server_exit=0 play_client_exit=0 play_cleanup=[]`
///
/// `play_cause` is the reason behind `play_end`, if the coordinator gave one. The two exits
/// are as [`exit_word`] says them: a clean stop is `0` for both; the client's is the first
/// client's, and every client's is in `play_client_exits`. `play_cleanup` lists what
/// the coordinator said of the cleanup, which is empty when both left by themselves.
/// Without a run that has ended, all four say `none`. After them comes what
/// [`play_clients_words`] says.
fn play_exit_words(editor: &Editor) -> String {
    let clients = play_clients_of(editor);
    let Some(end) = editor.last_end() else {
        return format!(
            "play_cause=none play_server_exit=none play_client_exit=none play_cleanup=none \
             {clients}"
        );
    };
    let cleanup: Vec<String> = end
        .cleanup
        .diagnostics
        .iter()
        .map(|said| {
            cleanup_word(
                &said.code,
                said.parameters.get("reason").map(String::as_str),
            )
        })
        .collect();
    format!(
        "play_cause={} play_server_exit={} play_client_exit={} play_cleanup=[{}] {clients}",
        end.cause
            .as_ref()
            .map_or("none", |cause| cause.code.as_str()),
        exit_word(
            end.cleanup
                .server
                .as_ref()
                .map(|exit| (exit.code, exit.signal, exit.forced))
        ),
        exit_word(
            end.cleanup
                .client
                .as_ref()
                .map(|exit| (exit.code, exit.signal, exit.forced))
        ),
        cleanup.join(",")
    )
}

/// How the last run ended, for a wait that fails because there is no run: the outcome, and
/// the reason behind it in brackets if there is one.
fn ended_words(editor: &Editor) -> Option<String> {
    editor.last_end().map(|end| match &end.cause {
        Some(cause) => format!("{} ({})", end.primary.code, cause.code),
        None => end.primary.code.clone(),
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "the check reaches into every part once"
)]
fn scripted(
    mut commands: Commands,
    mut script: ResMut<Scripted>,
    mut input: ResMut<InputScript>,
    mut editor: NonSendMut<Editor>,
    mut panels: ResMut<PanelChoice>,
    window: Single<&Window, With<PrimaryWindow>>,
    screen: Screen,
    mut orbit: ResMut<Orbit>,
    gizmo: Res<TransformGizmoSettings>,
    gizmo_state: Res<TransformGizmoState>,
    (shown, ground, closing): (Res<Shown>, Res<GroundShown>, Dialogs),
    (tool, brush, aim, stroke, row_drag, view, look, picture): Tools,
    (mut particles, mut preview, stage, gpu, project, project_dialog): Emitters,
    mut redraw: MessageWriter<RequestRedraw>,
    mut exit: MessageWriter<AppExit>,
) {
    let reactive = is_reactive();
    let since_last_frame = script.last_frame.elapsed();
    script.last_frame = Instant::now();
    script.frame += 1;
    if reactive {
        // The script asks for no frame here: the editor draws when an input event arrives
        // and when it asks itself. Nothing happens until the events of the last step are
        // played, the core has answered and the editor has come to rest.
        if !input.is_done() || since_last_frame < REST || editor.busy() {
            return;
        }
    } else {
        // The editor only draws on input; the script asks for every frame.
        redraw.write(RequestRedraw);
        if !input.is_done() || script.frame < script.next_step_at {
            return;
        }
        // Like a person, the script acts on what it sees: it waits until the core has
        // answered and the panels have had time to show it. A step played while a list is
        // being built anew would be aimed at where a button no longer is.
        if editor.busy() {
            script.next_step_at = script.frame + BETWEEN_STEPS;
            return;
        }
    }
    if let Some((wanted, give_up_at)) = script.waiting_for.clone() {
        let now = if wanted == GPU_SETTLED {
            // The GPU preview has settled once it is no longer starting.
            match &*gpu {
                editor_ui::GpuPreview::Starting => "Starting".to_owned(),
                _ => GPU_SETTLED.to_owned(),
            }
        } else if editor.restarting() {
            "Restarting".to_owned()
        } else {
            format!("{:?}", editor_ui::panels::play::PlayView::of(&editor).run)
        };
        script.no_run_frames = if now.eq_ignore_ascii_case("none") {
            script.no_run_frames + 1
        } else {
            0
        };
        let ended = ended_words(&editor);
        match wait_for_run(
            &wanted,
            &now,
            script.no_run_frames,
            ended.as_deref(),
            script.frame >= give_up_at,
        ) {
            Wait::Reached => {
                script.waiting_for = None;
                script.next_step_at = script.frame + BETWEEN_STEPS;
            }
            Wait::Failed(reason) => {
                script.waiting_for = None;
                script.failed.push(reason);
            }
            Wait::GoOn => {
                redraw.write(RequestRedraw);
            }
        }
        return;
    }
    if let Some(step) = script.steps.pop() {
        if step == "wait-gpu" {
            // An effect that does not start is given up by the stage after some seconds of
            // the clock; frames can come much faster than a game's do, so more are allowed.
            script.waiting_for = Some((
                GPU_SETTLED.to_owned(),
                script.frame + 20 * WAIT_FOR_THE_GAME,
            ));
            redraw.write(RequestRedraw);
            return;
        }
        if let Some(wanted) = step.strip_prefix("wait-run=") {
            script.waiting_for = Some((wanted.to_owned(), script.frame + WAIT_FOR_THE_GAME));
            script.no_run_frames = 0;
            redraw.write(RequestRedraw);
            return;
        }
        if let Some(pixels) = step.strip_prefix("scroll-project=") {
            // Set-up: the content of the Project settings is put this far up.
            match pixels.parse::<f32>() {
                Ok(pixels) => commands.queue(move |world: &mut World| {
                    let mut bodies =
                        world.query_filtered::<&mut ScrollPosition, With<ProjectBody>>();
                    for mut position in bodies.iter_mut(world) {
                        position.y = pixels;
                    }
                }),
                Err(_) => script.failed.push(format!("{step}: not a number")),
            }
            script.next_step_at = script.frame + BETWEEN_STEPS;
            redraw.write(RequestRedraw);
            info!("check step {step}");
            return;
        }
        script.next_step_at = script.frame + BETWEEN_STEPS;
        // The step's first event, or its call into the editor, wakes the editor once.
        redraw.write(RequestRedraw);
        let played = play(
            &step,
            &mut input,
            &mut editor,
            &mut panels,
            &screen,
            &mut orbit,
            (&mut *particles, &mut *preview),
        );
        info!(
            "check step {step}: {played:?}, cursor {:?}",
            window.cursor_position()
        );
        match played {
            // The step goes on with another, played when the editor has shown this one's effect.
            Ok(Some(next)) => script.steps.push(next),
            Ok(None) => {}
            Err(reason) => script.failed.push(format!("{step}: {reason}")),
        }
        return;
    }
    let frame = script.frame;
    let finished_at = *script.finished_at.get_or_insert(frame);
    // A reactive check reads the result at rest; a check without a picture, as of a hidden
    // window, waits as long as one with a picture, so that both read the same frame.
    if takes_picture(reactive, window_shown()) && script.frame == finished_at + SETTLE {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(script.picture.clone()))
            .observe(note_picture);
    } else if reactive || script.frame == finished_at + 2 * SETTLE {
        let snapshot = editor.snapshot();
        let status = &snapshot.status;
        let play = editor_ui::panels::play::PlayView::of(&editor);
        let run_notices = editor_ui::panels::notice::notices(&editor);
        // The lowest and highest sample of all landscapes; a flat new one is 0 to 0.
        let (lowest, highest) = snapshot
            .document
            .objects
            .iter()
            .filter_map(|object| match &object.data {
                scene_data::ObjectData::Terrain(terrain) => Some(terrain),
                _ => None,
            })
            .flat_map(|terrain| terrain.samples.iter().map(|sample| sample.height_m))
            .fold((0.0_f32, 0.0_f32), |(low, high), height| {
                (low.min(height), high.max(height))
            });
        let layers: &[scene_data::terrain::LayerDefinition] =
            editor_ui::session::landscape(&snapshot).map_or(&[], |(_, terrain)| &terrain.layers);
        let rules_now: &[scene_data::terrain::RuleDefinition] =
            editor_ui::session::landscape(&snapshot).map_or(&[], |(_, terrain)| &terrain.rules);
        let selected = snapshot
            .selection
            .first()
            .and_then(|id| snapshot.document.object(id));
        // Where each selected object is, to the millimetre.
        let places: Vec<[f32; 3]> = snapshot
            .selection
            .iter()
            .filter_map(|id| snapshot.document.object(id))
            .map(|object| {
                object
                    .local
                    .translation_m
                    .map(|v| (v * 1000.0).round() / 1000.0 + 0.0)
            })
            .collect();
        println!(
            "ARC_EDITOR_CHECK_RESULT selected={:?} position={:?} rotation={:?} half_size={:?} objects={} dirty={} cook_needed={} cooked={} problem={} undo={:?} redo={:?} dragging={} tool={:?} camera=({:.3},{:.3},{:.2}) failed_steps={:?} gizmo_active={} gizmo_hover={:?} cursor={:?} window={}x{} redrawn={} ground_tiles={} has_file={} sculpt={} brush={:?} brush_aim={:?} stroke_active={} ground_height=({:.3},{:.3}) panels={:?} notice={} question={:?} level={:?} level_dialog={:?} empty_hint={} paint={} layers={} paint_layer={:?} layer_weights={:?} rows_shown={} folded={} rules={:?} rule={:?} grass={} filled={:?} flat_note={} authored={} cooked_source={} layer_rows={} rule_rows={} toolbar_height={:?} toolbar_fits={:?} toolbar_folded={} menu_open={} play_cooked={} play_older={} play_check={} play_stop={} play_run={:?} play_end={} play_refusal={} run_notice={} left_notice={} shutdown={:?} game={} game_wakes={} selection={} places={:?} offsets={:?} several={:?} dashes={:?} snap={} moved_notice={} notice_actions={:?} opened={} play_word={:?} duplicate={:?} snap_steps=({}, {}) active={:?} pivot={:?} gizmo_at={:?} gizmo_row={:?} sizes={:?} turns={:?} under={:?} worlds={:?} kids={:?} drop_target={:?} workspace={:?} review={:?} review_rows={} view={:?} look={} look_sun={:?} ground_look={:?} box_look={:?} {} project={} project_dialog={} picture={} window_shown={} {} {}",
            selected.map(|object| object.name.as_str()),
            selected.map(|object| object.local.translation_m.map(|v| (v * 1000.0).round() / 1000.0)),
            selected.map(|object| {
                editor_ui::panels::inspector::euler_degrees(&object.local.rotation_xyzw)
                    // Adding zero turns a negative zero into zero.
                    .map(|degrees| (degrees * 10.0).round() / 10.0 + 0.0)
            }),
            selected.and_then(|object| match &object.data {
                scene_data::ObjectData::Box { half_extents_m } => {
                    Some(half_extents_m.map(|v| (v * 1000.0).round() / 1000.0))
                }
                _ => None,
            }),
            snapshot.document.objects.len(),
            status.dirty,
            status.cook_needed,
            status.simulation_hash.as_ref().map_or("none".into(), |hash| hash.hex()[..8].to_owned()),
            editor.problem.as_ref().map_or("none".into(), |problem| problem.code.clone()),
            snapshot.undo_label,
            snapshot.redo_label,
            editor.dragging(),
            gizmo.mode,
            orbit.yaw,
            orbit.pitch,
            orbit.distance,
            script.failed,
            gizmo_state.active,
            gizmo_state.hovered_axis,
            window.cursor_position(),
            window.width(),
            window.height(),
            shown.redrawn,
            ground.tiles,
            editor.has_file(),
            *tool == Tool::Sculpt,
            (brush.mode, brush.radius_m),
            aim.0
                .as_ref()
                .map(|hit| hit.point.map(|v| (v * 100.0).round() / 100.0 + 0.0)),
            stroke.is_active(),
            lowest,
            highest,
            panels.shown(),
            editor.problem.is_some(),
            closing.0.question,
            editor_ui::panels::levels::level_name(&editor.level_path),
            *closing.1,
            closing.2.iter().any(|node| node.display != Display::None),
            *tool == Tool::Paint,
            layers.len(),
            layers
                .iter()
                .find(|layer| closing.3.0.as_deref() == Some(layer.id.0.as_str()))
                .map(|layer| layer.name.as_str()),
            // Each layer's average weight, to the hundredth; -1 for a layer a rule fills.
            editor_ui::session::landscape(&snapshot).map_or(Vec::new(), |(_, terrain)| {
                terrain
                    .layers
                    .iter()
                    .map(|layer| {
                        editor_ui::session::layer_average(terrain, layer)
                            .map_or(-1.0, |average| (average * 100.0).round() / 100.0)
                    })
                    .collect()
            }),
            screen.rows_shown(),
            closing.4.objects.len(),
            // Each rule as its row describes it, and the chosen rule's settings as numbers.
            rules_now
                .iter()
                .map(|rule| editor_ui::rules::describe(rule, layers))
                .collect::<Vec<_>>(),
            rules_now
                .iter()
                .find(|rule| closing.5.0.as_deref() == Some(rule.id.0.as_str()))
                .map(|rule| {
                    editor_ui::rules::settings(&rule.algorithm)
                        .iter()
                        .map(|setting| editor_ui::rules::value(rule, setting))
                        .collect::<Vec<_>>()
                }),
            ground.grass,
            ground
                .filled
                .iter()
                .map(|weight| (weight * 1000.0).round() / 1000.0)
                .collect::<Vec<_>>(),
            // Whether the chosen rule's settings carry the note about flat ground, and which.
            rules_now
                .iter()
                .find(|rule| closing.5.0.as_deref() == Some(rule.id.0.as_str()))
                .and_then(editor_ui::rules::flat_ground_note)
                .map_or("none", |note| if note.starts_with("Perfectly") { "fades-in" } else { "below-band" }),
            authored_digest(&snapshot.document),
            editor
                .last_cook
                .as_ref()
                .map_or("none", |(source, _)| &source[..8]),
            screen.layer_and_rule_rows().0,
            screen.layer_and_rule_rows().1,
            screen.toolbar().map(|(height, _)| height),
            screen.toolbar().map(|(_, fits)| fits),
            // Folded is what is laid out: the tool menu's button is there instead of the tools.
            screen.menu_button("Tool").is_some(),
            closing.6.0,
            // The Play control as it would be shown: the cooked level, whether the level has
            // moved on from it, what a check of it found, and whether Stop is there.
            play.cooked.as_deref().unwrap_or("none"),
            play.older,
            match &play.check {
                editor_ui::PackageCheck::NotAsked => "none".to_owned(),
                editor_ui::PackageCheck::Checking => "checking".to_owned(),
                editor_ui::PackageCheck::Passed { .. } => "passed".to_owned(),
                editor_ui::PackageCheck::Refused { code } => format!("refused:{code}"),
            },
            play.stop_shown(),
            // The game: where its run stands, how the last one ended, what was refused,
            // which of its notices are up, and where closing the editor stands.
            play.run,
            play.ended.as_deref().unwrap_or("none"),
            play.refusal.as_deref().unwrap_or("none"),
            run_notices[1].is_some(),
            run_notices[2].is_some(),
            *closing.7,
            play.game,
            closing.8.0.load(std::sync::atomic::Ordering::Relaxed),
            // Several objects: how many are selected and where each is, what the inspector
            // says first, which numbers of the position it shows as dashes, and whether
            // snapping is on, read where it acts: the gizmo turns in steps.
            snapshot.selection.len(),
            places.clone(),
            // Where each stands from the first: what a move or a turn of the group keeps.
            places
                .iter()
                .map(|place| {
                    std::array::from_fn::<f32, 3, _>(|axis| {
                        ((place[axis] - places[0][axis]) * 1000.0).round() / 1000.0 + 0.0
                    })
                })
                .collect::<Vec<_>>(),
            editor_ui::panels::inspector::several(snapshot.selection.len()),
            snapshot
                .inspector
                .iter()
                .find(|field| field.id == "translation")
                .map(|field| editor_ui::panels::inspector::mixed_axes(&snapshot, field)),
            gizmo.snap_rotate.is_some(),
            // What the game's notices offer and what "Open log" would have shown; and the
            // word the Play control shows in its button's place.
            run_notices[3].is_some(),
            run_notices[1]
                .iter()
                .chain(&run_notices[3])
                .flat_map(|notice| notice.actions.iter().map(|action| action.caption()))
                .collect::<Vec<_>>(),
            editor
                .opened()
                .and_then(|path| path.file_name())
                .map_or("none".into(), |name| name.to_string_lossy().into_owned()),
            play.word(),
            // Whether the selection can be duplicated, and the snap steps the core holds.
            editor_ui::session::duplicate_offer(&snapshot),
            snapshot.snap.translation_m,
            snapshot.snap.rotation_degrees,
            // The object the core calls active, where the gizmo of several is wanted, where
            // the gizmo is in the level, to the millimetre, and which row carries its dot.
            snapshot
                .active
                .as_ref()
                .and_then(|id| snapshot.document.object(id))
                .map(|object| object.name.as_str()),
            snapshot.pivot,
            screen
                .gizmo_origin()
                .map(|at| at.to_array().map(|v| (v * 1000.0).round() / 1000.0 + 0.0)),
            editor_ui::panels::hierarchy::marked_object(&snapshot)
                .and_then(|id| snapshot.document.object(id))
                .map(|object| object.name.as_str()),
            // Of every selected object, in the selection's order: its half size if it is a
            // box, and its turns in degrees about X, Y and Z.
            snapshot
                .selection
                .iter()
                .filter_map(|id| snapshot.document.object(id))
                .filter_map(|object| match &object.data {
                    scene_data::ObjectData::Box { half_extents_m } => {
                        Some(half_extents_m.map(|v| (v * 1000.0).round() / 1000.0))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>(),
            snapshot
                .selection
                .iter()
                .filter_map(|id| snapshot.document.object(id))
                .map(|object| {
                    editor_ui::panels::inspector::euler_degrees(&object.local.rotation_xyzw)
                        .map(|degrees| (degrees * 10.0).round() / 10.0 + 0.0)
                })
                .collect::<Vec<_>>(),
            // The hierarchy: what each selected object is under ("top" for none), where each
            // is in the level whatever it is under, to the millimetre, the names of what is
            // under the first selected object's parent, in the list's order, and the place a
            // dragged row would be dropped on.
            snapshot
                .selection
                .iter()
                .filter_map(|id| snapshot.document.object(id))
                .map(|object| {
                    object
                        .parent
                        .as_ref()
                        .and_then(|parent| snapshot.document.object(parent))
                        .map_or("top", |parent| parent.name.as_str())
                })
                .collect::<Vec<_>>(),
            scene_data::world_transforms(&snapshot.document)
                .map(|worlds| {
                    snapshot
                        .selection
                        .iter()
                        .filter_map(|id| worlds.get(id))
                        .map(|world| {
                            world
                                .translation_m
                                .map(|v| (v * 1000.0).round() / 1000.0 + 0.0)
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
            selected.map_or(Vec::new(), |first| {
                snapshot
                    .hierarchy
                    .iter()
                    .filter(|row| row.parent == first.parent)
                    .map(|row| row.name.as_str())
                    .collect::<Vec<_>>()
            }),
            editor_ui::panels::hierarchy::drop_target(&snapshot, &row_drag),
            // The tool that is chosen, what the core's review lists, each as scope and code,
            // and how many problems the Review panel has laid out.
            *tool,
            snapshot
                .review
                .iter()
                .map(|problem| format!("{:?}:{}", problem.scope, problem.diagnostic.code))
                .collect::<Vec<_>>(),
            screen.review_rows(),
            // How the 3D view draws the level, what the editor knows of the game's look
            // profile, and how bright the sun is that Game look made, if it made one.
            *view,
            match &*look {
                editor_ui::LookStatus::NoGame => "none".to_owned(),
                editor_ui::LookStatus::Refused { code, .. } => format!("refused:{code}"),
                editor_ui::LookStatus::Loaded { identity } => format!(
                    "loaded:{}",
                    identity.as_deref().map_or("", |hash| &hash[..8])
                ),
            },
            screen.look_sun(),
            // What the ground's material is made with in place of its defaults: nothing in
            // the editor's own picture, the profile's colour and roughness in Game look.
            screen
                .ground_look()
                .map(|look| (look.base_color_linear_rgba, look.roughness)),
            // What a box is drawn with in Game look, whether the view is drawn so or not:
            // its material's colour and roughness, and the metres one repeat of its texture
            // covers. Nothing without a profile.
            screen.box_look(),
            emitters_words(
                &particles,
                &preview,
                &stage,
                &gpu,
                &screen,
                &snapshot.document,
            ),
            // What the editor found of the project, in one word, with the refusal's code
            // if it was refused; and whether the Project settings are open.
            match project.state() {
                editor_ui::ProjectState::Refused(refusal) => format!("Refused:{}", refusal.code),
                state => state.word().to_owned(),
            },
            project_dialog.open,
            // What the saved picture is: nothing is claimed of a picture that is blank.
            picture.word(),
            // Whether the window was on the desktop while the check ran.
            window_shown(),
            // The project's settings as the dialog shows them.
            settings_words(&project),
            // How the last run of the game ended and its processes left.
            play_exit_words(&editor),
        );
        exit.write(AppExit::Success);
    }
}

/// A click with the left button, with Shift held around it if `shift`.
fn click(input: &mut InputScript, at: Vec2, shift: bool) {
    if shift {
        input.hold_key(KeyCode::ShiftLeft, Key::Shift, true);
    }
    input.click(at, MouseButton::Left);
    if shift {
        input.hold_key(KeyCode::ShiftLeft, Key::Shift, false);
    }
}

/// Plays one step. An error says why the step could not be played at all; whether it had the
/// intended effect is read from the result line. A step that needs the editor to show
/// something before it can go on returns the step to play next.
fn play(
    step: &str,
    input: &mut InputScript,
    editor: &mut Editor,
    panels: &mut PanelChoice,
    screen: &Screen,
    orbit: &mut Orbit,
    (particles, preview): (&mut editor_ui::Particles, &mut editor_ui::Preview),
) -> Result<Option<String>, String> {
    let (name, argument) = step.split_once('=').unwrap_or((step, ""));
    let snapshot = editor.snapshot();
    let row_id = |argument: &str| -> Result<String, String> {
        let index: usize = argument
            .parse()
            .map_err(|_| "not a row number".to_owned())?;
        snapshot
            .hierarchy
            .get(index)
            .map(|row| row.id.0.clone())
            .ok_or_else(|| "no such row".to_owned())
    };
    let pixels = |argument: &str| {
        argument
            .parse::<f32>()
            .map_err(|_| "not a number".to_owned())
    };
    match name {
        "click-row" | "shift-click-row" => {
            let at = screen
                .row(&row_id(argument)?)
                .ok_or("the row is not on screen")?;
            click(input, at, name.starts_with("shift-"));
        }
        "drag-row" | "hold-row" => {
            let (dragged, onto) = argument
                .split_once(':')
                .ok_or("expected ROW:ROW or ROW:top")?;
            let from = screen
                .row(&row_id(dragged)?)
                .ok_or("the dragged row is not on screen")?;
            let to = if onto == "top" {
                screen
                    .hierarchy_blank()
                    .ok_or("the list has no place without a row")?
            } else {
                screen
                    .row(&row_id(onto)?)
                    .ok_or("the row to drop on is not on screen")?
            };
            input.drag(from, to - from, MouseButton::Left, name == "drag-row");
        }
        "click-toggle" => {
            let at = screen
                .toggle(&row_id(argument)?)
                .ok_or("that row has no toggle on screen")?;
            input.click(at, MouseButton::Left);
        }
        "click-object" | "shift-click-object" => {
            let at = screen
                .object(&row_id(argument)?)
                .ok_or("the object is not in the view")?;
            click(input, at, name.starts_with("shift-"));
        }
        "click-button" => {
            let at = screen.button(argument).ok_or("no such button on screen")?;
            input.click(at, MouseButton::Left);
        }
        "click-checkbox" => {
            let at = screen
                .checkbox(argument)
                .ok_or("no such checkbox on screen")?;
            input.click(at, MouseButton::Left);
        }
        "choose" => {
            let (menu, caption) = argument.split_once('>').ok_or("expected MENU>CAPTION")?;
            if let Some(at) = screen.button(caption) {
                input.click(at, MouseButton::Left);
            } else {
                let at = screen.menu_button(menu).ok_or("no such menu on screen")?;
                input.click(at, MouseButton::Left);
                return Ok(Some(format!("click-item={caption}")));
            }
        }
        "click-item" => {
            let at = screen
                .menu_item(argument)
                .ok_or("no such row in an open menu")?;
            input.click(at, MouseButton::Left);
        }
        "drag-gizmo" | "hold-gizmo" => {
            let (from, direction) = screen
                .gizmo_x_handle()
                .ok_or("the gizmo is not in the view")?;
            input.drag(
                from,
                direction * pixels(argument)?,
                MouseButton::Left,
                name == "drag-gizmo",
            );
        }
        "rotate-gizmo" => {
            let (from, to) = screen
                .gizmo_y_ring(pixels(argument)?)
                .ok_or("the gizmo is not in the view")?;
            input.drag(from, to - from, MouseButton::Left, true);
        }
        "release" => input.release(MouseButton::Left),
        "leave" => {
            let back_at = screen.view_centre().ok_or("the view has no room")?;
            input.leave_and_return(back_at);
        }
        "drag-camera" => {
            let (x, y) = argument.split_once(':').ok_or("expected X:Y")?;
            let from = screen.view_centre().ok_or("the view has no room")?;
            input.drag(
                from,
                Vec2::new(pixels(x)?, pixels(y)?),
                MouseButton::Right,
                true,
            );
        }
        "stroke" | "hold-stroke" | "erase-stroke" => {
            // A drag with the brush from one point of the ground plane to another.
            let numbers: Vec<f32> = argument.split(':').map(pixels).collect::<Result<_, _>>()?;
            let &[x1, z1, x2, z2] = numbers.as_slice() else {
                return Err("expected X1:Z1:X2:Z2".into());
            };
            let from = screen
                .ground(x1, z1)
                .ok_or("the start is not in the view")?;
            let to = screen.ground(x2, z2).ok_or("the end is not in the view")?;
            // Shift held from before the button goes down until after it is up erases.
            let erase = name == "erase-stroke";
            if erase {
                input.hold_key(KeyCode::ShiftLeft, Key::Shift, true);
            }
            input.drag(from, to - from, MouseButton::Left, name != "hold-stroke");
            if erase {
                input.hold_key(KeyCode::ShiftLeft, Key::Shift, false);
            }
        }
        "click-ground" => {
            let (x, z) = argument.split_once(':').ok_or("expected X:Z")?;
            let at = screen
                .ground(pixels(x)?, pixels(z)?)
                .ok_or("that point is not in the view")?;
            input.click(at, MouseButton::Left);
        }
        "type-name" => {
            let at = screen.name_field().ok_or("no name field on screen")?;
            input.click(at, MouseButton::Left);
            input.select_all();
            input.type_text(argument);
            input.key(&[], KeyCode::Enter, Key::Enter);
        }
        "close" => input.close(),
        "click-rule" => {
            let number = argument
                .parse()
                .map_err(|_| "not a rule number".to_owned())?;
            let at = screen.rule(number).ok_or("no such rule on screen")?;
            input.click(at, MouseButton::Left);
        }
        "step-rule-layer" => {
            let place = match argument {
                "grows-on" => editor_ui::session::RulePlace::GrowsOn,
                "kept-out-by" => editor_ui::session::RulePlace::KeptOutBy,
                "fills" => editor_ui::session::RulePlace::Fills,
                _ => return Err("expected grows-on, kept-out-by or fills".into()),
            };
            let at = screen
                .rule_layer(place)
                .ok_or("no such layer button on screen")?;
            input.click(at, MouseButton::Left);
        }
        "type-rule" => {
            let (setting, text) = argument.split_once(':').ok_or("expected SETTING:TEXT")?;
            let setting = setting
                .parse()
                .map_err(|_| "not a setting number".to_owned())?;
            let at = screen
                .rule_field(setting)
                .ok_or("no such setting on screen")?;
            input.click(at, MouseButton::Left);
            input.select_all();
            input.type_text(text);
            input.key(&[], KeyCode::Enter, Key::Enter);
        }
        "click-layer" => {
            let number = argument
                .parse()
                .map_err(|_| "not a layer number".to_owned())?;
            let at = screen.layer(number).ok_or("no such layer on screen")?;
            input.click(at, MouseButton::Left);
        }
        "type-layer" => {
            let at = screen
                .layer_name_field()
                .ok_or("no layer name field on screen")?;
            input.click(at, MouseButton::Left);
            input.select_all();
            input.type_text(argument);
            input.key(&[], KeyCode::Enter, Key::Enter);
        }
        "type" => input.type_text(argument),
        "aim-ground" => {
            // The cursor goes to the point of the ground plane at world X, Z and stays there.
            let (x, z) = argument.split_once(':').ok_or("expected X:Z")?;
            let at = screen
                .ground(pixels(x)?, pixels(z)?)
                .ok_or("that point is not in the view")?;
            input.move_to(at);
        }
        "wheel" => {
            let at = screen.view_centre().ok_or("the view has no room")?;
            input.wheel(at, pixels(argument)?);
        }
        "key" => {
            let control = [KeyCode::ControlLeft];
            let control_shift = [KeyCode::ControlLeft, KeyCode::ShiftLeft];
            let letter = |letter: &str| Key::Character(letter.into());
            match argument {
                "ctrl+z" => input.key(&control, KeyCode::KeyZ, letter("z")),
                "ctrl+shift+z" => input.key(&control_shift, KeyCode::KeyZ, letter("Z")),
                "ctrl+y" => input.key(&control, KeyCode::KeyY, letter("y")),
                "ctrl+s" => input.key(&control, KeyCode::KeyS, letter("s")),
                "ctrl+shift+s" => input.key(&control_shift, KeyCode::KeyS, letter("S")),
                "ctrl+o" => input.key(&control, KeyCode::KeyO, letter("o")),
                "ctrl+n" => input.key(&control, KeyCode::KeyN, letter("n")),
                "ctrl+d" => input.key(&control, KeyCode::KeyD, letter("d")),
                "enter" => input.key(&[], KeyCode::Enter, Key::Enter),
                "delete" => input.key(&[], KeyCode::Delete, Key::Delete),
                "escape" => input.key(&[], KeyCode::Escape, Key::Escape),
                "down" => input.key(&[], KeyCode::ArrowDown, Key::ArrowDown),
                "w" => input.key(&[], KeyCode::KeyW, letter("w")),
                "e" => input.key(&[], KeyCode::KeyE, letter("e")),
                "r" => input.key(&[], KeyCode::KeyR, letter("r")),
                "t" => input.key(&[], KeyCode::KeyT, letter("t")),
                "f5" => input.key(&[], KeyCode::F5, Key::F5),
                "shift+f5" => input.key(&[KeyCode::ShiftLeft], KeyCode::F5, Key::F5),
                _ => return Err("unknown key".into()),
            }
        }
        "type-field" => {
            let (target, text) = argument.split_once(':').ok_or("expected FIELD.AXIS:TEXT")?;
            let (field, axis) = target.split_once('.').ok_or("expected FIELD.AXIS")?;
            let axis = axis.parse().map_err(|_| "not an axis number".to_owned())?;
            let at = screen.field(field, axis).ok_or("no such field on screen")?;
            input.click(at, MouseButton::Left);
            input.select_all();
            input.type_text(text);
            input.key(&[], KeyCode::Enter, Key::Enter);
        }
        "unfocus" => input.unfocus(),
        "select" => editor.select(vec![editor_core::StableId(row_id(argument)?)]),
        "look-at" => {
            // Set-up: the view turns about this point of the level.
            let numbers: Vec<f32> = argument.split(':').map(pixels).collect::<Result<_, _>>()?;
            let &[x, y, z] = numbers.as_slice() else {
                return Err("expected X:Y:Z".into());
            };
            orbit.target = Vec3::new(x, y, z);
        }
        "tilt" => {
            // Set-up: the object of row N is turned DEGREES about the X axis.
            let (row, degrees) = argument.split_once(':').ok_or("expected ROW:DEGREES")?;
            let id = editor_core::StableId(row_id(row)?);
            let object = snapshot.document.object(&id).ok_or("no such object")?;
            editor.submit(editor_core::Command::Edit {
                label_key: "edit".into(),
                mutations: vec![editor_core::Mutation::Transform {
                    object: id,
                    value: scene_data::RigidTransform {
                        translation_m: object.local.translation_m,
                        rotation_xyzw: Quat::from_rotation_x(pixels(degrees)?.to_radians())
                            .to_array(),
                    },
                }],
            });
        }
        "reparent" => {
            // Set-up: the object of row CHILD goes under the object of row PARENT.
            let (child, parent) = argument.split_once(':').ok_or("expected CHILD:PARENT")?;
            editor.reparent(
                editor_core::StableId(row_id(child)?),
                Some(editor_core::StableId(row_id(parent)?)),
            );
        }
        "click-emitter" => {
            // A row of the emitters' list, counted from 0 down the list.
            let index: usize = argument
                .parse()
                .map_err(|_| "not a row number".to_owned())?;
            let rows = particles.rows();
            let row = rows.get(index).ok_or("no such emitter")?;
            let at = screen
                .emitter_row(&row.id)
                .ok_or("the emitter's row is not on screen")?;
            input.click(at, MouseButton::Left);
        }
        "type-emitter" => {
            // A number of the chosen emitter, by the format's dotted name and its place.
            let (target, text) = argument
                .split_once(':')
                .ok_or("expected FIELD.COMPONENT:TEXT")?;
            let (field, component) = target.rsplit_once('.').ok_or("expected FIELD.COMPONENT")?;
            let component = component
                .parse()
                .map_err(|_| "not a component number".to_owned())?;
            let at = screen
                .emitter_field(field, component)
                .ok_or("no such field on screen")?;
            input.click(at, MouseButton::Left);
            input.select_all();
            input.type_text(text);
            input.key(&[], KeyCode::Enter, Key::Enter);
        }
        "type-emitter-name" => {
            let at = screen
                .emitter_name_field()
                .ok_or("no emitter name field on screen")?;
            input.click(at, MouseButton::Left);
            input.select_all();
            input.type_text(argument);
            input.key(&[], KeyCode::Enter, Key::Enter);
        }
        "preview-steps" => {
            // Whole steps of the reference simulation, whether it runs or not: with the
            // preview paused and begun again, what is alive afterwards is the same each run.
            let steps = argument
                .parse()
                .map_err(|_| "not a number of steps".to_owned())?;
            preview.step(steps);
        }
        "inspector" => panels.set(Some(false), Some(true)),
        "save" => editor.save(),
        "cook" => editor.cook(),
        "spoil-cooked" => {
            // Set-up: the cooked level's simulation data is overwritten, as by another program.
            let package = editor.cooked().ok_or("nothing is cooked")?;
            std::fs::write(package.path().join("level.sim.json"), b"spoiled")
                .map_err(|error| error.to_string())?;
        }
        _ => return Err("unknown step".into()),
    }
    Ok(None)
}

/// Says in the log what each click landed on, so that a check that fails can be read.
fn log_clicks(
    click: On<Pointer<Click>>,
    names: Query<(
        Has<crate::viewport::Projected>,
        Has<editor_ui::ViewportSlot>,
        Has<RowObject>,
    )>,
) {
    let (object, view, row) = names.get(click.entity).unwrap_or_default();
    info!(
        "check click on {:?} at {:?}: object={object} view={view} row={row}",
        click.entity, click.pointer_location.position
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wait_for_a_state_of_a_run_fails_at_once_when_there_is_no_run() {
        // What is waited for is there, whatever else is so.
        assert_eq!(
            wait_for_run("running", "Running", 0, None, false),
            Wait::Reached
        );
        assert_eq!(
            wait_for_run("none", "None", 99, Some("play.stopped"), true),
            Wait::Reached
        );
        // A run that is on its way is waited for.
        assert_eq!(
            wait_for_run("running", "Starting", 0, None, false),
            Wait::GoOn
        );
        assert_eq!(
            wait_for_run("running", "Restarting", 0, None, false),
            Wait::GoOn
        );
        // No run for a few frames is no failure yet: the request and its run are not told
        // of in one frame.
        assert_eq!(
            wait_for_run("running", "None", NO_RUN_GRACE - 1, None, false),
            Wait::GoOn
        );
        // A run that ended before it got there, with how it ended and why.
        assert_eq!(
            wait_for_run(
                "running",
                "None",
                NO_RUN_GRACE,
                Some("play.start_failed (play.protocol_mismatch)"),
                false
            ),
            Wait::Failed(
                "wait-run=running: the run ended as play.start_failed (play.protocol_mismatch)"
                    .into()
            )
        );
        // Nothing was started at all.
        assert_eq!(
            wait_for_run("starting", "None", NO_RUN_GRACE, None, false),
            Wait::Failed("wait-run=starting: no run was started".into())
        );
        // A wait for the run to be over is not cut short by a run that is under way, and
        // times out as before; so does a run that stays where it is.
        assert_eq!(wait_for_run("none", "Running", 0, None, false), Wait::GoOn);
        assert_eq!(
            wait_for_run("none", "Stopping", 0, None, true),
            Wait::Failed("wait-run=none: the game stayed Stopping".into())
        );
        assert_eq!(
            wait_for_run("running", "Starting", 0, None, true),
            Wait::Failed("wait-run=running: the game stayed Starting".into())
        );
        // The wait for the GPU preview is never without a run: it only times out.
        assert_eq!(
            wait_for_run(GPU_SETTLED, "Starting", 0, None, false),
            Wait::GoOn
        );
        assert_eq!(
            wait_for_run(GPU_SETTLED, GPU_SETTLED, 0, None, false),
            Wait::Reached
        );
    }

    #[test]
    fn how_a_process_left_is_one_word_with_its_code_signal_and_whether_it_was_forced() {
        assert_eq!(exit_word(None), "none");
        assert_eq!(exit_word(Some((Some(0), None, false))), "0");
        assert_eq!(exit_word(Some((Some(101), None, false))), "101");
        assert_eq!(exit_word(Some((None, Some(15), false))), "signal15");
        assert_eq!(exit_word(Some((None, Some(9), true))), "signal9,forced");
        assert_eq!(exit_word(Some((Some(0), None, true))), "0,forced");
        assert_eq!(exit_word(Some((None, None, false))), "unknown");
        assert_eq!(
            cleanup_word("play.server_cleanup_failed", Some("grace_deadline")),
            "play.server_cleanup_failed:grace_deadline"
        );
        assert_eq!(cleanup_word("play.wait_failed", None), "play.wait_failed");
    }

    #[test]
    fn the_clients_of_a_run_are_said_by_number_and_as_different_processes() {
        assert_eq!(
            play_clients_words(None, None),
            "play_clients=none play_client_ids=none play_client_pids=none play_client_exits=none"
        );
        // A run of three under way: three numbers, three processes, no exits yet.
        let three = [
            (Some(1), Some(40)),
            (Some(2), Some(41)),
            (Some(3), Some(42)),
        ];
        assert_eq!(
            play_clients_words(Some((3, &three)), None),
            "play_clients=3 play_client_ids=[1,2,3] play_client_pids=3 play_client_exits=none"
        );
        // Ended: each client's exit by its number, a missing one as none.
        let exits = [
            (1, "0".to_owned()),
            (2, "signal9,forced".to_owned()),
            (3, "none".to_owned()),
        ];
        assert!(play_clients_words(Some((3, &three)), Some(&exits))
            .ends_with("play_client_exits=[1:0,2:signal9,forced,3:none]"));
        // Two clients said to be one process, and one that was never started, show as that.
        let wrong = [(Some(1), Some(40)), (Some(2), Some(40)), (None, None)];
        assert_eq!(
            play_clients_words(Some((3, &wrong)), None),
            "play_clients=3 play_client_ids=[1,2,?] play_client_pids=1 play_client_exits=none"
        );
    }

    #[test]
    fn a_window_is_hidden_only_in_a_scripted_check_that_does_not_ask_to_be_seen() {
        // Real use: shown, whatever the variable says.
        assert!(shown(false, None));
        assert!(shown(false, Some("0")));
        // A scripted check: hidden unless it is asked for.
        assert!(!shown(true, None));
        assert!(!shown(true, Some("")));
        assert!(!shown(true, Some("0")));
        assert!(!shown(true, Some(" 0 ")));
        assert!(shown(true, Some("1")));
        assert!(shown(true, Some("yes")));
    }

    #[test]
    fn no_picture_is_taken_of_a_hidden_window_and_the_result_says_none() {
        assert!(takes_picture(false, true));
        assert!(!takes_picture(false, false), "a hidden window");
        assert!(!takes_picture(true, true), "a reactive check");
        assert!(!takes_picture(true, false));
        // What the result says while no picture has arrived, which is for good when none
        // is taken: not `blank`, which is said of a picture that was read.
        assert_eq!(Picture::default().word(), "none");
    }

    #[test]
    fn a_picture_of_one_colour_is_blank_and_one_differing_pixel_makes_it_not() {
        // Two by two, all black, with alpha that differs: still one colour.
        let mut data = vec![0, 0, 0, 255, 0, 0, 0, 0, 0, 0, 0, 255, 0, 0, 0, 255];
        assert_eq!(picture_of(2, 2, Some(&data)), Picture::Blank);
        // One colour that is not black is blank as well: nothing was drawn into it.
        let grey: Vec<u8> = [31, 31, 36, 255].repeat(4);
        assert_eq!(picture_of(2, 2, Some(&grey)), Picture::Blank);
        data[9] = 1;
        assert_eq!(picture_of(2, 2, Some(&data)), Picture::Nonblank);
    }

    #[test]
    fn a_picture_whose_pixels_cannot_be_read_is_neither() {
        assert_eq!(picture_of(2, 2, None), Picture::Unknown);
        assert_eq!(picture_of(2, 2, Some(&[0; 12])), Picture::Unknown);
        assert_eq!(picture_of(0, 0, Some(&[])), Picture::Unknown);
        assert_eq!(Picture::default().word(), "none");
        assert_eq!(Picture::Blank.word(), "blank");
        assert_eq!(Picture::Nonblank.word(), "nonblank");
    }
}
