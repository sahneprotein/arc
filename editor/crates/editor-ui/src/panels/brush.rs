//! The landscape tool's settings. Takes the inspector's place while a landscape tool is
//! chosen: the brush modes for sculpting, the layers for painting, and the brush both share.

use bevy::{
    feathers::{
        containers::{pane, pane_body, pane_header},
        controls::{ButtonVariant, FeathersButton, FeathersSlider},
        display::{label, label_dim},
        theme::ThemedText,
    },
    prelude::*,
    ui_widgets::{slider_self_update, Activate, SliderPrecision, SliderStep, ValueChange},
};

use crate::{
    panels::{paint::paint_part, rules::rules_part},
    theme::{size, spacing},
    tool::{Brush, BrushMode, Tool},
};

/// Which of the landscape tools a part of the panel belongs to. A part without this shows
/// with all of them.
#[derive(Component, Clone, Copy, PartialEq, Eq, FromTemplate)]
pub enum ToolPart {
    #[default]
    Sculpt,
    Paint,
    Rules,
    /// Sculpt and paint: the brush both share.
    Brush,
}

impl ToolPart {
    fn shows_with(self, tool: Tool) -> bool {
        match self {
            Self::Sculpt => tool == Tool::Sculpt,
            Self::Paint => tool == Tool::Paint,
            Self::Rules => tool == Tool::Rules,
            Self::Brush => tool.uses_brush(),
        }
    }
}

/// The landscape tool panel.
#[derive(Component, Default, Clone)]
pub struct BrushPanel;

/// The button of one brush mode.
#[derive(Component, Default, Clone)]
pub struct ModeButton(pub BrushMode);

/// A brush setting a slider sets.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Setting {
    #[default]
    Radius,
    Strength,
    Falloff,
    Amount,
}

/// The slider of one brush setting.
#[derive(Component, Default, Clone)]
pub struct BrushSlider(pub Setting);

fn mode_button(mode: BrushMode) -> impl Scene {
    let name = mode.name();
    bsn! {
        @FeathersButton { @caption: bsn! { Text(name) ThemedText } }
        ModeButton({mode})
        Node { flex_grow: 1.0, flex_basis: px(0) }
        on(|activate: On<Activate>, buttons: Query<&ModeButton>, mut brush: ResMut<Brush>| {
            if let Ok(button) = buttons.get(activate.entity) {
                brush.mode = button.0;
            }
        })
    }
}

fn slider_row(
    title: &'static str,
    setting: Setting,
    min: f32,
    max: f32,
    value: f32,
    step: f32,
) -> impl Scene {
    bsn! {
        Node {
            min_height: px(size::ROW_HEIGHT),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(spacing::SPACE_4),
            flex_shrink: 0.0,
        }
        Children [
            (Node { width: px(96), flex_shrink: 0.0 } Children [ label(title) ]),
            (
                @FeathersSlider { @min: {min}, @max: {max}, @value: {value} }
                BrushSlider({setting})
                SliderStep({step})
                SliderPrecision(2)
                Node { flex_grow: 1.0 }
                on(slider_self_update)
                on(|change: On<ValueChange<f32>>, sliders: Query<&BrushSlider>, mut brush: ResMut<Brush>| {
                    match sliders.get(change.source).map(|slider| slider.0) {
                        Ok(Setting::Radius) => brush.radius_m = change.value,
                        Ok(Setting::Strength) => brush.strength = change.value,
                        Ok(Setting::Falloff) => brush.falloff = change.value,
                        Ok(Setting::Amount) => brush.amount_m = change.value,
                        Err(_) => {}
                    }
                })
            ),
        ]
    }
}

pub fn brush_panel() -> impl Scene {
    let brush = Brush::default();
    let (radius_min, radius_max) = Brush::RADIUS_RANGE;
    let (amount_min, amount_max) = Brush::AMOUNT_RANGE;
    bsn! {
        pane()
        BrushPanel
        Node { width: px(size::INSPECTOR_WIDTH), flex_shrink: 0.0, display: Display::None }
        Children [
            (pane_header() Children [ (Text("Landscape tool") ThemedText) ]),
            (
                pane_body()
                Node { flex_grow: 1.0, min_height: px(0), overflow: Overflow::clip() }
                Children [
                    (paint_part() ToolPart::Paint),
                    (rules_part() ToolPart::Rules),
                    (label_dim("Brush") ToolPart::Brush),
                    (
                        ToolPart::Sculpt
                        Node {
                            flex_direction: FlexDirection::Row,
                            column_gap: px(spacing::SPACE_2),
                            flex_shrink: 0.0,
                        }
                        Children [
                            mode_button(BrushMode::Raise),
                            mode_button(BrushMode::Lower),
                            mode_button(BrushMode::Smooth),
                            mode_button(BrushMode::Flatten),
                        ]
                    ),
                    (slider_row("Radius (m)", Setting::Radius, radius_min, radius_max, brush.radius_m, 0.5) ToolPart::Brush),
                    (slider_row("Strength", Setting::Strength, 0.0, 1.0, brush.strength, 0.05) ToolPart::Brush),
                    (slider_row("Falloff", Setting::Falloff, 0.0, 1.0, brush.falloff, 0.05) ToolPart::Brush),
                    (slider_row("Height (m)", Setting::Amount, amount_min, amount_max, brush.amount_m, 0.1) ToolPart::Sculpt),
                    (
                        label_dim("Drag on the landscape to sculpt. Height is how far Raise and Lower move the ground at full strength. Flatten levels to where the stroke begins. Escape cancels a stroke.")
                        ToolPart::Sculpt
                    ),
                    (
                        label_dim("Drag on the landscape to paint the chosen layer. Strength is how much of that layer the brush leaves at its centre; the other layers give way. Hold Shift as you begin to erase instead. Escape cancels a stroke.")
                        ToolPart::Paint
                    ),
                ]
            ),
        ]
    }
}

/// Shows the parts of the panel that belong to the chosen landscape tool.
pub(crate) fn show_landscape_parts(tool: Res<Tool>, mut parts: Query<(&ToolPart, &mut Node)>) {
    if !tool.is_changed() {
        return;
    }
    // Before a landscape tool is chosen the panel is hidden; it is laid out for sculpting.
    let tool = if tool.is_landscape() {
        *tool
    } else {
        Tool::Sculpt
    };
    for (part, mut node) in &mut parts {
        let wanted = if part.shows_with(tool) {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != wanted {
            node.display = wanted;
        }
    }
}

/// Shows which mode is chosen: its button is the primary one.
pub(crate) fn show_brush_mode(
    brush: Res<Brush>,
    mut buttons: Query<(&ModeButton, &mut ButtonVariant)>,
) {
    if !brush.is_changed() {
        return;
    }
    for (button, mut variant) in &mut buttons {
        let wanted = if button.0 == brush.mode {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Normal
        };
        if *variant != wanted {
            *variant = wanted;
        }
    }
}
