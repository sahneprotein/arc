//! The Particles tool as the panels handle it, without drawing: the game's emitters, the one
//! that is chosen, and the preview's controls.
//!
//! An emitter is a file of the game, under its `assets/particles` folder, in the format the
//! game and the editor share (`particle_spec`). The files are read and written by the
//! editor's services; nothing here knows the format's rules. Every change to an emitter goes
//! through the spec's validation and is one step of that emitter's own history: the level's
//! undo does not touch an emitter, and an emitter's does not touch the level.
//!
//! Nothing is written until Save. An emitter that differs from its file, or has none yet, is
//! marked as unsaved in the list, and the editor asks before it closes with one.
//!
//! The preview is the services' reference simulation of what the spec writes down. It is not
//! the game's particle runtime, which is not connected to the editor, and the panel says so.

use std::path::{Path, PathBuf};

use bevy::prelude::*;
use editor_services::{
    emitters::{list_emitters, remove_emitter, EmitterEdit, ListedEmitter, OnDisk},
    particle_spec::{self as spec},
};
use scene_data::Diagnostic;

pub use editor_services::{
    emitters::{
        number, number_bound, EmitterChange, EmitterPreview, NumberField, PreviewParticle,
        ShapeKind, NUMBER_FIELDS,
    },
    particle_spec::{default_emitter, max_live_estimate, Blend, EmitterSpec, Shape},
};

/// One row of the emitters' list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmitterRow {
    pub id: String,
    /// The emitter's name, or the file's name if the file cannot be read.
    pub name: String,
    /// Whether the emitter differs from its file, or has none yet.
    pub unsaved: bool,
    /// Why the file cannot be read, if it cannot: the refusal's code and the field at fault
    /// where it names one. The panel says it in words.
    pub refusal: Option<(String, Option<String>)>,
    pub chosen: bool,
}

impl EmitterRow {
    /// Whether the file cannot be read.
    pub fn refused(&self) -> bool {
        self.refusal.is_some()
    }
}

enum Held {
    /// An emitter that can be edited.
    Open(Box<EmitterEdit>),
    /// A file that cannot be read, with the reason.
    Refused(Diagnostic),
}

struct Entry {
    id: String,
    path: PathBuf,
    held: Held,
}

impl Entry {
    fn edit(&self) -> Option<&EmitterEdit> {
        match &self.held {
            Held::Open(edit) => Some(edit),
            Held::Refused(_) => None,
        }
    }

    fn unsaved(&self) -> bool {
        self.edit().is_some_and(EmitterEdit::unsaved)
    }

    /// What the rows are ordered by: those that can be read first, by name and then by ID.
    fn order(&self) -> (bool, String, &str) {
        let name = self
            .edit()
            .map(|edit| edit.spec().name.to_lowercase())
            .unwrap_or_default();
        (self.edit().is_none(), name, &self.id)
    }
}

/// The game's emitters as the editor holds them.
#[derive(Resource, Default)]
pub struct Particles {
    game: Option<PathBuf>,
    entries: Vec<Entry>,
    chosen: Option<String>,
    /// What was last refused: a value the spec does not take, a file that changed on disk.
    problem: Option<Diagnostic>,
    /// The emitter whose removal waits for its question to be answered.
    removing: Option<String>,
    /// How many emitters were made here, for their IDs.
    made: u64,
}

impl Particles {
    /// The emitters of the game in this folder.
    pub fn of_game(game: PathBuf) -> Self {
        let mut particles = Self {
            game: Some(game),
            ..default()
        };
        particles.refresh();
        particles
    }

    /// Whether the editor was started with a game, which is where emitters are kept.
    pub fn has_game(&self) -> bool {
        self.game.is_some()
    }

    pub fn game(&self) -> Option<&Path> {
        self.game.as_deref()
    }

