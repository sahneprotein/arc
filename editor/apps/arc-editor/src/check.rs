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
//! | `click-rule=N` | a click on the Nth rule of the landscape tool's list |
//! | `step-rule-layer=P` | a click on the chosen rule's layer button: `grows-on`, `kept-out-by`, `fills` |
//! | `type-rule=S:TEXT` | a click into setting S of the chosen rule (counted from 0), select all, TEXT, Enter |
//! | `click-layer=N` | a click on the Nth layer of the landscape tool's list |
//! | `type-layer=TEXT` | a click into the layer's name field, select all, TEXT, Enter |
//!
//! Steps that call the editor directly, for set-up: `select=N`, `reparent=CHILD:PARENT` (row
//! numbers), `look-at=X:Y:Z` (the point the view turns about), `tilt=ROW:DEGREES` (turn an
//! object about X), `inspector` (show the
//! inspector, hide the hierarchy), `save`, `cook`, `spoil-cooked` (something else writes into
//! the cooked level on disk).
//!
//! `wait-run=STATE` plays nothing: the script waits until the game the editor started is in
//! that state (`none`, `starting`, `running`, `stopping`). A game runs on its own time, and
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
//! `ARC_EDITOR_CHECK_WINDOW=WIDTHxHEIGHT` asks the desktop for a window of that size in
//! logical pixels, for the cases that check the layout at a given width. The desktop may give
//! another.

use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use bevy::{
    gizmos::transform_gizmo::{TransformGizmoSettings, TransformGizmoState},
    input::keyboard::Key,
    picking::PickingSystems,
    prelude::*,
    render::view::screenshot::{save_to_disk, Screenshot},
    window::{PrimaryWindow, RequestRedraw},
};
use editor_ui::{panels::hierarchy::RowObject, Brush, BrushAim, Editor, PanelChoice, Tool};

use crate::{
    brush::Stroke,
    input_script::{ignore_the_desktop, play_input, InputScript, Screen},
    viewport::{GroundShown, Orbit, Shown},
};

/// Frames before the first step, between steps, and to settle before the picture and the exit.
const FIRST_STEP_AT: u32 = 40;
const BETWEEN_STEPS: u32 = 10;
const SETTLE: u32 = 40;

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
            })
            .init_resource::<InputScript>()
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
}

/// How many frames a `wait-run` step waits before it counts as failed.
const WAIT_FOR_THE_GAME: u32 = 20_000;

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
    (tool, brush, aim, stroke): (Res<Tool>, Res<Brush>, Res<BrushAim>, Res<Stroke>),
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
        let now = if editor.restarting() {
            "Restarting".to_owned()
        } else {
            format!("{:?}", editor_ui::panels::play::PlayView::of(&editor).run)
        };
        if now.eq_ignore_ascii_case(&wanted) {
            script.waiting_for = None;
            script.next_step_at = script.frame + BETWEEN_STEPS;
        } else if script.frame >= give_up_at {
            script.waiting_for = None;
            script
                .failed
                .push(format!("wait-run={wanted}: the game stayed {now}"));
        } else {
            redraw.write(RequestRedraw);
        }
        return;
    }
    if let Some(step) = script.steps.pop() {
        if let Some(wanted) = step.strip_prefix("wait-run=") {
            script.waiting_for = Some((wanted.to_owned(), script.frame + WAIT_FOR_THE_GAME));
            redraw.write(RequestRedraw);
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
    // A reactive check reads the result at rest and takes no picture: a picture needs frames.
    if !reactive && script.frame == finished_at + SETTLE {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(script.picture.clone()));
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
            "ARC_EDITOR_CHECK_RESULT selected={:?} position={:?} rotation={:?} half_size={:?} objects={} dirty={} cook_needed={} cooked={} problem={} undo={:?} redo={:?} dragging={} tool={:?} camera=({:.3},{:.3},{:.2}) failed_steps={:?} gizmo_active={} gizmo_hover={:?} cursor={:?} window={}x{} redrawn={} ground_tiles={} has_file={} sculpt={} brush={:?} brush_aim={:?} stroke_active={} ground_height=({:.3},{:.3}) panels={:?} notice={} question={:?} level={:?} level_dialog={:?} empty_hint={} paint={} layers={} paint_layer={:?} layer_weights={:?} rows_shown={} folded={} rules={:?} rule={:?} grass={} filled={:?} flat_note={} authored={} cooked_source={} layer_rows={} rule_rows={} toolbar_height={:?} toolbar_fits={:?} toolbar_folded={} menu_open={} play_cooked={} play_older={} play_check={} play_stop={} play_run={:?} play_end={} play_refusal={} run_notice={} left_notice={} shutdown={:?} game={} game_wakes={} selection={} places={:?} offsets={:?} several={:?} dashes={:?} snap={} moved_notice={} notice_actions={:?} opened={} play_word={:?} duplicate={:?} snap_steps=({}, {})",
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
    let selected = || {
        snapshot
            .selection
            .first()
            .map(|id| id.0.clone())
            .ok_or_else(|| "nothing is selected".to_owned())
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
                .gizmo_x_handle(&selected()?)
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
                .gizmo_y_ring(&selected()?, pixels(argument)?)
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
