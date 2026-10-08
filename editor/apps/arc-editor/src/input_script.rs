//! Plays input into the window the way the desktop would deliver it.
//!
//! Used by the scripted check only. Each action becomes the same messages a real cursor,
//! mouse button, wheel or key produces, one action per frame, so that everything after the
//! window system runs for real: the engine's input state, picking, focus, the widgets, the
//! gizmo and the editor's own handlers.
//!
//! While a script plays, what the real pointer and keyboard do to the window is dropped
//! (`ignore_the_desktop`). Otherwise a person using the machine, with the cursor passing over
//! the window, would change what the check clicks on.

use std::collections::VecDeque;

use bevy::{
    ecs::system::SystemParam,
    gizmos::transform_gizmo::TransformGizmoSettings,
    input::{
        keyboard::{Key, KeyboardInput},
        mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel},
        touch::TouchPhase,
        ButtonState,
    },
    prelude::*,
    ui::{ComputedNode, UiGlobalTransform},
    ui_widgets::Button,
    window::{
        CursorLeft, CursorMoved, PrimaryWindow, RequestRedraw, WindowCloseRequested, WindowEvent,
        WindowFocused,
    },
};
use editor_ui::panels::{
    hierarchy::{RowObject, RowToggle},
    inspector::{FieldInput, NameInput},
    paint::{LayerNameInput, LayerRow},
    rules::{RuleField, RuleLayerButton, RuleRow},
};

use crate::viewport::{Projected, ViewCamera};

/// One thing the desktop could deliver in a frame.
#[derive(Clone, Debug)]
pub enum Action {
    MoveTo(Vec2),
    Button(MouseButton, ButtonState),
    Key(KeyCode, Key, ButtonState),
    Wheel(f32),
    /// The cursor leaves the window.
    Leave,
    /// The desktop asks the window to close.
    Close,
    /// The window gains or loses the keyboard focus.
    Focus(bool),
    /// A frame without input, for the application to react.
    Wait,
}

/// The actions still to be played, and the cursor and focus the script has given the window.
#[derive(Resource)]
pub struct InputScript {
    queue: VecDeque<Action>,
    cursor: Option<Vec2>,
    focused: bool,
}

impl Default for InputScript {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            cursor: None,
            focused: true,
        }
    }
}

impl InputScript {
    pub fn is_done(&self) -> bool {
        self.queue.is_empty()
    }

    /// Whether an input event is still to come, pauses not counted.
    fn has_events_left(&self) -> bool {
        self.queue
            .iter()
            .any(|action| !matches!(action, Action::Wait))
    }

    fn push(&mut self, actions: impl IntoIterator<Item = Action>) {
        self.queue.extend(actions);
    }

    /// Moves the cursor to `at` and leaves it there.
    pub fn move_to(&mut self, at: Vec2) {
        self.push([Action::MoveTo(at), Action::Wait, Action::Wait]);
    }

    /// Moves the cursor to `at`, presses and releases `button`.
    pub fn click(&mut self, at: Vec2, button: MouseButton) {
        self.push([
            Action::MoveTo(at),
            Action::Wait,
            Action::Button(button, ButtonState::Pressed),
            Action::Wait,
            Action::Button(button, ButtonState::Released),
            Action::Wait,
        ]);
    }

    /// Presses `button` at `from`, moves by `by` in eight steps, and releases. With `release`
    /// false the button stays down, for a drag that something else ends.
    pub fn drag(&mut self, from: Vec2, by: Vec2, button: MouseButton, release: bool) {
        self.push([
            Action::MoveTo(from),
            Action::Wait,
            Action::Button(button, ButtonState::Pressed),
            Action::Wait,
        ]);
        for step in 1..=8 {
            self.push([
                Action::MoveTo(from + by * (step as f32 / 8.0)),
                Action::Wait,
            ]);
        }
        if release {
            self.release(button);
        }
    }

    pub fn release(&mut self, button: MouseButton) {
        self.push([
            Action::Button(button, ButtonState::Released),
            Action::Wait,
            Action::Wait,
        ]);
    }