    /// Reads the game's folder again. An emitter with changes that are not saved is kept as
    /// it is, with its history, whatever its file has become: saving it says so if the file
    /// changed. One without is shown as its file is now, and is gone if its file is.
    pub fn refresh(&mut self) {
        let Some(game) = self.game.clone() else {
            return;
        };
        let listed = match list_emitters(&game) {
            Ok(listed) => listed,
            Err(refusal) => {
                self.problem = Some(refusal);
                return;
            }
        };
        let mut before = std::mem::take(&mut self.entries);
        for ListedEmitter { id, path, loaded } in listed {
            let held = before
                .iter()
                .position(|entry| entry.id == id)
                .map(|at| before.swap_remove(at).held);
            let held = match (held, loaded) {
                (Some(Held::Open(edit)), Ok(loaded))
                    if edit.unsaved()
                        || *edit.on_disk() == OnDisk::Sha256(loaded.sha256_hex.clone()) =>
                {
                    Held::Open(edit)
                }
                (Some(Held::Open(edit)), Err(_)) if edit.unsaved() => Held::Open(edit),
                (_, Ok(loaded)) => Held::Open(Box::new(EmitterEdit::loaded(loaded))),
                (_, Err(refusal)) => Held::Refused(refusal),
            };
            self.entries.push(Entry { id, path, held });
        }
        // What has no file any more stays only while it has changes to lose.
        self.entries
            .extend(before.into_iter().filter(Entry::unsaved));
        self.settle();
    }

    /// Puts the rows in order, and chooses a row if the chosen one is gone.
    fn settle(&mut self) {
        self.entries.sort_by(|a, b| a.order().cmp(&b.order()));
        let there = |id: &String| self.entries.iter().any(|entry| &entry.id == id);
        if !self.chosen.as_ref().is_some_and(there) {
            self.chosen = self.entries.first().map(|entry| entry.id.clone());
        }
        if !self.removing.as_ref().is_some_and(there) {
            self.removing = None;
        }
    }

