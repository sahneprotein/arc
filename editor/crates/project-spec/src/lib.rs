//! The versioned, renderer-independent project manifest shared by the editor and game.
//!
//! The unique `.aproj` lives at the game/project root. It selects the starter level,
//! renderer profile and current keyboard controls. The Play and gameplay groups are
//! present as typed, reserved settings so the UI can show the planned shape while keeping
//! those controls disabled until the game implements them.
//!
use serde::{Deserialize, Serialize};
use std::path::Path;

mod discovery;
pub use discovery::{project_root_from_file, resolve_project_path, DiscoveryError};

/// Default name used by setup. Existing files may be renamed.
pub const FILE_NAME: &str = "project.aproj";
pub const EXTENSION: &str = "aproj";
pub const SCHEMA: u32 = 1;
pub const MAX_FILE_BYTES: usize = 32 * 1024;

/// Whether a path names a project file. Case insensitive on every platform so discovery
/// never silently selects a different project after a folder is moved between systems.
pub fn is_project_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION))
}

/// Every name a key binding may have, as the game reads it: case matters, and nothing else
/// is a key. The game's client turns exactly these into keys and refuses to start on any
/// other, so a manifest that passes [`validate`] is one the client can play with.
pub const KEY_NAMES: [&str; 46] = [
    "A",
    "B",
    "C",
    "D",
    "E",
    "F",
    "G",
    "H",
    "I",
    "J",
    "K",
    "L",
    "M",
    "N",
    "O",
    "P",
    "Q",
    "R",
    "S",
    "T",
    "U",
    "V",
    "W",
    "X",
    "Y",
    "Z",
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "7",
    "8",
    "9",
    "Space",
    "Escape",
    "Enter",
    "Tab",
    "Backspace",
    "Delete",
    "ArrowUp",
    "ArrowDown",
    "ArrowLeft",
    "ArrowRight",
];

