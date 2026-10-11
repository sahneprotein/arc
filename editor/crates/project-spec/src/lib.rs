//! The versioned, renderer-independent project manifest shared by the editor and game.
//!
//! `arc-project.json` lives at the game/project root. It selects the starter level,
//! renderer profile and current keyboard controls. The Play and gameplay groups are
//! present as typed, reserved settings so the UI can show the planned shape while keeping
//! those controls disabled until the game implements them.
//!
//! Conversion is explicit: callers parse the stored schema, run every numbered migration
//! in order, then write the current schema to a new file or preserve a backup before
//! replacing the old one. Parsing never silently upgrades or writes.

use serde::{Deserialize, Serialize};
use std::path::Path;

pub const FILE_NAME: &str = "arc-project.json";
pub const SCHEMA: u32 = 2;
pub const MAX_FILE_BYTES: usize = 32 * 1024;

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

/// The project manifest written before the planned settings were formally reserved.
/// The converter to schema 2 supplies empty planned sections and preserves every live
/// field verbatim.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectConfigV1 {
    pub schema: u32,
    pub project: ProjectIdentity,
    pub startup: StartupSettings,
    pub rendering: RenderingSettings,
    pub controls: ControlSettings,
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

/// Reserved for the roadmap's Play settings. `None` means not configured; the editor
/// renders these rows disabled until their runtime support ships.
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
    Schema { expected: u32, found: u32 },
    Value(&'static str),
    NoConverter(u32),
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => write!(f, "project file exceeds {MAX_FILE_BYTES} bytes"),
            Self::Decode(error) => write!(f, "project file is not valid JSON: {error}"),
            Self::Schema { expected, found } => {
                write!(f, "project schema {found} needs conversion to {expected}")
            }
            Self::Value(field) => write!(f, "invalid project field {field}"),
            Self::NoConverter(schema) => write!(f, "no converter from project schema {schema}"),
        }
    }
}

impl std::error::Error for ProjectError {}

/// Whether [`convert`] has a converter from this schema to the current one: it is an older
/// schema this crate knows. The current schema needs none, and a newer or unknown one has
/// none. A caller that offers conversion asks this first, so that it never offers one that
/// cannot be done.
pub fn can_convert(schema: u32) -> bool {
    (1..SCHEMA).contains(&schema)
}

/// Reads a bounded current-schema project manifest. It never performs conversion or I/O.
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
    let keys = [
        config.controls.move_forward.as_str(),
        config.controls.move_backward.as_str(),
        config.controls.move_left.as_str(),
        config.controls.move_right.as_str(),
        config.controls.jump.as_str(),
        config.controls.toggle_view.as_str(),
        config.controls.release_cursor.as_str(),
    ];
    if keys
        .iter()
        .any(|key| key.trim().is_empty() || key.len() > 24)
    {
        return Err(ProjectError::Value("controls"));
    }
    if let Some(clients) = config.planned.multiplayer.clients
        && !(1..=4).contains(&clients)
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

/// Converts one version at a time. Schema 1 to 2 adds reserved, inactive Play and gameplay
/// groups; the project's live settings and level selection are preserved.
pub fn convert(value: serde_json::Value) -> Result<ProjectConfig, ProjectError> {
    let schema = value
        .get("schema")
        .and_then(serde_json::Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(ProjectError::Value("schema"))?;
    match schema {
        1 => {
            let old: ProjectConfigV1 = serde_json::from_value(value)
                .map_err(|error| ProjectError::Decode(error.to_string()))?;
            if old.schema != 1 {
                return Err(ProjectError::Schema {
                    expected: 1,
                    found: old.schema,
                });
            }
            let mut converted = ProjectConfig::new(old.project.name.clone());
            converted.schema = 2;
            converted.project = old.project;
            converted.startup = old.startup;
            converted.rendering = old.rendering;
            converted.controls = old.controls;
            validate(&converted)?;
            Ok(converted)
        }
        SCHEMA => {
            let config: ProjectConfig = serde_json::from_value(value)
                .map_err(|error| ProjectError::Decode(error.to_string()))?;
            validate(&config)?;
            Ok(config)
        }
        other => Err(ProjectError::NoConverter(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn conversion_dispatches_by_schema_and_migrates_v1_without_losing_live_settings() {
        let value = serde_json::to_value(ProjectConfig::new("Arc Game")).unwrap();
        assert_eq!(
            convert(value.clone()).unwrap(),
            ProjectConfig::new("Arc Game")
        );
        let mut old = ProjectConfig::new("Arc Game");
        old.schema = 1;
        let old = ProjectConfigV1 {
            schema: 1,
            project: old.project,
            startup: StartupSettings {
                start_level: "levels/legacy.level.json".into(),
            },
            rendering: old.rendering,
            controls: old.controls,
        };
        let converted = convert(serde_json::to_value(old).unwrap()).unwrap();
        assert_eq!(converted.schema, 2);
        assert_eq!(converted.startup.start_level, "levels/legacy.level.json");
        assert_eq!(converted.controls.jump, "Space");
        assert!(converted.planned.multiplayer.clients.is_none());

        let mut unknown = value;
        unknown["schema"] = 0.into();
        assert_eq!(convert(unknown), Err(ProjectError::NoConverter(0)));
    }

    #[test]
    fn only_an_older_known_schema_is_one_that_can_be_converted() {
        assert!(can_convert(1));
        // The current schema needs no conversion, and no other has a converter.
        assert!(!can_convert(SCHEMA));
        assert!(!can_convert(0));
        assert!(!can_convert(SCHEMA + 1));
        // What is said to be convertible is converted, and what is not is refused.
        let mut newer = serde_json::to_value(ProjectConfig::new("Arc Game")).unwrap();
        newer["schema"] = (SCHEMA + 1).into();
        assert_eq!(
            convert(newer.clone()),
            Err(ProjectError::NoConverter(SCHEMA + 1))
        );
        assert_eq!(
            decode(&serde_json::to_vec(&newer).unwrap()),
            Err(ProjectError::Schema {
                expected: SCHEMA,
                found: SCHEMA + 1
            })
        );
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
        config.planned.multiplayer.clients = Some(5);
        assert_eq!(
            validate(&config),
            Err(ProjectError::Value("planned.multiplayer.clients"))
        );
    }
}