    /// The cursor leaves the window and, some frames later, comes back at `back_at`.
    pub fn leave_and_return(&mut self, back_at: Vec2) {
        self.push([
            Action::Leave,
            Action::Wait,
            Action::Wait,
            Action::MoveTo(back_at),
            Action::Wait,
        ]);
    }

    /// The desktop asks the window to close, as its close button does.
    pub fn close(&mut self) {
        self.push([Action::Close, Action::Wait, Action::Wait]);
    }

    /// The window loses the keyboard focus.
    pub fn unfocus(&mut self) {
        self.push([
            Action::Focus(false),
            Action::Wait,
            Action::Wait,
            Action::Focus(true),
            Action::Wait,
        ]);
    }

    pub fn wheel(&mut self, at: Vec2, lines: f32) {
        self.push([
            Action::MoveTo(at),
            Action::Wait,
            Action::Wheel(lines),
            Action::Wait,
        ]);
    }

    /// Presses and releases a key, with the modifiers held around it.
    pub fn key(&mut self, modifiers: &[KeyCode], code: KeyCode, logical: Key) {
        for modifier in modifiers {
            self.push([Action::Key(*modifier, Key::Control, ButtonState::Pressed)]);
        }
        self.push([
            Action::Key(code, logical.clone(), ButtonState::Pressed),
            Action::Wait,
            Action::Key(code, logical, ButtonState::Released),
        ]);
        for modifier in modifiers {
            self.push([Action::Key(*modifier, Key::Control, ButtonState::Released)]);
        }
        self.push([Action::Wait]);
    }

    /// Presses a key and keeps it down, or lets it go.
    pub fn hold_key(&mut self, code: KeyCode, logical: Key, down: bool) {
        let state = if down {
            ButtonState::Pressed
        } else {
            ButtonState::Released
        };
        self.push([Action::Key(code, logical, state), Action::Wait]);
    }

    /// Types `text` one character at a time. Digits, a minus sign and a point are enough for
    /// number fields.
    pub fn type_text(&mut self, text: &str) {
        for character in text.chars() {
            let code = match character {
                '0' => KeyCode::Digit0,
                '1' => KeyCode::Digit1,
                '2' => KeyCode::Digit2,
                '3' => KeyCode::Digit3,
                '4' => KeyCode::Digit4,
                '5' => KeyCode::Digit5,
                '6' => KeyCode::Digit6,
                '7' => KeyCode::Digit7,
                '8' => KeyCode::Digit8,
                '9' => KeyCode::Digit9,
                '-' => KeyCode::Minus,
                _ => KeyCode::Period,
            };
            self.key(&[], code, Key::Character(character.to_string().into()));
        }
    }
}

/// Drops what the real pointer and keyboard did to the window this frame, and puts the cursor
/// and the focus back to what the script last made them. Runs first in the frame, before
/// anything reads input.
#[expect(
    clippy::too_many_arguments,
    reason = "one queue per kind of input message"
)]
pub fn ignore_the_desktop(
    script: Res<InputScript>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut moved: ResMut<Messages<CursorMoved>>,
    mut left: ResMut<Messages<CursorLeft>>,
    mut buttons: ResMut<Messages<MouseButtonInput>>,
    mut wheels: ResMut<Messages<MouseWheel>>,
    mut keys: ResMut<Messages<KeyboardInput>>,
    mut focus: ResMut<Messages<WindowFocused>>,
    mut window_events: ResMut<Messages<WindowEvent>>,
) {
    moved.clear();
    left.clear();
    buttons.clear();
    wheels.clear();
    keys.clear();
    focus.clear();
    let kept: Vec<WindowEvent> = window_events
        .drain()
        .filter(|event| {
            !matches!(
                event,
                WindowEvent::CursorMoved(_)
                    | WindowEvent::CursorLeft(_)
                    | WindowEvent::CursorEntered(_)
                    | WindowEvent::MouseButtonInput(_)
                    | WindowEvent::MouseWheel(_)
                    | WindowEvent::MouseMotion(_)
                    | WindowEvent::KeyboardInput(_)
                    | WindowEvent::WindowFocused(_)
            )
        })
        .collect();
    for event in kept {
        window_events.write(event);
    }
    let physical = script
        .cursor
        .map(|position| (position * window.scale_factor()).as_dvec2());
    if window.physical_cursor_position().map(|p| p.as_dvec2()) != physical {
        window.set_physical_cursor_position(physical);
    }
    if window.focused != script.focused {
        window.focused = script.focused;
    }
}