    /// The list's rows, in its order.
    pub fn rows(&self) -> Vec<EmitterRow> {
        self.entries
            .iter()
            .map(|entry| EmitterRow {
                id: entry.id.clone(),
                name: match entry.edit() {
                    Some(edit) => edit.spec().name.clone(),
                    None => entry
                        .path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| entry.id.clone()),
                },
                unsaved: entry.unsaved(),
                refusal: match &entry.held {
                    Held::Open(_) => None,
                    Held::Refused(refusal) => Some((
                        refusal.code.clone(),
                        refusal.parameters.get("field").cloned(),
                    )),
                },
                chosen: self.chosen.as_ref() == Some(&entry.id),
            })
            .collect()
    }

    fn entry(&self) -> Option<&Entry> {
        let chosen = self.chosen.as_ref()?;
        self.entries.iter().find(|entry| &entry.id == chosen)
    }

    fn edit_mut(&mut self) -> Option<&mut EmitterEdit> {
        let chosen = self.chosen.as_ref()?;
        match &mut self
            .entries
            .iter_mut()
            .find(|entry| &entry.id == chosen)?
            .held
        {
            Held::Open(edit) => Some(edit),
            Held::Refused(_) => None,
        }
    }

    pub fn chosen_id(&self) -> Option<&str> {
        self.chosen.as_deref()
    }

    /// The chosen emitter, if it is one that can be edited.
    pub fn chosen(&self) -> Option<&EmitterSpec> {
        self.entry()?.edit().map(EmitterEdit::spec)
    }

    /// Why the chosen file cannot be read, if it is such a file, and where it is.
    pub fn chosen_refusal(&self) -> Option<(&Diagnostic, &Path)> {
        let entry = self.entry()?;
        match &entry.held {
            Held::Refused(refusal) => Some((refusal, &entry.path)),
            Held::Open(_) => None,
        }
    }

    pub fn chosen_unsaved(&self) -> bool {
        self.entry().is_some_and(Entry::unsaved)
    }

    pub fn can_undo(&self) -> bool {
        self.entry()
            .and_then(Entry::edit)
            .is_some_and(EmitterEdit::can_undo)
    }

    pub fn can_redo(&self) -> bool {
        self.entry()
            .and_then(Entry::edit)
            .is_some_and(EmitterEdit::can_redo)
    }

    /// How many emitters have changes that are not saved.
    pub fn unsaved_count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.unsaved()).count()
    }

    /// What was last refused.
    pub fn problem(&self) -> Option<&Diagnostic> {
        self.problem.as_ref()
    }

    pub fn dismiss_problem(&mut self) {
        self.problem = None;
    }

    pub fn choose(&mut self, id: &str) {
        if self.entries.iter().any(|entry| entry.id == id) && self.chosen.as_deref() != Some(id) {
            self.chosen = Some(id.to_owned());
            self.problem = None;
        }
    }

    /// An ID no emitter here has: the time in nanoseconds with a counter, as the spec writes
    /// an ID. Unique in this editor, and unlikely to meet one made elsewhere.
    fn fresh_id(&mut self) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        loop {
            self.made += 1;
            let id = format!("{:032x}", (nanos << 20) | u128::from(self.made & 0xf_ffff));
            if id.bytes().any(|byte| byte != b'0')
                && !self.entries.iter().any(|entry| entry.id == id)
            {
                return id;
            }
        }
    }

    /// `name`, or `name` with the first number after it that no emitter here is called.
    fn free_name(&self, name: &str) -> String {
        let taken = |name: &str| {
            self.entries
                .iter()
                .filter_map(Entry::edit)
                .any(|edit| edit.spec().name == name)
        };
        if !taken(name) {
            return name.to_owned();
        }
        (2..)
            .map(|number| format!("{name} {number}"))
            .find(|name| !taken(name))
            .unwrap_or_else(|| name.to_owned())
    }

    /// Takes `emitter` into the list as one that has no file yet, and chooses it.
    fn add(&mut self, emitter: EmitterSpec) -> Option<String> {
        let game = self.game.as_ref()?;
        let path = match spec::asset_path(game, &emitter.id) {
            Ok(path) => path,
            Err(refusal) => {
                self.problem = Some(editor_services::emitters::emitter_diagnostic(
                    &refusal,
                    &editor_services::emitters::emitters_folder(game),
                ));
                return None;
            }
        };
        let id = emitter.id.clone();
        self.entries.push(Entry {
            id: id.clone(),
            path,
            held: Held::Open(Box::new(EmitterEdit::new(emitter))),
        });
        self.chosen = Some(id.clone());
        self.problem = None;
        self.settle();
        Some(id)
    }

    /// A new emitter with the spec's defaults. It has no file until it is saved. `None`
    /// without a game.
    pub fn new_emitter(&mut self) -> Option<String> {
        self.game.as_ref()?;
        let emitter = default_emitter();
        let emitter = EmitterSpec {
            id: self.fresh_id(),
            name: self.free_name(&emitter.name),
            ..emitter
        };
        self.add(emitter)
    }

    /// A copy of the chosen emitter as it is now, under an ID of its own. It has no file
    /// until it is saved. `None` when no emitter that can be edited is chosen.
    pub fn duplicate_chosen(&mut self) -> Option<String> {
        let original = self.chosen()?.clone();
        let named = EmitterSpec {
            id: self.fresh_id(),
            name: self.free_name(&format!("{} copy", original.name)),
            ..original.clone()
        };
        // A name that the addition makes too long for the spec stays as it was.
        let copy = if spec::validate(&named).is_ok() {
            named
        } else {
            EmitterSpec {
                name: original.name,
                ..named
            }
        };
        self.add(copy)
    }

    /// Changes the chosen emitter, as one step of its history. A change the spec refuses
    /// changes nothing and is the problem the panel shows. Whether the emitter changed.
    pub fn apply(&mut self, change: &EmitterChange) -> bool {
        let Some(edit) = self.edit_mut() else {
            return false;
        };
        match edit.apply(change) {
            Ok(changed) => {
                self.problem = None;
                if changed {
                    self.settle();
                }
                changed
            }
            Err(refusal) => {
                self.problem = Some(refusal);
                false
            }
        }
    }

    /// Takes the chosen emitter's last change back.
    pub fn undo(&mut self) -> bool {
        let undone = self.edit_mut().is_some_and(EmitterEdit::undo);
        if undone {
            self.problem = None;
            self.settle();
        }
        undone
    }

    /// Makes the chosen emitter's change again that was last taken back.
    pub fn redo(&mut self) -> bool {
        let redone = self.edit_mut().is_some_and(EmitterEdit::redo);
        if redone {
            self.problem = None;
            self.settle();
        }
        redone
    }

    /// Writes the chosen emitter to its file. Whether it is saved now; if not, the problem
    /// says why and the emitter is as it was.
    pub fn save_chosen(&mut self) -> bool {
        let Some(game) = self.game.clone() else {
            return false;
        };
        let Some(edit) = self.edit_mut() else {
            return false;
        };
        if !edit.unsaved() {
            return true;
        }
        match edit.save(&game) {
            Ok(_) => {
                self.problem = None;
                true
            }
            Err(refusal) => {
                self.problem = Some(refusal);
                false
            }
        }
    }

    /// Writes every emitter that has changes. Whether all of them are saved now; the first
    /// that could not be is chosen, and the problem says why.
    pub fn save_all(&mut self) -> bool {
        let Some(game) = self.game.clone() else {
            return self.unsaved_count() == 0;
        };
        for entry in &mut self.entries {
            let Held::Open(edit) = &mut entry.held else {
                continue;
            };
            if !edit.unsaved() {
                continue;
            }
            if let Err(refusal) = edit.save(&game) {
                self.chosen = Some(entry.id.clone());
                self.problem = Some(refusal);
                return false;
            }
        }
        self.problem = None;
        true
    }

    /// Whether the chosen emitter can be removed here: one that can be edited. A file that
    /// cannot be read is not the editor's to remove.
    pub fn can_remove(&self) -> bool {
        self.chosen().is_some()
    }

    /// Asks whether the chosen emitter is to be removed.
    pub fn ask_remove(&mut self) {
        if self.can_remove() {
            self.removing.clone_from(&self.chosen);
        }
    }

    /// The name of the emitter whose removal is asked about, and whether it has a file.
    pub fn removing(&self) -> Option<(&str, bool)> {
        let id = self.removing.as_ref()?;
        let edit = self.entries.iter().find(|entry| &entry.id == id)?.edit()?;
        Some((&edit.spec().name, *edit.on_disk() != OnDisk::Absent))
    }

    pub fn cancel_remove(&mut self) {
        self.removing = None;
    }

    /// Removes the emitter that was asked about: its file, if it is still what the editor
    /// last saw, and its row. Whether it is gone; if not, the problem says why.
    pub fn confirm_remove(&mut self) -> bool {
        let Some(id) = self.removing.take() else {
            return false;
        };
        let Some(at) = self.entries.iter().position(|entry| entry.id == id) else {
            return false;
        };
        let on_disk = match self.entries[at].edit() {
            Some(edit) => edit.on_disk().clone(),
            None => return false,
        };
        if on_disk != OnDisk::Absent {
            let Some(game) = self.game.as_ref() else {
                return false;
            };
            if let Err(refusal) = remove_emitter(game, &id, &on_disk) {
                self.problem = Some(refusal);
                return false;
            }
        }
        self.entries.remove(at);
        // The row after it is chosen, or the last one.
        self.chosen = self
            .entries
            .get(at)
            .or(self.entries.last())
            .map(|entry| entry.id.clone());
        self.problem = None;
        true
    }
}

