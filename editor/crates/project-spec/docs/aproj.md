# The project file

A project starts at its `.aproj` file. The file's parent folder is the project
root; levels, look profiles and other authored assets use paths relative to that
root. Moving the folder together keeps those references intact. The project file
does not embed level or asset bytes.

A project folder has exactly one `.aproj` directly inside it. Setup writes
`project.aproj`. Renaming is allowed independently of the project's display name.
The extension is case insensitive on every platform. Subfolders are not searched.
Several matching entries, including directories and links, are refused as
`project.multiple_files`. No entry is preferred by name or directory order.
Without a matching entry the folder is not a project yet: set it up, or make a
new project in its own folder.

The closed JSON format starts at schema **1**. Every other schema and every
unknown field is refused. Canonical output uses two-space pretty JSON, the typed
field order and one trailing newline. Files are bounded to 32 KiB. The fields are:

- `schema`, `project.name`, `startup.start_level`, `rendering.profile`.
- `controls`: seven distinct supported key names and mouse/right-stick look bindings.
- `planned.multiplayer`: local test `clients` (null means one; otherwise 1–4),
  and reserved `bots`, `network_conditions`, `start_at`, `zone`.
- `planned.gameplay`: reserved `mode`, `team_count`, `respawn_seconds`, `weapon_set`.

The reserved fields have no runtime effect until their feature ships. New projects
write them as null. The game's installation and executable locations are personal
editor preferences. Recents, window state, drafts, caches, cooked output and Play
logs are not project-file fields. Local test client count remains shared so a
project can reproduce its test setup.

`project_spec::resolve_project_path(root)` owns discovery for editor and game.
It returns the unique entry, or `root/project.aproj` as the prospective setup
destination when none exists (including a root not created yet). Callers loading
a project must check existence and regular-file status and decode before use.
Directories are counted for ambiguity but refused as `project.not_a_file` on
open. A link to a regular file inside the root is accepted; a link outside the
root or a dangling link is refused as `project.outside_folder`.

`project_spec::project_root_from_file(path)` checks extension, uniqueness,
containment and regular-file status, then returns the canonical parent. It does
not decode contents. `DiscoveryError` carries stable `code`, `path` and optional
I/O `detail`: `project.read`, `project.path_invalid`, `project.not_a_file`,
`project.multiple_files`, `project.outside_folder`. The editor-services wrapper
additionally loads and validates the file before returning the root.

Looking never writes. Setup is explicit. Settings saves resolve the current
filename and retain cooperating-writer locks, changed-since-read checks and
atomic replacement. Play preflight reads only the unique `.aproj` and validates
its start level and look profile. `--project-root` remains the game contract;
the client uses the same discovery and schema reader and refuses a folder
without exactly one usable project file.

## Questions for the owner

- Should setup name the file after the project? Recommendation: `project.aproj`,
  with renaming allowed, so changing a display name never renames a file.
- Should several project files share a folder? Recommendation: exactly one, so
  folder-based game launches and recents remain unambiguous.
- Should local test client count become personal? Recommendation: keep it in the
  project file to share the test setup.
- Should the game receive the file instead of `--project-root`? Recommendation:
  retain the root argument and share the unique-file rule.
