//! The chosen emitter's settings, in the inspector's place while the Particles tool is
//! chosen, and the preview's controls over the 3D view.
//!
//! The settings are the fields of the particle format the game and the editor share, in its
//! order: emission, lifetime, shape, motion, look and limits. A number is typed and takes
//! effect when the field is left or Enter is pressed. What the format refuses changes
//! nothing; the panel says which field it was and what it may be, and the field shows the
//! emitter's value again. Every change is one step of the emitter's own history.
//!
//! At the panel's foot the emitter is saved, and the foot says whether it has changes that
//! are not saved. Nothing is written before that.
//!
//! A file that cannot be read has no settings: the panel says why, what to do, and where the
//! file is.
//!
//! Over the 3D view are the preview's controls: which picture is asked for, pause and
//! resume, begin again, and three speeds. There are two pictures. The reference simulation
//! is worked out by the editor on the processor; the GPU preview is the game's particle
//! runtime drawing the emitter on the graphics card. The overlay is headed by what is drawn
//! and never by what was asked for: "GPU preview" only while the application says the
//! graphics card draws the effect, and "Reference simulation" otherwise, with a line that
//! says why the GPU preview is not shown.

use bevy::{
    feathers::{
        containers::{pane, pane_body, pane_header},
        controls::{
            ButtonVariant, FeathersButton, FeathersNumberInput, FeathersTextInput,
            FeathersTextInputContainer, NumberInputValue, UpdateNumberInput,
        },
        display::{label, label_dim},
        theme::{ThemeBackgroundColor, ThemeBorderColor, ThemeTextColor, ThemeToken, ThemedText},
        tokens,
    },
    input_focus::InputFocus,
    prelude::*,
    text::{EditableText, TextEdit},
    ui::InteractionDisabled,
    ui_widgets::{Activate, ScrollArea, ValueChange},
    window::RequestRedraw,
};

use crate::{
    panels::inspector::{group_title, shown_number},
    particles::{
        max_live_estimate, number, number_bound, Blend, EmitterChange, EmitterSpec, GpuPreview,
        NumberField, Particles, Preview, PreviewMode, PreviewSpeed, ShapeKind, NUMBER_FIELDS,
    },
    shell::PanelChoice,
    theme::{
        feathers::{BORDER, SIGNAL_WARN},
        radius, size, spacing,
    },
    tool::Tool,
    wording,
};

/// The emitter's settings panel.
#[derive(Component, Default, Clone)]
pub struct EmitterPanel;

/// The node the settings are rebuilt in.
#[derive(Component, Default, Clone)]
pub(crate) struct EmitterBody;

/// The text field of the emitter's name, and the name it was last given to show.
#[derive(Component, Default, Clone)]
pub struct EmitterNameInput(pub String);

/// One number input: the field by the format's dotted name, its place in a vector, and the
/// value the input shows.
#[derive(Component, Default, Clone)]
pub struct EmitterField {
    pub field: String,
    pub component: usize,
    pub value: f32,
}

/// One of a few things an emitter is, each a button of a row of buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EmitterChoice {
    /// The kind of shape, by its place in [`ShapeKind::ALL`].
    Shape(usize),
    /// How the particles are laid over what is behind them: added to it, or over it.
    Additive(bool),
    /// Whether the emitter begins again when its duration is over.
    Looping(bool),
}

impl Default for EmitterChoice {
    fn default() -> Self {
        Self::Shape(0)
    }
}

impl EmitterChoice {
    pub fn name(self) -> &'static str {
        match self {
            Self::Shape(kind) => match ShapeKind::ALL[kind % ShapeKind::ALL.len()] {
                ShapeKind::Point => "Point",
                ShapeKind::Sphere => "Sphere",
                ShapeKind::Cone => "Cone",
                ShapeKind::Box => "Box",
            },
            Self::Additive(true) => "Additive",
            Self::Additive(false) => "Alpha",
            Self::Looping(true) => "Repeats",
            Self::Looping(false) => "Once",
        }
    }

    /// The change that makes the emitter this.
    pub fn change(self) -> EmitterChange {
        match self {
            Self::Shape(kind) => EmitterChange::Shape(ShapeKind::ALL[kind % ShapeKind::ALL.len()]),
            Self::Additive(true) => EmitterChange::Blend(Blend::Additive),
            Self::Additive(false) => EmitterChange::Blend(Blend::Alpha),
            Self::Looping(looping) => EmitterChange::Looping(looping),
        }
    }

    /// Whether the emitter is this.
    pub fn holds(self, emitter: &EmitterSpec) -> bool {
        match self {
            Self::Shape(kind) => {
                ShapeKind::of(&emitter.shape) == ShapeKind::ALL[kind % ShapeKind::ALL.len()]
            }
            Self::Additive(additive) => (emitter.look.blend == Blend::Additive) == additive,
            Self::Looping(looping) => emitter.emission.looping == looping,
        }
    }
}

/// The button of one choice.
#[derive(Component, Default, Clone)]
pub struct ChoiceButton(pub EmitterChoice);

/// The line that says how many particles the settings let be alive at once.
#[derive(Component, Default, Clone)]
pub struct EmitterEstimate;

/// The button that saves the emitter, and the line beside it that says whether there is
/// anything to save.
#[derive(Component, Default, Clone)]
pub struct SaveEmitterButton;

#[derive(Component, Default, Clone)]
pub struct EmitterSavedLine;