/// How fast the preview's time goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PreviewSpeed {
    Quarter,
    #[default]
    Full,
    Double,
}

impl PreviewSpeed {
    pub const ALL: [PreviewSpeed; 3] = [Self::Quarter, Self::Full, Self::Double];

    pub fn factor(self) -> f64 {
        match self {
            Self::Quarter => 0.25,
            Self::Full => 1.0,
            Self::Double => 2.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Quarter => "0.25x",
            Self::Full => "1x",
            Self::Double => "2x",
        }
    }
}

/// Which picture of the chosen emitter is asked for.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PreviewMode {
    /// The editor's reference simulation, worked out on the processor.
    #[default]
    Reference,
    /// The game's particle runtime, on the graphics card.
    Gpu,
}

/// What the application says of the GPU preview: the game's particle runtime drawing the
/// chosen emitter in the 3D view. The application is the one that knows; the panel shows
/// the reference simulation, and says why, whenever this is not [`GpuPreview::Running`].
#[derive(Resource, Clone, PartialEq, Eq, Debug, Default)]
pub enum GpuPreview {
    /// It is not asked for, or there is no emitter to show.
    #[default]
    Off,
    /// The graphics device cannot run it, with what it lacks.
    Unavailable(String),
    /// The game's particle runtime does not draw an emitter with this setting: the field
    /// by the particle format's dotted name, and the runtime's reason.
    Unsupported { field: String, reason: String },
    /// The effect is made and the graphics card has not said yet that it can draw it.
    Starting,
    /// The graphics card says it draws the effect.
    Running,
    /// It was asked for and did not start, with what is known of why.
    Failed(String),
}

