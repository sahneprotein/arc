# Roadmap: the Arc editor and the game, to production

Written on 2026-10-25. It replaces `history/2026-10-09-play-and-production-roadmap.md`,
which stays as the record of what was built and checked up to 2026-10-13, with its evidence.
This document holds the order of the work from now on and what each step must show before
it counts as done. What is decided says so and names who decided it and when; everything
else is proposed.

The product is two things that ship together: the game, a server-authoritative zoned online
game with real-time combat (repository `arc-game`, design in its
`docs/superpowers/specs/2026-10-03-foundation-design.md`), and the editor its levels are
made in (the cargo workspace `editor/` of the engine fork `arc`).

## Decided

- **One project file, `.aproj`, and no legacy.** A project is the folder of its `.aproj`
  file; the file holds everything the project is, and the editor starts from it. No project
  exists yet, so nothing older is read, converted or kept compatible: the editor and the
  game know `.aproj` at schema version 1 and refuse anything else. Owner, 2026-10-25.
- **Every file format has one version.** The level, the cooked package (manifest, simulation
  and visual payloads), the emitter, the look profile and the editor's settings are each at
  version 1, as `.aproj` is, and refuse every other: no older version is read, no document
  is upgraded from one version to another, and no version is chosen by what a file holds.
  Owner, 2026-10-25.
- **Server authority.** The server accepts inputs, never positions; every new message,
  replicated component and handshake field is reviewed against that (the game's `CLAUDE.md`,
  "Server authority").
- **What the game loads is a cooked package**, identified by the hash of its manifest,
  verified before use, and compared by client and server in the handshake. The editor's
  source files are never read by the game.
- **Evidence.** A step is done when what it claims was run and the run is recorded: the
  command, the build it ran on, the counts, and what was not run. Something that is reasoned
  but not run is said to be reasoned.

## Where things are on 2026-10-25

| Repository | Branch | Holds | State |
|---|---|---|---|
| arc | `arc/0.19` at `bd0883094` | The engine, Bevy 0.19.1 and the carried patch; the game's pin | Mainline |
| arc | `codex/play-backend` at `fda0ec1ff` | The editor: levels, landscape sculpt/paint/rules, objects with multi-selection and snapping, review, particles tool with GPU preview, Game look, project browser and settings, Play in the game's window with local test clients | Pushed on 2026-10-26, not merged. Holds `codex/aproj` (`.aproj`, opening a project by its file, switching projects) and the one-version formats |
| arc-game | `main` at `3657f21` | Foundation (workspace, arc pin, xtask gates, CI, netcode skeleton, bots and load test), Pip character presentation, aim pitch | Mainline |
| arc-game | `codex/cooked-level-play`, then `codex/play-readiness` at `7dd510d` | Cooked packages with terrain, paint and collision; the look profile; particle placements; project settings; readiness reported to the editor | Pushed, not merged |
| arc-game | `codex/aproj-game`, over `codex/play-readiness` | The client reading `.aproj`, the one cooked schema, the pin of `project-spec` at `fda0ec1ff`, Clippy mended | Pushed on 2026-10-26, not merged |
| arc-game | `feat/foundation-hardening` at `ec8c595` | One commit of 2026-10-08: further xtask gates (`check-assets`, `check-docs`, `doctor`), the art pipeline's regression checks, a rewrite of the client's presentation (`crates/client/src/pip/`) | Pushed, merged nowhere, and not in `main`, which took aim pitch on the earlier presentation. About 17,500 lines added; an owner decision (M0) |
| lightyear (upstream) | PR #1808 | A bound on fragmented message size, for our issue #1776 | Open upstream; the game pins lightyear `=0.30.1` |

## Next, scheduled on 2026-10-26

Work that waits on no owner decision, in this order. Each item is done when its checks
are run and recorded in the milestone it belongs to.

1. **The game's `check-arc` knows the editor's crates** (M0.4; Codex). The game pins
   `project-spec`, `particle-spec` and `particle-runtime` from arc by their own commit, and
   `check-arc` refuses every crate from arc that is not an engine crate of `arc-pin.toml`.
   It fails for that reason on `codex/play-readiness` and after. It should accept those
   crates at a commit of their own, and still refuse an engine crate that is not at the
   pinned commit.
2. **The hello window** (M0.4; Codex, after 1). Three `protocol` tests fail
   (`handshake client_without_hello_is_dropped` and two `slow_client` tests): the server
   drops a client that never sent a hello only after 7.95 seconds. Find what the window
   is meant to be and mend the code or the tests.
