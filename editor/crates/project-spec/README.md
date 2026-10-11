# Arc project manifest

`arc-project.json` is the project-root manifest. Schema 2 is strict JSON with these active
sections:

- `project.name`
- `startup.start_level`, a project-relative level path
- `rendering.profile`, a project-relative path to the game's versioned look profile
- `controls`, the current WASD, mouse/gamepad look, jump, view-toggle and cursor-release
  bindings

It also contains `planned.multiplayer` (clients, bots, network conditions, start location,
zone) and `planned.gameplay` (mode, teams, respawn, weapon set). Those values are `null` in a
new project and are not read by the runtime. The editor should render those groups disabled
until their corresponding game/coordinator work is implemented.

Schema 1 -> 2 is an explicit converter: it preserves the active values and adds those empty
planned groups. `project-spec::convert` handles the in-memory conversion; editor-services
backs up the original bytes as `arc-project.json.schema-1.bak` before replacing a manifest.
Opening a project never invokes the converter or writes to disk. Unsupported future schemas
are refused until a converter is added: `project-spec::can_convert` says which schemas have a
converter, and editor-services reports an older, convertible manifest as
`project.convert_required` and any other schema as `project.schema_unsupported`, for which no
conversion is offered. A conversion holds the manifest with a `.lock` file beside it and
checks, just before replacing it, that it is still the bytes that were converted; if another
program wrote it meanwhile nothing is replaced (`project.changed`).

New-project setup selects the first valid existing level, if one exists. Otherwise it creates
the current authored starter map at `levels/first-level.level.json` (or the next unused
numbered name), using create-new writes so existing files are never replaced. The name and
the whole manifest are validated before anything is created. `plan_setup` says, by reading
only, which level a setup would start with and which level files it rejects, each with its
reason; a folder whose level files are all rejected is set up with a fresh starter only when
the caller says the person agreed (`FreshStarter::Agreed`), and is otherwise refused
(`project.levels_rejected`) with nothing created.

Everything setup, loading and conversion read or write stays inside the project folder as the
file system resolves it. A manifest, a level or a `levels` folder that is a link to somewhere
outside is refused (`project.outside_folder`), not followed. The default
render profile path points to the existing `assets/render/look-v1.json`; setup leaves those
bytes untouched.
