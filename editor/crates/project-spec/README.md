# Arc project file

`project-spec` is the renderer-independent project contract shared by editor and
game. The unique case-insensitive `.aproj` in a folder defines that project;
setup writes `project.aproj`, and renaming is allowed.

Schema 1 is closed, bounded JSON. Other schemas and unknown fields fail closed.
The project name, start level, look profile, keys/look bindings, local test client
count and reserved multiplayer/gameplay fields live here. Personal editor
settings and generated output do not. See [the complete contract](docs/aproj.md).

`resolve_project_path(root)` returns the unique entry or the default setup
destination when none exists. `project_root_from_file(path)` checks a selected
entry and returns its canonical parent without decoding. Both are read-only;
`DiscoveryError` has a stable code, path and optional filesystem detail. Loaders
must check file status and use `decode` before running. `canonical_bytes` validates
and serializes schema 1 deterministically.

New-project setup selects the first valid existing level. If none exists it
creates an authored starter at `levels/first-level.level.json` (or the next unused
name). Rejected existing levels require the person's agreement to make a starter
beside them. Existing look profiles and levels are preserved. Inspection never
writes; only explicit setup and saves write.