3. **M1's exit as a scripted case** (Claude). A project is made, played, left for another
   and opened again by its `.aproj`, in one run of `apps/arc-editor/checks/run.sh`. No
   code names `arc-project.json` any more; only this document does.
   (Done at `0ad9e5c3b`: the case `project-cycle`, which passed 3 of 3 runs. For it, a
   check's stand-in for the game now plays every project the editor enters, `5cd91c852`;
   after that the groups `play-` 34, `project-switch` 18, `browser-` 27, `close` 20,
   `game-` 26 and `key-` 18 pass, and `editor-ui` with `arc-editor` 305 of 305.)
4. **`fold-not`** (M0.4; Claude). At 1100 logical pixels the toolbar folds where the case
   expects one row: mend the layout or the case.
   (Done at `85b3ec3e9`: the tools fold by measurement when the row is longer than the
   window, which the row with the Particles tool is at 1100 and not at 1150, so the case
   asks for 1150. The fold group passes 6 of 6.)
5. **M2.1, the real run with two local test clients** (a person, with the display awake),
   once 1 to 4 are in.

## Milestones

Each milestone ends with its work merged into the mainlines, the checks of both
repositories green on the merged commits, and its exit criteria recorded as evidence. A
later milestone may start in parallel only where it touches nothing the earlier one changes.

### M0: one line of history (the gate for everything after it)

1. Decide `feat/foundation-hardening`: what of it is taken onto `main` (proposed: the xtask
   gates and the art pipeline checks, each as its own commit on today's `main`; the
   presentation rewrite only if it survives a review against aim pitch, otherwise dropped
   and the branch deleted).
2. Name the editor's mainline in arc (proposed: `editor/0.19`, which the editor's README
   already calls its development branch) and merge `codex/play-backend` into it.
3. Merge `codex/cooked-level-play` and `codex/play-readiness` into the game's `main`.
4. Run the full checks on the merged commits: in the editor its CI (format with the explicit
   package list, `check`, `test`, `clippy -D warnings`, on Linux, macOS and Windows) and the
   whole scripted suite (`apps/arc-editor/checks/run.sh`); in the game `cargo xtask ci`.
   Mend what fails. On 2026-10-25, on `codex/play-backend` at `243ffb3ae` and on macOS
   only: Clippy passes; the workspace tests, run once whole, passed 571 and failed 1, a
   level schema test that was stale and is mended since, its package then 21 of 21 (the
   Play test whose stand-in client ended before it was ready was mended before that run);
   the whole scripted suite passes 457 of 458. The one failure, `fold-not`, also fails
   on the tip from before that day: at 1100 logical pixels the toolbar folds, where the case
   expects one row. Linux and Windows were not run.
   On 2026-10-26, after the formats were made one version each (merge `ff62b1815`), on macOS:
   Clippy over the workspace passes; tests by package pass, `project-spec`, `scene-data`,
   `editor-core`, `editor-cook` and `editor-services` 239 of 239, `editor-ui` and
   `arc-editor` 305 of 305; the scripted groups that cook, play, place emitters and switch
   projects pass. `landscape-render`, `particle-runtime`, `particle-spec` and `ui-spike`
   were not run after that merge, which changed none of their files: on this machine
   their own engine build did not finish within two hours. A whole-workspace run needs a
   faster machine or CI.
5. Move the game's pin of `project-spec` to the editor's merged commit.

Exit: no `codex/*` branch holds work that is on no mainline; both mainlines pass their full
checks; the pins name commits on mainlines.

### M1: the project model

1. `.aproj` at schema 1 in `project-spec`, with the one rule that finds a project's file in
   its folder, used by the editor and the game alike; every legacy path removed: the reading
   and conversion of `arc-project.json`, schema migrations and their backups, the browser's
   and the settings' conversion flows and their words and cases. (Built: the editor's side
   on `codex/aproj`, merged into `codex/play-backend` at `eed04bbe1`; the game's side on
   `codex/aproj-game` at `fda83a5`, its pin of `project-spec` moved to `fda0ec1ff` on
   2026-10-26.)