/// What the panel says when no emitter is chosen.
pub const NOTHING_CHOSEN: &str = "Choose an emitter in the list, or make a new one.";

/// The groups of settings, by the first part of a field's dotted name, in the format's order.
const GROUPS: [(&str, &str); 6] = [
    ("emission", "Emission"),
    ("lifetime", "Lifetime"),
    ("shape", "Shape"),
    ("motion", "Motion"),
    ("look", "Look"),
    ("limits", "Limits"),
];

/// The width a setting's name takes, so that the fields line up.
const LABEL_WIDTH: f32 = 128.0;

/// The longest name the name field takes: what the format lets a name be.
const NAME_LENGTH: usize = 96;

/// The line under the limit: how many particles these settings let be alive at once.
pub fn estimate_words(emitter: &EmitterSpec) -> String {
    let most = max_live_estimate(emitter);
    let particles = if most == 1 {
        "particle is"
    } else {
        "particles are"
    };
    format!("With these settings at most {most} {particles} alive at once.")
}

/// What the panel's foot says of the chosen emitter.
pub fn saved_words(particles: &Particles) -> &'static str {
    if particles.chosen().is_none() {
        ""
    } else if particles.chosen_unsaved() {
        "This emitter has changes that are not saved."
    } else {
        "Saved."
    }
}

fn save(_: On<Activate>, mut particles: ResMut<Particles>) {
    particles.save_chosen();
}

pub fn emitter_panel() -> impl Scene {
    bsn! {
        pane()
        EmitterPanel
        Node { width: px(size::INSPECTOR_WIDTH), flex_shrink: 0.0, display: Display::None }
        Children [
            (pane_header() Children [ (Text("Emitter") ThemedText) ]),
            (
                pane_body()
                EmitterBody
                ScrollArea
                Node { flex_grow: 1.0, min_height: px(0), overflow: Overflow::scroll_y() }
            ),
            (
                pane_body()
                Node { flex_shrink: 0.0, border: UiRect::top(px(1)) }
                ThemeBorderColor(BORDER)
                Children [
                    (label_dim("") EmitterSavedLine),
                    (
                        @FeathersButton {
                            @caption: bsn! { Text("Save emitter") ThemedText },
                            @variant: ButtonVariant::Primary,
                        }
                        SaveEmitterButton
                        Node { align_self: AlignSelf::FlexStart }
                        on(save)
                    ),
                ]
            ),
        ]
    }
}

fn row(children: impl SceneList) -> impl Scene {
    bsn! {
        Node {
            min_height: px(size::ROW_HEIGHT),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            column_gap: px(spacing::SPACE_4),
            flex_shrink: 0.0,
        }
        Children [ {children} ]
    }
}

fn setting_label(text: &str) -> impl Scene {
    bsn! {
        Node { width: px(LABEL_WIDTH), flex_shrink: 0.0 }
        Children [ label(text.to_owned()) ]
    }
}

/// The letter and the stripe of one number of a vector: none for a number that stands alone,
/// X, Y and Z for three, and red, green, blue and alpha for a colour's four.
fn sigil(components: usize, component: usize) -> (Option<&'static str>, ThemeToken) {
    const AXES: [(&str, ThemeToken); 3] = [
        ("X", tokens::TEXT_INPUT_X_AXIS),
        ("Y", tokens::TEXT_INPUT_Y_AXIS),
        ("Z", tokens::TEXT_INPUT_Z_AXIS),
    ];
    match (components, component) {
        (1, _) => (None, tokens::TEXT_INPUT_BG),
        (4, 3) => (Some("A"), tokens::TEXT_DIM),
        (4, component) => (
            Some(["R", "G", "B"][component % 3]),
            AXES[component % 3].1.clone(),
        ),
        (_, component) => {
            let (letter, color) = AXES[component % 3].clone();
            (Some(letter), color)
        }
    }
}

fn number_input(field: &NumberField, component: usize, value: f32) -> impl Scene + use<> {
    let (letter, color) = sigil(field.components, component);
    let name = field.field.to_owned();
    bsn! {
        @FeathersNumberInput { @sigil_color: {color}, @label_text: {letter} }
        EmitterField { field: {name}, component: {component}, value: {value} }
        Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0) }
        on(commit_number)
    }
}

/// The row of one field of the emitter. A colour's four numbers take a line of their own
/// under the field's name: beside it they would be too narrow to read.
fn number_rows(emitter: &EmitterSpec, field: &NumberField) -> Box<dyn SceneList> {
    let title = wording::emitter_field(field.field);
    let inputs: Vec<Box<dyn SceneList>> = (0..field.components)
        .filter_map(|component| {
            let value = shown_number(number(emitter, field.field, component)? as f32);
            Some(Box::new(bsn_list![number_input(field, component, value)]) as Box<dyn SceneList>)
        })
        .collect();
    if field.components > 3 {
        Box::new(bsn_list![
            row(bsn_list![label(title.to_owned())]),
            row(bsn_list![{ inputs }]),
        ])
    } else {
        Box::new(bsn_list![row(bsn_list![setting_label(title), { inputs }])])
    }
}

fn choose(activate: On<Activate>, buttons: Query<&ChoiceButton>, mut particles: ResMut<Particles>) {
    if let Ok(button) = buttons.get(activate.entity) {
        particles.apply(&button.0.change());
    }
}