impl GpuPreview {
    pub fn running(&self) -> bool {
        *self == Self::Running
    }
}

/// The preview of the chosen emitter: the services' reference simulation and its controls.
/// Always from the same seed, so that Restart shows the same particles again.
#[derive(Resource)]
pub struct Preview {
    simulation: Option<EmitterPreview>,
    /// Whether time goes on. Paused, the particles stand still.
    pub running: bool,
    pub speed: PreviewSpeed,
    /// Which picture is asked for. What is shown is the application's to say
    /// ([`GpuPreview`]).
    pub mode: PreviewMode,
    /// How many times the preview was begun again, for the application's own effect to
    /// begin again with it.
    restarts: u32,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            simulation: None,
            running: true,
            speed: PreviewSpeed::default(),
            mode: PreviewMode::default(),
            restarts: 0,
        }
    }
}

impl Preview {
    /// The seed of every preview.
    pub const SEED: u64 = 1;

    /// Shows `emitter`, from time zero if it is another emitter than is shown, or the same
    /// one changed. Whether what is shown changed.
    pub fn show(&mut self, emitter: Option<&EmitterSpec>) -> bool {
        let Some(emitter) = emitter else {
            return self.simulation.take().is_some();
        };
        match &mut self.simulation {
            Some(simulation) if simulation.spec() == emitter => false,
            Some(simulation) => {
                simulation.set_spec(emitter.clone());
                true
            }
            None => {
                self.simulation = Some(EmitterPreview::new(emitter.clone(), Self::SEED));
                true
            }
        }
    }

    /// Begins again at time zero: the same particles as before.
    pub fn restart(&mut self) {
        self.restarts = self.restarts.wrapping_add(1);
        if let Some(simulation) = &mut self.simulation {
            simulation.restart();
        }
    }

    /// How many times the preview was begun again.
    pub fn restarts(&self) -> u32 {
        self.restarts
    }

    /// Whether time is going on: the preview runs, shows an emitter, and that emitter has
    /// not ended.
    pub fn moving(&self) -> bool {
        self.running
            && self
                .simulation
                .as_ref()
                .is_some_and(|simulation| !simulation.finished())
    }

    /// Lets `real_seconds` of the clock pass at the preview's speed. Whether anything moved.
    pub fn tick(&mut self, real_seconds: f64) -> bool {
        if !self.moving() {
            return false;
        }
        let Some(simulation) = &mut self.simulation else {
            return false;
        };
        let before = simulation.time_seconds();
        simulation.advance(real_seconds * self.speed.factor());
        simulation.time_seconds() != before
    }

    /// Takes whole steps of the simulation, whether the preview runs or not.
    pub fn step(&mut self, steps: u32) {
        if let Some(simulation) = &mut self.simulation {
            for _ in 0..steps {
                simulation.step();
            }
        }
    }

    pub fn simulation(&self) -> Option<&EmitterPreview> {
        self.simulation.as_ref()
    }