/// Whether `name` is a key a binding may name.
pub fn is_key_name(name: &str) -> bool {
    KEY_NAMES.contains(&name)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfig {
    pub schema: u32,
    pub project: ProjectIdentity,
    pub startup: StartupSettings,
    pub rendering: RenderingSettings,
    pub controls: ControlSettings,
    /// These fields are deliberately persisted as a forward-looking contract, but have
    /// no effect on a run until the coordinator and game implement them.
    pub planned: PlannedSettings,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectIdentity {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StartupSettings {
    /// Project-relative `.level.json` path, normalized with `/` separators.
    pub start_level: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RenderingSettings {
    /// Project-relative path to the current look profile. Existing profiles are preserved
    /// byte for byte during project setup and are migrated by their own versioned reader.
    pub profile: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ControlSettings {
    pub move_forward: String,
    pub move_backward: String,
    pub move_left: String,
    pub move_right: String,
    pub jump: String,
    pub toggle_view: String,
    pub release_cursor: String,
    pub look: LookBinding,
}

impl ControlSettings {
    /// The seven key bindings in the manifest's order, each with the field it is, as a
    /// refusal names it.
    pub fn keys(&self) -> [(&'static str, &str); 7] {
        [
            ("controls.move_forward", &self.move_forward),
            ("controls.move_backward", &self.move_backward),
            ("controls.move_left", &self.move_left),
            ("controls.move_right", &self.move_right),
            ("controls.jump", &self.jump),
            ("controls.toggle_view", &self.toggle_view),
            ("controls.release_cursor", &self.release_cursor),
        ]
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LookBinding {
    pub mouse: bool,
    pub gamepad_right_stick: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedSettings {
    pub multiplayer: PlannedMultiplayer,
    pub gameplay: PlannedGameplay,
}

/// The fewest and the most clients Play starts on one machine to test with.
pub const MIN_LOCAL_CLIENTS: u8 = 1;
pub const MAX_LOCAL_CLIENTS: u8 = 4;

/// Reserved for the roadmap's Play settings. `None` means not configured; the editor
/// renders these rows disabled until their runtime support ships.
///
/// `clients` is the first that is read: how many clients the editor's Play starts on this
/// machine, [`MIN_LOCAL_CLIENTS`] to [`MAX_LOCAL_CLIENTS`]. `None` is one client, and is
/// kept as `None` until a person sets a number.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedMultiplayer {
    pub clients: Option<u8>,
    pub bots: Option<String>,
    pub network_conditions: Option<String>,
    pub start_at: Option<String>,
    pub zone: Option<String>,
}

/// Reserved for authored game rules that do not exist yet. Keeping this explicit avoids
/// pretending that a setting is active merely because the project file can name it.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedGameplay {
    pub mode: Option<String>,
    pub team_count: Option<u16>,
    pub respawn_seconds: Option<f32>,
    pub weapon_set: Option<String>,
}

impl ProjectConfig {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema: SCHEMA,
            project: ProjectIdentity { name: name.into() },
            startup: StartupSettings {
                start_level: "levels/first-level.level.json".into(),
            },
            rendering: RenderingSettings {
                profile: "assets/render/look-v1.json".into(),
            },
            controls: ControlSettings {
                move_forward: "W".into(),
                move_backward: "S".into(),
                move_left: "A".into(),
                move_right: "D".into(),
                jump: "Space".into(),
                toggle_view: "V".into(),
                release_cursor: "Escape".into(),
                look: LookBinding {
                    mouse: true,
                    gamepad_right_stick: true,
                },
            },
            planned: PlannedSettings {
                multiplayer: PlannedMultiplayer {
                    clients: None,
                    bots: None,
                    network_conditions: None,
                    start_at: None,
                    zone: None,
                },
                gameplay: PlannedGameplay {
                    mode: None,
                    team_count: None,
                    respawn_seconds: None,
                    weapon_set: None,
                },
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectError {
    TooLarge,
    Decode(String),
    Schema {
        expected: u32,
        found: u32,
    },
    Value(&'static str),
    /// Two actions are bound to one key: the two fields, in the manifest's order.
    SharedKey {
        first: &'static str,
        second: &'static str,
    },
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => write!(f, "project file exceeds {MAX_FILE_BYTES} bytes"),
            Self::Decode(error) => write!(f, "project file is not valid JSON: {error}"),
            Self::Schema { expected, found } => {
                write!(f, "unsupported project schema {found}; expected {expected}")
            }
            Self::Value(field) => write!(f, "invalid project field {field}"),
            Self::SharedKey { first, second } => {
                write!(f, "project fields {first} and {second} name the same key")
            }
        }
    }
}

impl std::error::Error for ProjectError {}

/// Reads a bounded current-schema project manifest. It performs no I/O and refuses every other schema.
pub fn decode(bytes: &[u8]) -> Result<ProjectConfig, ProjectError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(ProjectError::TooLarge);
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|error| ProjectError::Decode(error.to_string()))?;
    let schema = value
        .get("schema")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(ProjectError::Value("schema"))?;
    if schema != SCHEMA {
        return Err(ProjectError::Schema {
            expected: SCHEMA,
            found: schema,
        });
    }
    let config: ProjectConfig =
        serde_json::from_value(value).map_err(|error| ProjectError::Decode(error.to_string()))?;
    validate(&config)?;
    Ok(config)
}

pub fn validate(config: &ProjectConfig) -> Result<(), ProjectError> {
    if config.schema != SCHEMA {
        return Err(ProjectError::Schema {
            expected: SCHEMA,
            found: config.schema,
        });
    }
    if config.project.name.trim().is_empty() || config.project.name.len() > 96 {
        return Err(ProjectError::Value("project.name"));
    }
    if !safe_relative_path(&config.startup.start_level) {
        return Err(ProjectError::Value("startup.start_level"));
    }
    if !safe_relative_path(&config.rendering.profile) {
        return Err(ProjectError::Value("rendering.profile"));
    }
    validate_controls(&config.controls)?;
    if let Some(clients) = config.planned.multiplayer.clients
        && !(MIN_LOCAL_CLIENTS..=MAX_LOCAL_CLIENTS).contains(&clients)
    {
        return Err(ProjectError::Value("planned.multiplayer.clients"));
    }
    if let Some(seconds) = config.planned.gameplay.respawn_seconds
        && (!seconds.is_finite() || seconds < 0.0 || seconds > 3600.0)
    {
        return Err(ProjectError::Value("planned.gameplay.respawn_seconds"));
    }
    Ok(())
}

/// Refuses bindings the game could not play with: a key that is not one of [`KEY_NAMES`],
/// named by its field, and two actions on one key, named by both fields. The game cannot
/// do two things on one key in an order anyone could rely on.
pub fn validate_controls(controls: &ControlSettings) -> Result<(), ProjectError> {
    let keys = controls.keys();
    if let Some(&(field, _)) = keys.iter().find(|(_, key)| !is_key_name(key)) {
        return Err(ProjectError::Value(field));
    }
    for (at, &(second, key)) in keys.iter().enumerate() {
        if let Some(&(first, _)) = keys[..at].iter().find(|(_, earlier)| *earlier == key) {
            return Err(ProjectError::SharedKey { first, second });
        }
    }
    Ok(())
}

fn safe_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !value.trim().is_empty()
        && !path.is_absolute()
        && !value.contains('\\')
        && path.components().all(|part| {
            matches!(
                part,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}

/// Stable editor serialization: two-space pretty JSON and one trailing newline.
pub fn canonical_bytes(config: &ProjectConfig) -> Result<Vec<u8>, ProjectError> {
    validate(config)?;
    let mut bytes = serde_json::to_vec_pretty(config)
        .map_err(|error| ProjectError::Decode(error.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_extension_is_case_insensitive_and_does_not_match_backups() {
        assert_eq!(EXTENSION, "aproj");
        for name in [
            FILE_NAME,
            "Harbour.aproj",
            "Harbour.APROJ",
            "folder with spaces/A.aproj",
        ] {
            assert!(is_project_file(Path::new(name)), "{name}");
        }
        for name in ["notes.json", "project.aproj.bak", "project.aproj.lock"] {
            assert!(!is_project_file(Path::new(name)), "{name}");
        }
    }

    #[test]
    fn only_schema_one_is_accepted() {
        assert_eq!(SCHEMA, 1);
        assert_eq!(FILE_NAME, "project.aproj");
        let mut value = serde_json::to_value(ProjectConfig::new("Arc Game")).unwrap();
        for found in [0, 2, 3, u32::MAX] {
            value["schema"] = found.into();
            assert_eq!(
                decode(&serde_json::to_vec(&value).unwrap()),
                Err(ProjectError::Schema { expected: 1, found })
            );
        }
    }

    #[test]
    fn defaults_describe_the_live_controls_and_keep_future_settings_empty() {
        let config = ProjectConfig::new("Arc Game");
        assert_eq!(config.controls.move_forward, "W");
        assert_eq!(config.controls.jump, "Space");
        assert_eq!(config.controls.release_cursor, "Escape");
        assert!(config.planned.multiplayer.clients.is_none());
        assert!(config.planned.gameplay.mode.is_none());
    }

    #[test]
    fn canonical_project_bytes_round_trip_and_are_stable() {
        let config = ProjectConfig::new("Arc Game");
        let first = canonical_bytes(&config).unwrap();
        assert!(first.ends_with(b"}\n"));
        assert_eq!(decode(&first).unwrap(), config);
        assert_eq!(canonical_bytes(&decode(&first).unwrap()).unwrap(), first);
    }

    #[test]
    fn closed_schema_paths_and_values_are_validated() {
        let bytes = canonical_bytes(&ProjectConfig::new("Arc Game")).unwrap();
        let unknown = String::from_utf8(bytes.clone()).unwrap().replace(
            &format!("\"schema\": {SCHEMA},"),
            &format!("\"schema\": {SCHEMA},\n  \"extra\": true,"),
        );
        assert!(matches!(
            decode(unknown.as_bytes()),
            Err(ProjectError::Decode(_))
        ));

        let mut config = ProjectConfig::new("Arc Game");
        config.startup.start_level = "../outside.level.json".into();
        assert_eq!(
            validate(&config),
            Err(ProjectError::Value("startup.start_level"))
        );
        config.startup.start_level = "levels/first.level.json".into();
        // One to four local test clients, or none named.
        for clients in [Some(0), Some(5), Some(u8::MAX)] {
            config.planned.multiplayer.clients = clients;
            assert_eq!(
                validate(&config),
                Err(ProjectError::Value("planned.multiplayer.clients"))
            );
        }
        for clients in [None, Some(MIN_LOCAL_CLIENTS), Some(MAX_LOCAL_CLIENTS)] {
            config.planned.multiplayer.clients = clients;
            assert_eq!(validate(&config), Ok(()));
        }
    }

    #[test]
    fn a_binding_is_one_of_the_keys_the_game_reads_and_no_two_actions_share_one() {
        // The vocabulary: 26 letters, 10 digits and ten named keys, each once.
        assert_eq!(KEY_NAMES.len(), 46);
        let mut names = KEY_NAMES.to_vec();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 46);
        assert!(is_key_name("W") && is_key_name("7") && is_key_name("ArrowLeft"));
        // Case and spelling matter, as they do to the game.
        for other in ["w", "space", "Esc", "Shift", "F1", "Left", " W", ""] {
            assert!(!is_key_name(other), "{other:?}");
        }

        // The defaults are playable, and so is every key in any one place.
        let defaults = ProjectConfig::new("Arc Game");
        assert_eq!(validate(&defaults), Ok(()));
        let mut config = defaults.clone();
        config.controls.jump = "Enter".into();
        assert_eq!(validate(&config), Ok(()));

        // A key the game does not read is refused with its field, in every place.
        let fields = defaults.controls.keys().map(|(field, _)| field);
        for (at, field) in fields.into_iter().enumerate() {
            let mut config = defaults.clone();
            let controls = &mut config.controls;
            let places = [
                &mut controls.move_forward,
                &mut controls.move_backward,
                &mut controls.move_left,
                &mut controls.move_right,
                &mut controls.jump,
                &mut controls.toggle_view,
                &mut controls.release_cursor,
            ];
            for (index, place) in places.into_iter().enumerate() {
                if index == at {
                    *place = "Shift".into();
                }
            }
            assert_eq!(validate(&config), Err(ProjectError::Value(field)));
            // Nothing that is refused is written or read back.
            assert!(canonical_bytes(&config).is_err());
            let bytes = serde_json::to_vec(&config).unwrap();
            assert_eq!(decode(&bytes), Err(ProjectError::Value(field)));
        }

        // Two actions on one key.
        let mut config = defaults.clone();
        config.controls.move_backward = "W".into();
        let shared = ProjectError::SharedKey {
            first: "controls.move_forward",
            second: "controls.move_backward",
        };
        assert_eq!(validate(&config), Err(shared.clone()));
        assert!(canonical_bytes(&config).is_err());
        assert_eq!(
            shared.to_string(),
            "project fields controls.move_forward and controls.move_backward name the same key"
        );
        // Of three on one key the first two are named.
        config.controls.jump = "W".into();
        assert_eq!(validate(&config), Err(shared));
        // What is looked with is free, also both off.
        let mut config = defaults;
        config.controls.look.mouse = false;
        config.controls.look.gamepad_right_stick = false;
        assert_eq!(validate(&config), Ok(()));
    }
}