fn choice_button(choice: EmitterChoice, emitter: &EmitterSpec) -> impl Scene + use<> {
    let caption = choice.name();
    let variant = if choice.holds(emitter) {
        ButtonVariant::Primary
    } else {
        ButtonVariant::Normal
    };
    bsn! {
        @FeathersButton {
            @caption: bsn! { Text(caption) ThemedText },
            @variant: {variant},
        }
        ChoiceButton({choice})
        Node { flex_grow: 1.0 }
        on(choose)
    }
}

/// A row of buttons of which one holds, under a name.
fn choice_row(title: &str, choices: &[EmitterChoice], emitter: &EmitterSpec) -> Box<dyn SceneList> {
    let buttons: Vec<Box<dyn SceneList>> = choices
        .iter()
        .map(|choice| Box::new(bsn_list![choice_button(*choice, emitter)]) as Box<dyn SceneList>)
        .collect();
    if choices.len() > 2 {
        Box::new(bsn_list![row(bsn_list![{ buttons }])])
    } else {
        Box::new(bsn_list![row(bsn_list![setting_label(title), { buttons }])])
    }
}

/// A refusal as the panel shows it: what it is and what to do, edged in the warning colour,
/// which always comes with these words.
fn refusal_block(words: &wording::Problem, more: Option<String>) -> Box<dyn SceneList> {
    let title = words.title.clone();
    let remedy = words.remedy.clone();
    let more: Box<dyn SceneList> = match more {
        Some(more) => Box::new(bsn_list![label_dim(more)]),
        None => Box::new(bsn_list![]),
    };
    Box::new(bsn_list![(
        EmitterRefusal
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(spacing::SPACE_1),
            padding: UiRect::left(px(spacing::SPACE_4)),
            border: UiRect::left(px(2)),
            flex_shrink: 0.0,
        }
        ThemeBorderColor(SIGNAL_WARN)
        Children [ label(title), label_dim(remedy), {more} ]
    )])
}

/// The block that says what was refused.
#[derive(Component, Default, Clone)]
pub struct EmitterRefusal;

/// What was last refused of the chosen emitter, in words: with the range the field at fault
/// may be in, where the format bounds it.
pub fn problem_words(particles: &Particles) -> Option<wording::Problem> {
    let refusal = particles.problem()?;
    let parameter = |key: &str| refusal.parameters.get(key).map(String::as_str);
    // Of a vector's numbers the first's bound is given: a colour's alpha has another, and
    // the format's own detail says which number it was.
    let range = parameter("field").and_then(|field| number_bound(field, 0));
    Some(wording::emitter(&refusal.code, parameter, range))
}

/// Why the chosen file cannot be read, in words, and where the file is.
pub fn refusal_words(particles: &Particles) -> Option<(wording::Problem, String)> {
    let (refusal, path) = particles.chosen_refusal()?;
    let parameter = |key: &str| refusal.parameters.get(key).map(String::as_str);
    let mut file = format!("File: {}", path.display());
    if let Some(detail) = parameter("detail").filter(|detail| !detail.is_empty()) {
        file = format!("{file}\nDetails: {detail}");
    }
    Some((wording::emitter(&refusal.code, parameter, None), file))
}

fn settings(emitter: &EmitterSpec) -> Vec<Box<dyn SceneList>> {
    let mut rows: Vec<Box<dyn SceneList>> = Vec::new();
    let name = emitter.name.clone();
    rows.push(Box::new(bsn_list![row(bsn_list![
        setting_label(wording::emitter_field("name")),
        (
            @FeathersTextInputContainer
            Node { flex_grow: 1.0, min_width: px(0), border: UiRect::all(px(1)) }
            ThemeBorderColor(BORDER)
            Children [(
                @FeathersTextInput { @max_characters: NAME_LENGTH }
                EmitterNameInput({name})
            )]
        )
    ])]));
    for (prefix, title) in GROUPS {
        rows.push(Box::new(bsn_list![group_title(title)]));
        if prefix == "shape" {
            let kinds: Vec<_> = (0..ShapeKind::ALL.len())
                .map(EmitterChoice::Shape)
                .collect();
            rows.push(choice_row("", &kinds, emitter));
        }
        for field in NUMBER_FIELDS {
            let of_group = field.field.split('.').next() == Some(prefix);
            // A shape has only the numbers of its kind.
            if of_group && number(emitter, field.field, 0).is_some() {
                rows.push(number_rows(emitter, field));
            }
        }
        match prefix {
            "emission" => rows.push(choice_row(
                "When it ends",
                &[EmitterChoice::Looping(true), EmitterChoice::Looping(false)],
                emitter,
            )),
            "look" => rows.push(choice_row(
                "Blend",
                &[
                    EmitterChoice::Additive(false),
                    EmitterChoice::Additive(true),
                ],
                emitter,
            )),
            "limits" => {
                let words = estimate_words(emitter);
                rows.push(Box::new(bsn_list![(label_dim(words) EmitterEstimate)]));
            }
            _ => {}
        }
    }
    rows
}

/// What decides whether the panel is rebuilt or only its values refreshed: the emitter, the
/// rows it has, and what the panel says beside the settings.
#[derive(Clone, PartialEq, Default)]
pub struct PanelShape {
    has_game: bool,
    chosen: Option<String>,
    /// The kind of shape, which decides which numbers there are, by its name.
    shape: Option<&'static str>,
    /// What was refused, as the panel words it.
    problem: Option<wording::Problem>,
    /// Why the file cannot be read, and where it is.
    refusal: Option<(wording::Problem, String)>,
}