    /// What the preview says of itself: how many particles are alive, the most the spec
    /// lets be, and the time it has come to.
    pub fn words(&self) -> String {
        let Some(simulation) = &self.simulation else {
            return String::new();
        };
        let alive = simulation.live();
        let most = max_live_estimate(simulation.spec());
        let particles = if alive == 1 { "particle" } else { "particles" };
        let state = if simulation.finished() {
            ", ended"
        } else if !self.running {
            ", paused"
        } else {
            ""
        };
        format!(
            "{alive} {particles} alive, at most {most}. {:.1} s{state}",
            simulation.time_seconds()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(field: &str, value: f64) -> EmitterChange {
        EmitterChange::Number {
            field: field.into(),
            component: 0,
            value,
        }
    }

    fn names(particles: &Particles) -> Vec<String> {
        particles.rows().into_iter().map(|row| row.name).collect()
    }

    #[test]
    fn without_a_game_there_are_no_emitters_and_none_can_be_made() {
        let mut particles = Particles::default();
        assert!(!particles.has_game());
        assert_eq!(particles.new_emitter(), None);
        assert_eq!(particles.rows(), []);
        assert!(particles.save_all());
    }

    #[test]
    fn a_new_emitter_is_chosen_unsaved_and_has_no_file_until_it_is_saved() {
        let game = tempfile::tempdir().unwrap();
        let mut particles = Particles::of_game(game.path().into());
        assert_eq!(particles.rows(), []);

        let id = particles.new_emitter().unwrap();
        let rows = particles.rows();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].name.as_str(), rows[0].unsaved, rows[0].chosen),
            ("New Emitter", true, true)
        );
        assert_eq!(particles.chosen_id(), Some(id.as_str()));
        assert_eq!(particles.unsaved_count(), 1);
        assert!(!game.path().join("assets/particles").exists());