/// Plays one action per frame.
#[expect(
    clippy::too_many_arguments,
    reason = "one writer per kind of input message"
)]
pub fn play_input(
    mut script: ResMut<InputScript>,
    window: Single<(Entity, &mut Window), With<PrimaryWindow>>,
    mut cursor_moved: MessageWriter<CursorMoved>,
    mut cursor_left: MessageWriter<CursorLeft>,
    mut focus_changes: MessageWriter<WindowFocused>,
    mut close_requests: MessageWriter<WindowCloseRequested>,
    mut buttons: MessageWriter<MouseButtonInput>,
    mut wheels: MessageWriter<MouseWheel>,
    mut keys: MessageWriter<KeyboardInput>,
    mut window_events: MessageWriter<WindowEvent>,
    mut redraw: MessageWriter<RequestRedraw>,
) {
    let (entity, mut window) = window.into_inner();
    let Some(action) = script.queue.pop_front() else {
        return;
    };
    if crate::check::is_reactive() {
        // A real device wakes the editor once for each event and not at all afterwards: the
        // next frame is asked for only while an event is still to come, and the pauses after
        // the last event are dropped. Whatever the editor shows after that it must have
        // asked for itself.
        if script.has_events_left() {
            redraw.write(RequestRedraw);
        } else {
            script.queue.clear();
        }
    } else {
        redraw.write(RequestRedraw);
    }
    match action {
        Action::MoveTo(position) => {
            let delta = position - script.cursor.unwrap_or(position);
            script.cursor = Some(position);
            // As the window system records a move; `set_cursor_position` would ask the desktop
            // to move the real cursor.
            let physical = (position * window.scale_factor()).as_dvec2();
            window.set_physical_cursor_position(Some(physical));
            let moved = CursorMoved {
                window: entity,
                position,
                delta: Some(delta),
            };
            cursor_moved.write(moved.clone());
            window_events.write(WindowEvent::CursorMoved(moved));
        }
        Action::Button(button, state) => {
            let input = MouseButtonInput {
                button,
                state,
                window: entity,
            };
            buttons.write(input);
            window_events.write(WindowEvent::MouseButtonInput(input));
        }
        Action::Wheel(lines) => {
            let wheel = MouseWheel {
                unit: MouseScrollUnit::Line,
                x: 0.0,
                y: lines,
                window: entity,
                phase: TouchPhase::Moved,
            };
            wheels.write(wheel);
            window_events.write(WindowEvent::MouseWheel(wheel));
        }
        Action::Key(key_code, logical_key, state) => {
            let text = match (&logical_key, state) {
                (Key::Character(text), ButtonState::Pressed) => Some(text.clone()),
                _ => None,
            };
            let input = KeyboardInput {
                key_code,
                logical_key,
                state,
                text,
                repeat: false,
                window: entity,
            };
            keys.write(input.clone());
            window_events.write(WindowEvent::KeyboardInput(input));
        }
        Action::Focus(focused) => {
            script.focused = focused;
            window.focused = focused;
            let event = WindowFocused {
                window: entity,
                focused,
            };
            focus_changes.write(event.clone());
            window_events.write(WindowEvent::WindowFocused(event));
        }
        Action::Close => {
            let request = WindowCloseRequested { window: entity };
            close_requests.write(request.clone());
            window_events.write(WindowEvent::WindowCloseRequested(request));
        }
        Action::Leave => {
            script.cursor = None;
            // What the window system does when the cursor crosses the window's edge. Not
            // through `set_cursor_position`: that would ask the desktop to move the cursor.
            window.set_physical_cursor_position(None);
            let left = CursorLeft { window: entity };
            cursor_left.write(left.clone());
            window_events.write(WindowEvent::CursorLeft(left));
        }
        Action::Wait => {}
    }
}