fn panel_shape(particles: &Particles) -> PanelShape {
    PanelShape {
        has_game: particles.has_game(),
        chosen: particles.chosen_id().map(str::to_owned),
        shape: particles.chosen().map(|emitter| {
            EmitterChoice::Shape(
                ShapeKind::ALL
                    .iter()
                    .position(|kind| *kind == ShapeKind::of(&emitter.shape))
                    .unwrap_or_default(),
            )
            .name()
        }),
        problem: problem_words(particles),
        refusal: refusal_words(particles),
    }
}

/// Shows the chosen emitter's settings. Rebuilds when another emitter is chosen, when its
/// kind of shape changed, and when what was refused did; otherwise only sends new values to
/// the inputs, so that an input being used is not replaced.
pub(crate) fn show_emitter(
    mut commands: Commands,
    particles: Res<Particles>,
    body: Query<Entity, With<EmitterBody>>,
    mut inputs: Query<(Entity, &mut EmitterField)>,
    mut names: Query<&mut EmitterNameInput>,
    (mut choices, mut estimates): (
        Query<(&ChoiceButton, &mut ButtonVariant)>,
        Query<&mut Text, With<EmitterEstimate>>,
    ),
    mut shown: Local<Option<PanelShape>>,
) {
    let Ok(body) = body.single() else {
        return;
    };
    let now = panel_shape(&particles);
    if shown.as_ref() != Some(&now) {
        let mut scenes: Vec<Box<dyn SceneList>> = Vec::new();
        if let Some(words) = &now.problem {
            scenes.push(refusal_block(words, None));
        }
        match (particles.chosen(), &now.refusal) {
            (Some(emitter), _) => scenes.extend(settings(emitter)),
            (None, Some((words, file))) => scenes.push(refusal_block(words, Some(file.clone()))),
            (None, None) if !now.has_game => {
                scenes.push(Box::new(bsn_list![label_dim(
                    crate::panels::emitters::NO_GAME
                )]));
            }
            (None, None) => scenes.push(Box::new(bsn_list![label_dim(NOTHING_CHOSEN)])),
        }
        commands
            .entity(body)
            .despawn_children()
            .queue_spawn_related_scenes::<Children>(scenes);
        *shown = Some(now);
        return;
    }
    let Some(emitter) = particles.chosen() else {
        return;
    };
    for (entity, mut input) in &mut inputs {
        let Some(value) = number(emitter, &input.field, input.component) else {
            continue;
        };
        let value = shown_number(value as f32);
        // Not equal also when the input was told to show its value again (`commit_number`).
        if value != input.value {
            input.value = value;
            commands.trigger(UpdateNumberInput {
                entity,
                value: NumberInputValue::F32(value),
            });
        }
    }
    // The name changed without the field, by undo for instance: show it again.
    for mut input in &mut names {
        if input.0 != emitter.name {
            input.0.clone_from(&emitter.name);
        }
    }
    for (button, mut variant) in &mut choices {
        let wanted = if button.0.holds(emitter) {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Normal
        };
        if *variant != wanted {
            *variant = wanted;
        }
    }
    let words = estimate_words(emitter);
    for mut text in &mut estimates {
        if text.0 != words {
            text.0.clone_from(&words);
        }
    }
}

/// Gives a new number input its value.
pub(crate) fn fill_emitter_fields(
    mut commands: Commands,
    inputs: Query<(Entity, &EmitterField), Added<EmitterField>>,
) {
    for (entity, input) in &inputs {
        commands.trigger(UpdateNumberInput {
            entity,
            value: NumberInputValue::F32(input.value),
        });
    }
}

/// Makes a committed number the emitter's. A number the format refuses changes nothing: the
/// panel says why, and the input is told to show the emitter's value again.
fn commit_number(
    change: On<ValueChange<f32>>,
    mut inputs: Query<&mut EmitterField>,
    mut particles: ResMut<Particles>,
) {
    if !change.is_final {
        return;
    }
    let Ok(mut input) = inputs.get_mut(change.source) else {
        return;
    };
    if input.value == change.value {
        return;
    }
    let applied = particles.apply(&EmitterChange::Number {
        field: input.field.clone(),
        component: input.component,
        value: f64::from(change.value),
    });
    // Whatever the emitter has now is not what the input shows: a value that is no number
    // equals none, so the next look at the emitter writes its value into the input.
    if !applied {
        input.value = f32::NAN;
    }
}

/// Writes the name into its field when the field is new or the name changed elsewhere, unless
/// the field is being typed in.
pub(crate) fn fill_emitter_names(
    mut names: Query<(Entity, &EmitterNameInput, &mut EditableText), Changed<EmitterNameInput>>,
    focus: Res<InputFocus>,
) {
    for (entity, name, mut text) in &mut names {
        if focus.get() == Some(entity) || text.value().to_string() == name.0 {
            continue;
        }
        text.queue_edit(TextEdit::SelectAll);
        text.queue_edit(TextEdit::Insert(name.0.as_str().into()));
    }
}