2. The editor opens a project by its file: the browser's "Open Project" takes an `.aproj`,
   and `arc-editor <file.aproj>` starts on it. (Built at `3c164b7c5`. A person can no longer
   set up a folder that is not a project yet from the browser, since the file dialog chooses
   files; "New Project" makes one. Whether setting up a folder is kept is the owner's.)
3. Going from one project to another from the Level menu, with the questions closing asks.
   (Built at `56a4a6eaf`, with the fix at `05d48ca30` for a race that let closing or leaving
   go on while a game's server was left behind.)
4. The other legacy of the editor's start, for the owner to decide (proposed: remove both):
   - the start with `--level`, `--package` and `--game` "as the editor always started",
     without a project, which every scripted case uses today; the cases would start on a
     project made for them instead;
   - the game's old built-in map as the editor's level without a project, and
     `arc-scene convert-current-map`.
5. Opening an `.aproj` from the desktop (file association), once the editor is a packaged
   application (M5).

Exit: a new project is made, played, left for another and opened again, all through its
`.aproj`, by a scripted run and once by a person; no code or text names `arc-project.json`.

### M2: the Play loop, complete

1. A real run from the editor with two local test clients, both windows shown, Stop ending
   the server and both clients with status 0.
2. A person uses the editor from the desktop, with nothing named: the browser, the folder
   and file dialogs, a new project, Play, Stop, closing. What they find is mended.
3. A person compares the editor's states (Play control, notices, dialogs, Project
   settings) with the Claude Design system "Arc Editor"; what differs is mended or recorded
   as a design change.
4. Preview in the editor's own 3D view (P2 of the history document): built or deferred, by
   the owner's decision; the game's window stays the way to play with several clients.

Exit: the evidence of 1 to 3, with pictures that a person looked at.

### M3: content

1. Asset references in the scene and cooked formats; published GLB models placed in levels;
   an asset browser with load and validation problems, import and reimport. Authored sources
   stay out of what the game loads.
2. A level looks the same in the editor's Game view and in the client: the same meshes,
   materials, lighting, sky, shadows, exposure and tone mapping, shown by captures compared
   side by side with the display awake.
3. Particles: an emitter placed in a level and played in the client; the editor's and the
   client's captures compared; the adapter's refused fields (sphere, cone and box shapes,
   drag) drawn exactly or kept refused.
4. Material and lighting previews, then scene lights and environment once the scene and
   cooked formats carry them and the client reads them.
5. How a cooked level ships with the emitters and assets it uses.

Exit: one level with models, emitters and lighting, authored in the editor, played in the
client, and compared by picture.

### M4: the game's systems, and authoring for them

The order of the foundation design's sub-projects 2, 3 and 5, with what the editor adds to
each.

1. Gateway, accounts, server-issued connect tokens, character persistence in Postgres. The
   development key stops being accepted outside development.
2. Several zones: a zone directory, handoff between zone servers, interest management
   (today every client receives every entity of its zone), orchestration. In the editor:
   zone exits, safe areas and triggers as cooked volumes.
3. Combat with lag compensation (the 20-tick rollback limit of lightyear is the known
   constraint), first-person weapon use, and combat overlays in the editor.
4. Landscape at the size of a zone, once the owner has said how large a zone is and how fine
   its ground.
5. Prefabs with overrides, once the core has a reference and override model.

Exit per item: its load test or integration test run against the budget it states.

### M5: production

1. **Distribution.** The editor and the client as signed, installable applications for the
   platforms the owner chooses (macOS signing and notarization, Windows signing); the server
   image the `server-image` workflow builds; reproducible release builds from tags.
2. **Versions and compatibility.** The protocol fingerprint as now; a written rule for
   changes to `.aproj`, the scene format and cooked packages, now that projects will exist:
   every change after schema 1 comes with an explicit, tested migration.
3. **Security.** Connect tokens (M4), the authority review of every boundary change,
   `cargo-deny` as now, and lightyear updated to a release with the bound of PR #1808, with
   our server's own limit set.
4. **Observability.** Metrics and dashboards for the servers (Prometheus and Grafana, as
   planned), crash reports from the client and the editor, logs that can be collected.
5. **Budgets.** Players per zone against the tick budget (measured today: 100 bots within
   it, 200 not, on the development machine), the client's frame time with many characters
   in a window, the editor's frame, cook and Play-start times, each on a named reference
   machine.
6. **Deployment.** Docker Compose for the first servers, then Agones on k3s when one machine
   is not enough.
7. **Checks that are missing today.** The editor's scripted suite in CI under a virtual
   display; a Linux aarch64 runner (deferred by the owner); the game's client checked in a
   window by CI.

Exit: a tagged release installed from its artifacts on a clean machine connects to a
deployed server and plays a level made in the editor.

## Open for the owner

1. `feat/foundation-hardening`: take which parts onto `main` (M0.1).
2. The editor's mainline branch in arc (M0.2).
3. The editor's start without a project, and the game's old map in the editor (M1.4).
4. Of the `.aproj` contract: a new file always named `project.aproj` and renamable
   (proposed), one file per folder with several refused (proposed), the number of local test
   clients kept in the file (proposed), the game given the folder as now (proposed).
5. Preview in the editor before several clients, or after (M2.4).
6. The platforms the editor and the client are released for (M5.1).
7. Carried over from the history document: whether Gameplay markers are designed before the
   game's multi-zone work, whether the toolbar's tools become workspaces, whether combat
   overlays come before combat, and how large a zone is.