/// Where things are on the window, in the logical pixels the cursor uses.
#[derive(SystemParam)]
pub struct Screen<'w, 's> {
    rows: Query<
        'w,
        's,
        (
            &'static RowObject,
            &'static ComputedNode,
            &'static UiGlobalTransform,
        ),
    >,
    fields: Query<
        'w,
        's,
        (
            &'static FieldInput,
            &'static ComputedNode,
            &'static UiGlobalTransform,
        ),
    >,
    texts: Query<
        'w,
        's,
        (
            &'static Text,
            &'static ComputedNode,
            &'static UiGlobalTransform,
            &'static ChildOf,
        ),
    >,
    buttons: Query<'w, 's, (), With<Button>>,
    names: Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<NameInput>>,
    toggles: Query<
        'w,
        's,
        (
            &'static RowToggle,
            &'static ComputedNode,
            &'static UiGlobalTransform,
        ),
    >,
    layers: Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<LayerRow>>,
    layer_names:
        Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<LayerNameInput>>,
    rules: Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform), With<RuleRow>>,
    rule_fields: Query<
        'w,
        's,
        (
            &'static RuleField,
            &'static ComputedNode,
            &'static UiGlobalTransform,
        ),
    >,
    rule_layers: Query<
        'w,
        's,
        (
            &'static RuleLayerButton,
            &'static ComputedNode,
            &'static UiGlobalTransform,
        ),
    >,
    objects: Query<'w, 's, (&'static Projected, &'static GlobalTransform)>,
    camera: Single<'w, 's, (&'static Camera, &'static GlobalTransform), With<ViewCamera>>,
    gizmo: Res<'w, TransformGizmoSettings>,
}

fn centre(node: &ComputedNode, transform: &UiGlobalTransform) -> Vec2 {
    transform.translation * node.inverse_scale_factor
}

impl Screen<'_, '_> {
    /// The hierarchy row of the object with this stable ID.
    pub fn row(&self, id: &str) -> Option<Vec2> {
        self.rows
            .iter()
            .find(|(row, node, _)| row.0 == id && !node.is_empty())
            .map(|(_, node, transform)| centre(node, transform))
    }

    /// How many hierarchy rows are laid out, folded ones not counted.
    pub fn rows_shown(&self) -> usize {
        self.rows
            .iter()
            .filter(|(_, node, _)| !node.is_empty())
            .count()
    }

    /// The fold toggle of the hierarchy row of the object with this stable ID.
    pub fn toggle(&self, id: &str) -> Option<Vec2> {
        self.toggles
            .iter()
            .find(|(toggle, node, _)| toggle.0 == id && !node.is_empty())
            .map(|(_, node, transform)| centre(node, transform))
    }

    /// The number input of `field`'s component `axis`.
    pub fn field(&self, field: &str, axis: usize) -> Option<Vec2> {
        self.fields
            .iter()
            .find(|(input, ..)| input.field == field && input.axis == axis)
            .map(|(_, node, transform)| centre(node, transform))
    }

    /// The caption of a button that is laid out on the window. A panel's title with the same
    /// words is not a button and is not found.
    pub fn button(&self, caption: &str) -> Option<Vec2> {
        self.texts
            .iter()
            .find(|(text, node, _, parent)| {
                text.0 == caption && !node.is_empty() && self.buttons.contains(parent.parent())
            })
            .map(|(_, node, transform, _)| centre(node, transform))
    }

    /// The name field of the inspector.
    pub fn name_field(&self) -> Option<Vec2> {
        self.names
            .iter()
            .next()
            .filter(|(node, _)| !node.is_empty())
            .map(|(node, transform)| centre(node, transform))
    }

    /// The row of the landscape's layer with this number, counted from 1 down the list.
    pub fn layer(&self, number: usize) -> Option<Vec2> {
        let mut rows: Vec<Vec2> = self
            .layers
            .iter()
            .filter(|(node, _)| !node.is_empty())
            .map(|(node, transform)| centre(node, transform))
            .collect();
        rows.sort_by(|a, b| a.y.total_cmp(&b.y));
        rows.get(number.checked_sub(1)?).copied()
    }