/// Enter in the name field renames the emitter. An unchanged name does nothing; a name the
/// format refuses, an empty one for instance, is said like any other refusal.
pub(crate) fn commit_emitter_name(
    keys: Res<ButtonInput<KeyCode>>,
    focus: Res<InputFocus>,
    mut names: Query<(&mut EmitterNameInput, &EditableText)>,
    mut particles: ResMut<Particles>,
) {
    if !keys.just_pressed(KeyCode::Enter) && !keys.just_pressed(KeyCode::NumpadEnter) {
        return;
    }
    let Some(Ok((mut shown, text))) = focus.get().map(|entity| names.get_mut(entity)) else {
        return;
    };
    let typed = text.value().to_string();
    let typed = typed.trim();
    if typed == shown.0 {
        return;
    }
    if particles.apply(&EmitterChange::Name(typed.to_owned())) {
        shown.0 = typed.to_owned();
    }
}

/// Shows the panel in the right-hand place while the Particles tool is chosen and that place
/// is shown, and what its foot says: whether there is anything to save.
pub(crate) fn fit_emitter(
    mut commands: Commands,
    tool: Res<Tool>,
    choice: Res<PanelChoice>,
    particles: Res<Particles>,
    mut panels: Query<&mut Node, With<EmitterPanel>>,
    mut lines: Query<&mut Text, With<EmitterSavedLine>>,
    buttons: Query<(Entity, Has<InteractionDisabled>), With<SaveEmitterButton>>,
) {
    let wanted = if choice.shown().1 && *tool == Tool::Particles {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut panels {
        if node.display != wanted {
            node.display = wanted;
        }
    }
    let words = saved_words(&particles);
    for mut text in &mut lines {
        if text.0 != words {
            words.clone_into(&mut text.0);
        }
    }
    let offered = particles.chosen_unsaved();
    for (entity, off) in &buttons {
        if offered && off {
            commands.entity(entity).remove::<InteractionDisabled>();
        } else if !offered && !off {
            commands.entity(entity).insert(InteractionDisabled);
        }
    }
}

/// The preview's controls and what it says, over the 3D view.
#[derive(Component, Default, Clone)]
pub struct PreviewOverlay;

/// The caption of the button that pauses and resumes.
#[derive(Component, Default, Clone)]
pub struct PreviewRunCaption;

/// The button of one speed, by its place in [`PreviewSpeed::ALL`].
#[derive(Component, Default, Clone)]
pub struct PreviewSpeedButton(pub usize);

/// The line that says how many particles are alive and what time the preview has come to.
#[derive(Component, Default, Clone)]
pub struct PreviewWords;

/// The overlay's text marked `T`, which is not the one marked `Other` and no heading or
/// note.
type PreviewText<T, Other> = (With<T>, Without<Other>, Without<PreviewLine>);

/// What the preview is headed, and what that means.
pub const REFERENCE_TITLE: &str = "Reference simulation";
pub const REFERENCE_NOTE: &str = "The editor works this out from the emitter's settings, on \
    the processor. It is not the game's particle runtime, and the game may draw the emitter \
    differently.";

/// What the preview is headed while the game's particle runtime draws it, and what that
/// means.
pub const GPU_TITLE: &str = "GPU preview";
pub const GPU_NOTE: &str = "The game's particle runtime draws this on the graphics card, \
    from the emitter's settings as the game builds an effect from them. It does not report \
    how many particles are alive.";

/// What the overlay is headed: what is drawn, whatever was asked for.
pub fn preview_title(mode: PreviewMode, gpu: &GpuPreview) -> &'static str {
    if mode == PreviewMode::Gpu && gpu.running() {
        GPU_TITLE
    } else {
        REFERENCE_TITLE
    }
}

/// The line under the heading: what the picture that is drawn is.
pub fn preview_note(mode: PreviewMode, gpu: &GpuPreview) -> &'static str {
    if mode == PreviewMode::Gpu && gpu.running() {
        GPU_NOTE
    } else {
        REFERENCE_NOTE
    }
}

/// Why the GPU preview was asked for and is not what is drawn, in words. Nothing while it
/// is drawn, or was not asked for.
pub fn gpu_reason(mode: PreviewMode, gpu: &GpuPreview) -> Option<String> {
    if mode != PreviewMode::Gpu {
        return None;
    }
    let shown = "The reference simulation is shown.";
    match gpu {
        GpuPreview::Running => None,
        GpuPreview::Off => Some(format!("The GPU preview has no emitter to draw. {shown}")),
        GpuPreview::Starting => Some(format!("The GPU preview is starting. {shown}")),
        GpuPreview::Unavailable(lacks) => Some(format!(
            "The GPU preview cannot run on this graphics device: {lacks}. {shown}"
        )),
        GpuPreview::Unsupported { field, .. } => {
            let label = wording::emitter_field(field);
            let setting = label.split(" (").next().unwrap_or(label).to_lowercase();
            Some(format!(
                "The game's particle runtime does not draw an emitter with this {setting} \
                 yet, so the game cannot show this emitter as it is. {shown}"
            ))
        }
        GpuPreview::Failed(why) => Some(format!("The GPU preview did not start: {why}. {shown}")),
    }
}

/// The button of one picture of the emitter: the GPU preview, or the reference simulation.
#[derive(Component, Default, Clone)]
pub struct PreviewModeButton {
    pub gpu: bool,
}

/// The overlay's heading, the line under it, and the line that says why the GPU preview is
/// not shown.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug, FromTemplate)]
pub enum PreviewLine {
    #[default]
    Title,
    Note,
    GpuReason,
}