        assert!(particles.save_chosen());
        assert_eq!(particles.unsaved_count(), 0);
        // Another editor on the same game finds it.
        assert_eq!(
            names(&Particles::of_game(game.path().into())),
            ["New Emitter"]
        );
        // A second new one gets a name of its own, and an ID of its own.
        let second = particles.new_emitter().unwrap();
        assert_ne!(second, id);
        assert_eq!(names(&particles), ["New Emitter", "New Emitter 2"]);
    }

    #[test]
    fn a_change_is_one_step_of_the_emitters_own_history_and_a_refused_one_is_a_problem() {
        let game = tempfile::tempdir().unwrap();
        let mut particles = Particles::of_game(game.path().into());
        particles.new_emitter().unwrap();
        assert!(particles.save_chosen());
        assert!(!particles.can_undo());

        assert!(particles.apply(&number("emission.rate_per_second", 25.0)));
        assert!(particles.chosen_unsaved() && particles.can_undo());
        assert_eq!(particles.chosen().unwrap().emission.rate_per_second, 25.0);
        // The same value again is no step.
        assert!(!particles.apply(&number("emission.rate_per_second", 25.0)));

        // A value the spec refuses changes nothing and says which field.
        assert!(!particles.apply(&number("emission.rate_per_second", -1.0)));
        let refusal = particles.problem().unwrap();
        assert_eq!(refusal.code, "particle.value");
        assert_eq!(refusal.parameters["field"], "emission.rate_per_second");
        assert_eq!(particles.chosen().unwrap().emission.rate_per_second, 25.0);

        // Undo takes the change back, the emitter is as its file is, and the problem is gone.
        assert!(particles.undo());
        assert!(!particles.chosen_unsaved() && particles.problem().is_none());
        assert!(particles.redo() && particles.chosen_unsaved());
        assert!(!particles.redo());
    }

    #[test]
    fn a_rename_reorders_the_list_and_keeps_the_emitter_chosen() {
        let game = tempfile::tempdir().unwrap();
        let mut particles = Particles::of_game(game.path().into());
        let first = particles.new_emitter().unwrap();
        particles.new_emitter().unwrap();
        particles.choose(&first);
        assert!(particles.apply(&EmitterChange::Name("Smoke".into())));
        assert_eq!(names(&particles), ["New Emitter 2", "Smoke"]);
        assert_eq!(particles.chosen_id(), Some(first.as_str()));
        assert!(particles.rows()[1].chosen);
    }

    #[test]
    fn a_duplicate_is_a_copy_under_its_own_id_with_no_file() {
        let game = tempfile::tempdir().unwrap();
        let mut particles = Particles::of_game(game.path().into());
        let original = particles.new_emitter().unwrap();
        particles.apply(&number("emission.rate_per_second", 40.0));
        particles.save_chosen();

        let copy = particles.duplicate_chosen().unwrap();
        assert_ne!(copy, original);
        assert_eq!(particles.chosen_id(), Some(copy.as_str()));
        let chosen = particles.chosen().unwrap();
        assert_eq!(
            (chosen.name.as_str(), chosen.emission.rate_per_second),
            ("New Emitter copy", 40.0)
        );
        assert!(particles.chosen_unsaved());
        assert_eq!(particles.unsaved_count(), 1);

        // A name as long as the spec lets it be is kept as it is.
        let long = "n".repeat(spec::NAME_MAX_BYTES);
        assert!(particles.apply(&EmitterChange::Name(long.clone())));
        particles.duplicate_chosen().unwrap();
        assert_eq!(particles.chosen().unwrap().name, long);
    }

    #[test]
    fn a_removal_is_asked_about_first_and_takes_the_file_and_the_row() {
        let game = tempfile::tempdir().unwrap();
        let mut particles = Particles::of_game(game.path().into());
        let first = particles.new_emitter().unwrap();
        particles.save_chosen();
        let second = particles.new_emitter().unwrap();
        particles.choose(&first);

        assert!(particles.removing().is_none());
        particles.ask_remove();
        assert_eq!(particles.removing(), Some(("New Emitter", true)));
        // Keeping it changes nothing.
        particles.cancel_remove();
        assert!(particles.removing().is_none());
        assert!(!particles.confirm_remove());
        assert_eq!(particles.rows().len(), 2);

        particles.ask_remove();
        assert!(particles.confirm_remove());
        assert_eq!(particles.chosen_id(), Some(second.as_str()));
        assert_eq!(Particles::of_game(game.path().into()).rows(), []);

        // One that was never saved has no file to remove.
        particles.ask_remove();
        assert_eq!(particles.removing(), Some(("New Emitter 2", false)));
        assert!(particles.confirm_remove());
        assert_eq!(particles.rows(), []);
        assert_eq!(particles.chosen_id(), None);
    }

    #[test]
    fn a_file_that_changed_on_disk_is_not_written_over_or_removed() {
        let game = tempfile::tempdir().unwrap();
        let mut particles = Particles::of_game(game.path().into());
        particles.new_emitter().unwrap();
        particles.save_chosen();
        // Another editor changes and saves the same emitter.
        let mut other = Particles::of_game(game.path().into());
        other.apply(&number("emission.rate_per_second", 60.0));
        assert!(other.save_chosen());

        particles.apply(&number("emission.rate_per_second", 5.0));
        assert!(!particles.save_chosen());
        assert_eq!(particles.problem().unwrap().code, "particle.conflict");
        assert!(particles.chosen_unsaved());
        assert!(!particles.save_all());
        particles.ask_remove();
        assert!(!particles.confirm_remove());
        assert_eq!(particles.problem().unwrap().code, "particle.conflict");
        assert_eq!(particles.rows().len(), 1);
    }

    #[test]
    fn reading_the_folder_again_keeps_unsaved_changes_and_takes_what_changed_elsewhere() {
        let game = tempfile::tempdir().unwrap();
        let mut particles = Particles::of_game(game.path().into());
        let kept = particles.new_emitter().unwrap();
        particles.save_chosen();
        let followed = particles.new_emitter().unwrap();
        particles.save_chosen();
        let never_saved = particles.new_emitter().unwrap();

        // Elsewhere: one is changed, and a file that is no emitter appears.
        let mut other = Particles::of_game(game.path().into());
        other.choose(&followed);
        other.apply(&EmitterChange::Name("Dust".into()));
        other.save_chosen();
        std::fs::write(
            game.path().join("assets/particles/bad-id.emitter-v1.json"),
            "{",
        )
        .unwrap();
        // Here: the first has a change that is not saved.
        particles.choose(&kept);
        particles.apply(&number("emission.rate_per_second", 33.0));

        particles.refresh();
        let rows = particles.rows();
        assert_eq!(
            rows.iter().map(|row| row.name.as_str()).collect::<Vec<_>>(),
            [
                "Dust",
                "New Emitter",
                "New Emitter 3",
                "bad-id.emitter-v1.json"
            ]
        );
        assert_eq!(particles.chosen_id(), Some(kept.as_str()));
        assert!(particles.chosen_unsaved() && particles.can_undo());
        assert!(rows[3].refused() && !rows[3].unsaved);
        assert_eq!(
            rows[3].refusal,
            Some(("particle.value".to_owned(), Some("id".to_owned())))
        );
        assert!(rows.iter().any(|row| row.id == never_saved && row.unsaved));

        // The file that cannot be read says why, and is not the editor's to remove.
        particles.choose("bad-id");
        let (refusal, path) = particles.chosen_refusal().unwrap();
        assert_eq!(refusal.code, "particle.value");
        assert!(path.ends_with("bad-id.emitter-v1.json"));
        assert!(particles.chosen().is_none() && !particles.can_remove());
        particles.ask_remove();
        assert!(particles.removing().is_none());
    }

    #[test]
    fn saving_all_writes_every_emitter_that_has_changes() {
        let game = tempfile::tempdir().unwrap();
        let mut particles = Particles::of_game(game.path().into());
        particles.new_emitter().unwrap();
        particles.new_emitter().unwrap();
        assert_eq!(particles.unsaved_count(), 2);
        assert!(particles.save_all());
        assert_eq!(particles.unsaved_count(), 0);
        assert_eq!(Particles::of_game(game.path().into()).rows().len(), 2);
    }

    #[test]
    fn the_preview_shows_the_chosen_emitter_from_the_start_and_repeats_itself() {
        let mut preview = Preview::default();
        assert_eq!(preview.words(), "");
        assert!(!preview.moving() && !preview.tick(1.0));

        let emitter = default_emitter();
        assert!(preview.show(Some(&emitter)));
        assert!(
            !preview.show(Some(&emitter)),
            "the same emitter is not begun again"
        );
        assert!(preview.moving());
        preview.step(60);
        let alive = preview.simulation().unwrap().live();
        assert!(alive > 0);
        assert_eq!(
            preview.words(),
            format!("{alive} particles alive, at most 20. 1.0 s")
        );

        // Restart and the same steps: the same particles.
        let first: Vec<_> = preview.simulation().unwrap().particles().collect();
        preview.restart();
        assert_eq!(preview.simulation().unwrap().live(), 0);
        preview.step(60);
        let again: Vec<_> = preview.simulation().unwrap().particles().collect();
        assert_eq!(first, again);

        // A changed emitter begins again; no emitter shows nothing.
        let faster = EmitterSpec {
            emission: spec::Emission {
                rate_per_second: 50.0,
                ..emitter.emission.clone()
            },
            ..emitter
        };
        assert!(preview.show(Some(&faster)));
        assert_eq!(preview.simulation().unwrap().time_seconds(), 0.0);
        assert!(preview.show(None));
        assert!(preview.simulation().is_none());
    }

    #[test]
    fn a_paused_preview_stands_still_and_time_goes_at_the_chosen_speed() {
        let mut preview = Preview::default();
        preview.show(Some(&default_emitter()));
        assert!(preview.tick(0.1));
        let at = preview.simulation().unwrap().time_seconds();
        assert!((at - 0.1).abs() < 0.02, "{at}");

        preview.running = false;
        assert!(!preview.tick(0.1));
        assert_eq!(preview.simulation().unwrap().time_seconds(), at);
        assert!(preview.words().ends_with(", paused"), "{}", preview.words());

        preview.running = true;
        preview.speed = PreviewSpeed::Double;
        preview.tick(0.1);
        let doubled = preview.simulation().unwrap().time_seconds() - at;
        assert!((doubled - 0.2).abs() < 0.02, "{doubled}");
        assert_eq!(PreviewSpeed::Quarter.name(), "0.25x");
    }

    #[test]
    fn an_emitter_that_does_not_loop_ends_and_the_preview_says_so() {
        let once = EmitterSpec {
            emission: spec::Emission {
                looping: false,
                duration_seconds: 0.5,
                ..default_emitter().emission
            },
            ..default_emitter()
        };
        let mut preview = Preview::default();
        preview.show(Some(&once));
        preview.step(60 * 4);
        assert!(!preview.moving());
        assert!(!preview.tick(0.1));
        assert!(preview.words().ends_with(", ended"), "{}", preview.words());
        preview.restart();
        assert!(preview.moving());
    }
}