    /// How many layer rows and rule rows of the landscape tool are laid out.
    pub fn layer_and_rule_rows(&self) -> (usize, usize) {
        (
            self.layers
                .iter()
                .filter(|(node, _)| !node.is_empty())
                .count(),
            self.rules
                .iter()
                .filter(|(node, _)| !node.is_empty())
                .count(),
        )
    }

    /// The row of the landscape's rule with this number, counted from 1 down the list.
    pub fn rule(&self, number: usize) -> Option<Vec2> {
        let mut rows: Vec<Vec2> = self
            .rules
            .iter()
            .filter(|(node, _)| !node.is_empty())
            .map(|(node, transform)| centre(node, transform))
            .collect();
        rows.sort_by(|a, b| a.y.total_cmp(&b.y));
        rows.get(number.checked_sub(1)?).copied()
    }

    /// The number field of the chosen rule's setting with this place in its list.
    pub fn rule_field(&self, setting: usize) -> Option<Vec2> {
        self.rule_fields
            .iter()
            .find(|(field, node, _)| field.setting == setting && !node.is_empty())
            .map(|(_, node, transform)| centre(node, transform))
    }

    /// The button that steps the chosen rule's layer in this place.
    pub fn rule_layer(&self, place: editor_ui::session::RulePlace) -> Option<Vec2> {
        self.rule_layers
            .iter()
            .find(|(button, node, _)| button.0 == place && !node.is_empty())
            .map(|(_, node, transform)| centre(node, transform))
    }

    /// The name field of the chosen layer.
    pub fn layer_name_field(&self) -> Option<Vec2> {
        self.layer_names
            .iter()
            .next()
            .filter(|(node, _)| !node.is_empty())
            .map(|(node, transform)| centre(node, transform))
    }

    /// The point of the ground plane at world `x`, `z` on the window.
    pub fn ground(&self, x: f32, z: f32) -> Option<Vec2> {
        self.project(Vec3::new(x, 0.0, z))
    }

    fn project(&self, world: Vec3) -> Option<Vec2> {
        let (camera, transform) = *self.camera;
        camera.world_to_viewport(transform, world).ok()
    }

    /// The middle of the object with this stable ID in the 3D view.
    pub fn object(&self, id: &str) -> Option<Vec2> {
        let (_, transform) = self.objects.iter().find(|(object, _)| object.0 .0 == id)?;
        self.project(transform.translation())
    }

    /// A point on the gizmo's X handle of the object with this stable ID, and the direction
    /// on the window in which that handle points.
    pub fn gizmo_x_handle(&self, id: &str) -> Option<(Vec2, Vec2)> {
        let (_, transform) = self.objects.iter().find(|(object, _)| object.0 .0 == id)?;
        let origin = transform.translation();
        let (_, camera) = *self.camera;
        let scale = (camera.translation() - origin).length() * self.gizmo.screen_scale_factor;
        let on_handle = self.project(origin + Vec3::X * scale * 0.6)?;
        let further = self.project(origin + Vec3::X * scale)?;
        Some((on_handle, (further - on_handle).normalize_or_zero()))
    }

    /// Two points on the gizmo's rotation ring about Y of the object with this stable ID,
    /// `degrees` apart and away from the other two rings.
    pub fn gizmo_y_ring(&self, id: &str, degrees: f32) -> Option<(Vec2, Vec2)> {
        let (_, transform) = self.objects.iter().find(|(object, _)| object.0 .0 == id)?;
        let origin = transform.translation();
        let (_, camera) = *self.camera;
        let radius = (camera.translation() - origin).length()
            * self.gizmo.screen_scale_factor
            * self.gizmo.rotate_ring_radius;
        let on_ring = |angle: f32| {
            self.project(origin + Quat::from_rotation_y(angle.to_radians()) * Vec3::X * radius)
        };
        // 45 degrees from the X axis: the rings about X and about Z cross the Y ring on the axes.
        Some((on_ring(45.0)?, on_ring(45.0 + degrees)?))
    }

    /// The middle of the 3D view.
    pub fn view_centre(&self) -> Option<Vec2> {
        self.camera
            .0
            .logical_viewport_rect()
            .map(|rect| rect.center())
    }
}