fn mode_button(gpu: bool) -> impl Scene {
    let caption = if gpu { "GPU preview" } else { "Reference" };
    bsn! {
        @FeathersButton { @caption: bsn! { Text(caption) ThemedText } }
        PreviewModeButton { gpu: {gpu} }
        on(|activate: On<Activate>, buttons: Query<&PreviewModeButton>, mut preview: ResMut<Preview>| {
            if let Ok(button) = buttons.get(activate.entity) {
                let mode = if button.gpu { PreviewMode::Gpu } else { PreviewMode::Reference };
                if preview.mode != mode {
                    preview.mode = mode;
                }
            }
        })
    }
}

/// What the button that pauses and resumes says.
pub fn run_caption(running: bool) -> &'static str {
    if running {
        "Pause"
    } else {
        "Resume"
    }
}

fn speed_button(speed: usize) -> impl Scene {
    let caption = PreviewSpeed::ALL[speed % PreviewSpeed::ALL.len()].name();
    bsn! {
        @FeathersButton { @caption: bsn! { Text(caption) ThemedText } }
        PreviewSpeedButton({speed})
        on(|activate: On<Activate>, buttons: Query<&PreviewSpeedButton>, mut preview: ResMut<Preview>| {
            if let Ok(button) = buttons.get(activate.entity) {
                preview.speed = PreviewSpeed::ALL[button.0 % PreviewSpeed::ALL.len()];
            }
        })
    }
}

/// The overlay, for the node the 3D view is in: in its upper left corner, where the view's
/// own control is while the level is shown.
pub fn preview_overlay() -> impl Scene {
    bsn! {
        PreviewOverlay
        Node {
            position_type: PositionType::Absolute,
            left: px(spacing::SPACE_4),
            top: px(spacing::SPACE_4),
            max_width: px(size::INSPECTOR_WIDTH),
            flex_direction: FlexDirection::Column,
            row_gap: px(spacing::SPACE_2),
            padding: UiRect::all(px(spacing::SPACE_4)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(radius::RADIUS_FIELD)),
            display: Display::None,
        }
        ThemeBackgroundColor(tokens::PANE_BODY_BG)
        ThemeBorderColor(BORDER)
        Children [
            (label(REFERENCE_TITLE) PreviewLine::Title),
            (label_dim(REFERENCE_NOTE) PreviewLine::Note),
            (
                label("")
                ThemeTextColor(SIGNAL_WARN)
                PreviewLine::GpuReason
                Node { display: Display::None }
            ),
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    align_items: AlignItems::Center,
                    column_gap: px(spacing::SPACE_2),
                    row_gap: px(spacing::SPACE_2),
                }
                Children [ mode_button(false), mode_button(true) ]
            ),
            (
                Node {
                    flex_direction: FlexDirection::Row,
                    flex_wrap: FlexWrap::Wrap,
                    align_items: AlignItems::Center,
                    column_gap: px(spacing::SPACE_2),
                    row_gap: px(spacing::SPACE_2),
                }
                Children [
                    (
                        @FeathersButton { @caption: bsn! { Text("Pause") ThemedText PreviewRunCaption } }
                        on(|_: On<Activate>, mut preview: ResMut<Preview>| {
                            preview.running = !preview.running;
                        })
                    ),
                    (
                        @FeathersButton { @caption: bsn! { Text("Restart") ThemedText } }
                        on(|_: On<Activate>, mut preview: ResMut<Preview>| preview.restart())
                    ),
                    speed_button(0),
                    speed_button(1),
                    speed_button(2),
                ]
            ),
            (label_dim("") PreviewWords),
        ]
    }
}

/// What the overlay's last line says: of the preview, or why there is none. Of the GPU
/// preview it says the most the settings let be alive, which is all that is known: the
/// graphics card does not say how many are.
pub fn preview_words(particles: &Particles, preview: &Preview, gpu: &GpuPreview) -> String {
    let Some(emitter) = particles.chosen() else {
        return if particles.has_game() {
            "No emitter is chosen.".into()
        } else {
            "No game, so no emitter to show.".into()
        };
    };
    if preview.mode == PreviewMode::Gpu && gpu.running() {
        let state = if preview.running { "" } else { " Paused." };
        format!(
            "At most {} particles alive, by the settings.{state}",
            max_live_estimate(emitter)
        )
    } else {
        preview.words()
    }
}

/// Keeps the preview on the chosen emitter and lets its time pass while the Particles tool
/// is chosen, and asks for the next frame while particles move: the editor otherwise draws
/// only when something happens. Entering the tool reads the game's folder again, so that
/// what changed there since is seen. While the graphics card draws the GPU preview the
/// reference simulation stands still: it is not what is shown.
pub(crate) fn run_preview(
    tool: Res<Tool>,
    time: Res<Time<Real>>,
    gpu: Res<GpuPreview>,
    mut particles: ResMut<Particles>,
    mut preview: ResMut<Preview>,
    mut redraw: MessageWriter<RequestRedraw>,
) {
    if *tool != Tool::Particles {
        return;
    }
    if tool.is_changed() {
        particles.refresh();
    }
    let chosen = particles.chosen();
    if preview.simulation().map(|simulation| simulation.spec()) != chosen {
        preview.show(chosen);
    }
    let reference_shown = !(preview.mode == PreviewMode::Gpu && gpu.running());
    if reference_shown && preview.moving() {
        preview.tick(time.delta_secs_f64());
        redraw.write(RequestRedraw);
    }
}

/// Shows the overlay while the Particles tool is chosen, and what its controls say.
pub(crate) fn show_preview(
    tool: Res<Tool>,
    (particles, preview, gpu): (Res<Particles>, Res<Preview>, Res<GpuPreview>),
    mut overlays: Query<&mut Node, (With<PreviewOverlay>, Without<PreviewLine>)>,
    mut captions: Query<&mut Text, PreviewText<PreviewRunCaption, PreviewWords>>,
    mut buttons: Query<
        (
            Option<&PreviewSpeedButton>,
            Option<&PreviewModeButton>,
            &mut ButtonVariant,
        ),
        Or<(With<PreviewSpeedButton>, With<PreviewModeButton>)>,
    >,
    mut words: Query<&mut Text, PreviewText<PreviewWords, PreviewRunCaption>>,
    mut lines: Query<(&PreviewLine, &mut Text, &mut Node), Without<PreviewOverlay>>,
) {
    let wanted = if *tool == Tool::Particles {
        Display::Flex
    } else {
        Display::None
    };
    for mut node in &mut overlays {
        if node.display != wanted {
            node.display = wanted;
        }
    }
    if *tool != Tool::Particles {
        return;
    }
    let caption = run_caption(preview.running);
    for mut text in &mut captions {
        if text.0 != caption {
            caption.clone_into(&mut text.0);
        }
    }
    for (speed, mode, mut variant) in &mut buttons {
        let chosen = match (speed, mode) {
            (Some(speed), _) => {
                PreviewSpeed::ALL[speed.0 % PreviewSpeed::ALL.len()] == preview.speed
            }
            (None, Some(mode)) => mode.gpu == (preview.mode == PreviewMode::Gpu),
            (None, None) => false,
        };
        let wanted = if chosen {
            ButtonVariant::Primary
        } else {
            ButtonVariant::Normal
        };
        if *variant != wanted {
            *variant = wanted;
        }
    }
    let said = preview_words(&particles, &preview, &gpu);
    for mut text in &mut words {
        if text.0 != said {
            text.0.clone_from(&said);
        }
    }
    let reason = gpu_reason(preview.mode, &gpu);
    for (line, mut text, mut node) in &mut lines {
        let (said, shown) = match line {
            PreviewLine::Title => (preview_title(preview.mode, &gpu), true),
            PreviewLine::Note => (preview_note(preview.mode, &gpu), true),
            PreviewLine::GpuReason => (reason.as_deref().unwrap_or_default(), reason.is_some()),
        };
        if text.0 != said {
            said.clone_into(&mut text.0);
        }
        let display = if shown { Display::Flex } else { Display::None };
        if node.display != display {
            node.display = display;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_one_emitter(game: &std::path::Path) -> Particles {
        let mut particles = Particles::of_game(game.into());
        particles.new_emitter().unwrap();
        particles
    }

    #[test]
    fn every_choice_makes_the_emitter_what_it_says_and_holds_afterwards() {
        let game = tempfile::tempdir().unwrap();
        let mut particles = with_one_emitter(game.path());
        let choices = (0..ShapeKind::ALL.len())
            .map(EmitterChoice::Shape)
            .chain([
                EmitterChoice::Additive(true),
                EmitterChoice::Additive(false),
                EmitterChoice::Looping(false),
                EmitterChoice::Looping(true),
            ])
            .collect::<Vec<_>>();
        for choice in choices {
            particles.apply(&choice.change());
            assert!(particles.problem().is_none(), "{choice:?}");
            assert!(choice.holds(particles.chosen().unwrap()), "{choice:?}");
        }
        assert_eq!(EmitterChoice::Shape(2).name(), "Cone");
        assert_eq!(EmitterChoice::Additive(false).name(), "Alpha");
        assert_eq!(EmitterChoice::Looping(true).name(), "Repeats");
    }

    #[test]
    fn the_panel_is_rebuilt_for_another_shape_or_refusal_and_not_for_another_number() {
        let game = tempfile::tempdir().unwrap();
        let mut particles = with_one_emitter(game.path());
        let before = panel_shape(&particles);
        let rate = |value| EmitterChange::Number {
            field: "emission.rate_per_second".into(),
            component: 0,
            value,
        };
        particles.apply(&rate(30.0));
        assert!(panel_shape(&particles) == before);

        particles.apply(&EmitterChoice::Shape(3).change());
        let boxed = panel_shape(&particles);
        assert!(boxed != before);
        assert_eq!(boxed.shape, Some("Box"));

        particles.apply(&rate(-5.0));
        let refused = panel_shape(&particles);
        assert!(refused != boxed);
        let words = refused.problem.unwrap();
        assert_eq!(
            words.title,
            "An emitter cannot have that value for its rate"
        );
        assert_eq!(
            words.remedy,
            "It may be from 0 to 10000. Nothing was changed."
        );
        // The next change that is taken clears what was refused.
        particles.apply(&rate(31.0));
        assert!(panel_shape(&particles) == boxed);
    }

    #[test]
    fn a_file_that_cannot_be_read_says_why_what_to_do_and_where_it_is() {
        let game = tempfile::tempdir().unwrap();
        let folder = game.path().join("assets/particles");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("bad-id.emitter-v1.json"), "{").unwrap();
        let particles = Particles::of_game(game.path().into());
        let (words, file) = refusal_words(&particles).unwrap();
        assert_eq!(
            words.title,
            "The file's name is not the ID of the emitter in it"
        );
        assert!(!words.remedy.is_empty());
        assert!(
            file.starts_with("File: ") && file.contains("bad-id.emitter-v1.json"),
            "{file}"
        );
        assert!(file.contains("Details: "), "{file}");
        assert_eq!(saved_words(&particles), "");
    }

    #[test]
    fn the_foot_says_whether_there_is_anything_to_save() {
        let game = tempfile::tempdir().unwrap();
        let mut particles = with_one_emitter(game.path());
        assert_eq!(
            saved_words(&particles),
            "This emitter has changes that are not saved."
        );
        particles.save_chosen();
        assert_eq!(saved_words(&particles), "Saved.");
        assert_eq!(saved_words(&Particles::default()), "");
    }

    #[test]
    fn the_limit_line_says_the_most_that_can_be_alive() {
        let game = tempfile::tempdir().unwrap();
        let particles = with_one_emitter(game.path());
        assert_eq!(
            estimate_words(particles.chosen().unwrap()),
            "With these settings at most 20 particles are alive at once."
        );
    }

    #[test]
    fn the_preview_is_headed_a_reference_simulation_and_says_what_that_means() {
        assert_eq!(REFERENCE_TITLE, "Reference simulation");
        assert!(REFERENCE_NOTE.contains("not the game's particle runtime"));
        assert!(REFERENCE_NOTE.contains("may draw the emitter differently"));
        assert_eq!((run_caption(true), run_caption(false)), ("Pause", "Resume"));

        let game = tempfile::tempdir().unwrap();
        let mut preview = Preview::default();
        let off = GpuPreview::Off;
        assert_eq!(preview.mode, PreviewMode::Reference);
        assert_eq!(
            preview_words(&Particles::default(), &preview, &off),
            "No game, so no emitter to show."
        );
        let empty = Particles::of_game(game.path().into());
        assert_eq!(
            preview_words(&empty, &preview, &off),
            "No emitter is chosen."
        );
        let particles = with_one_emitter(game.path());
        preview.show(particles.chosen());
        assert_eq!(
            preview_words(&particles, &preview, &off),
            "0 particles alive, at most 20. 0.0 s"
        );
    }

    #[test]
    fn the_overlay_is_headed_by_what_is_drawn_and_never_by_what_was_asked_for() {
        let states = [
            GpuPreview::Off,
            GpuPreview::Starting,
            GpuPreview::Unavailable("it has 2 bind groups and 4 are needed".into()),
            GpuPreview::Unsupported {
                field: "shape".into(),
                reason: "point emitters only".into(),
            },
            GpuPreview::Failed("the graphics card did not take the effect".into()),
        ];
        // Asked for and not drawn: the reference's heading and note, and the reason.
        for gpu in &states {
            assert_eq!(
                preview_title(PreviewMode::Gpu, gpu),
                REFERENCE_TITLE,
                "{gpu:?}"
            );
            assert_eq!(preview_note(PreviewMode::Gpu, gpu), REFERENCE_NOTE);
            let reason = gpu_reason(PreviewMode::Gpu, gpu).unwrap();
            assert!(
                reason.ends_with("The reference simulation is shown."),
                "{reason}"
            );
            // Not asked for: nothing is said of it.
            assert_eq!(gpu_reason(PreviewMode::Reference, gpu), None);
        }
        // Drawn: headed so, and no reason. Running without being asked for is the reference.
        let running = GpuPreview::Running;
        assert_eq!(preview_title(PreviewMode::Gpu, &running), "GPU preview");
        assert!(preview_note(PreviewMode::Gpu, &running).contains("game's particle runtime"));
        assert_eq!(gpu_reason(PreviewMode::Gpu, &running), None);
        assert_eq!(
            preview_title(PreviewMode::Reference, &running),
            REFERENCE_TITLE
        );

        // Each reason says its own thing, in words.
        let unsupported = gpu_reason(PreviewMode::Gpu, &states[3]).unwrap();
        assert!(unsupported.contains("with this shape yet"), "{unsupported}");
        assert!(
            unsupported.contains("the game cannot show this emitter"),
            "{unsupported}"
        );
        let drag = gpu_reason(
            PreviewMode::Gpu,
            &GpuPreview::Unsupported {
                field: "motion.drag_per_s".into(),
                reason: String::new(),
            },
        )
        .unwrap();
        assert!(drag.contains("with this drag yet"), "{drag}");
        assert!(gpu_reason(PreviewMode::Gpu, &states[2])
            .unwrap()
            .contains("2 bind groups"));
        assert!(gpu_reason(PreviewMode::Gpu, &states[1])
            .unwrap()
            .contains("starting"));
    }

    #[test]
    fn the_gpu_preview_says_the_most_that_can_be_alive_and_not_a_count_it_cannot_read() {
        let game = tempfile::tempdir().unwrap();
        let particles = with_one_emitter(game.path());
        let mut preview = Preview::default();
        preview.show(particles.chosen());
        preview.mode = PreviewMode::Gpu;
        assert_eq!(
            preview_words(&particles, &preview, &GpuPreview::Running),
            "At most 20 particles alive, by the settings."
        );
        preview.running = false;
        assert!(preview_words(&particles, &preview, &GpuPreview::Running).ends_with(" Paused."));
        // Asked for and not drawn: the reference's own count.
        assert_eq!(
            preview_words(&particles, &preview, &GpuPreview::Starting),
            "0 particles alive, at most 20. 0.0 s, paused"
        );
    }
}
