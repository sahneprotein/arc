# Play and Stop, and how the editor's features are grouped

A proposal, 2026-10-09, brought up to date on 2026-10-10 with what has been built since.
Nothing proposed in it is decided. It has two parts: how starting and stopping the game should look and behave in the editor, and how
the editor's features should be grouped as it grows into the production tool for a zoned
online game with real-time combat. Each part says first what exists today and then what is
proposed, and the two are never mixed in one table.

Sources:

- `crates/editor-services/docs/play-boundary.md`: the agreed contract between the UI and the
  editor core for Play. Its first half is implemented (checking a cooked package); its second
  half is a documented seam and not code.
- The Claude Design system "Arc Editor": the pages PlayControl, Menu, StatusBar, Notice and
  Dialog, and `layouts.md`; the canvas boards "Play: the control in each state", "Playing:
  preview in the editor", "Playing: the game in its own window", "Closing the editor during
  a run".
- The game's foundation design (`arc-game`, `docs/superpowers/specs/2026-10-03-foundation-design.md`):
  zoned shards with one server process per zone, client-predicted movement with server
  reconciliation, and a roadmap of accounts, multi-zone, art pipeline, combat and deployment.
- The editor as it is on the branch `codex/play-workflow`.
- The game as it is on its branch `codex/cooked-level-play` (commit `df87d04`), with that
  branch's own account in `docs/cooked-package-runtime.md`. It is not merged into the game's
  `main`.

Words used here. The **open level** is the level as it is in the editor now, with every
change made to it; the **saved file** is what the last Save wrote, and the two differ while
there are unsaved changes. A **cooked level** is the package the cook step makes from the
open level as it is at that moment, saved or not; it is what the game loads, and it is named
by a hash. A **receipt** is the core's record that
a cooked package on disk was read and verified. A **run** is one start of the game from the
editor, from the request to the end of its cleanup.

## Part 1: Play and Stop

### 1.1 What exists today

| Piece | Where | State |
|---|---|---|
| Cook | editor | Built. Level menu and the Play menu's foot; the status bar names the cooked level |
| Checking a cooked package (`VerifyPackage`, `VerifiedPackageReceipt`) | editor core and UI | Built. Asked for from the Play menu's row "Check cooked level"; a refusal is a notice in words, and Stop cancels a check that is running |
| The Play control (P0 of 1.11) | editor UI | Built. At the right end of the toolbar at every width. Its menu holds the two ways to play, switched off, with the reason in its foot; the foot names the cooked level, says when the level has moved on from it, and offers "Cook" or "Cook first". It never shows a game as starting or running |
| The game running a cooked level | game, branch `codex/cooked-level-play` | Built for levels of boxes and spawn points. Server, client and `cargo xtask run-local` take `--cooked-package <folder> --cooked-package-hash <manifest hash>`. Each process verifies the package itself before it opens a port or a window, and builds collision, spawn points and the client's boxes from it |
| The same level on both sides | game, the same branch | Built. The client's hello carries the hash of the package's manifest, the server accepts only its own, and the client checks the hash the server answers with. A cooked level against the built-in one, or against another cooked level, is refused before a character exists |
| Landscape in the game | game | Not built. A cooked level that has a landscape is refused when the game starts, in words; nothing is dropped silently |
| Starting the game from the editor | editor core | Not built: no Play coordinator (`StartPlay`, `StopPlay`, `play_snapshot`, the waker), no process launch or supervision, no readiness reported back. This is P1 |
| Running the game inside the editor's view | editor and game | Not built. P2 |
| Design | Claude Design | Drawn: five boards and the PlayControl page. P0 follows them where it can; the Play menu is as wide as any menu, because the design has no token for the wider menu it draws |

So the two ends exist and the link between them does not. A person can cook in the editor
and have the cooked level checked; the game's `run-local` takes a cooked level's folder and
hash on its command line. Nothing in the editor starts the game yet, and the two have not
been run together.

What the game branch says of itself, and this note does not claim more: its loader and
handshake tests pass; it has not been run live against a package cooked by the editor, has
not been looked at on screen, and has not been through the game's full checks.

### 1.2 What the contract fixes, in the UI's terms

These are not open; the proposal below only arranges them.

1. Play always starts from a verified cooked package, never from the level as it is being
   edited and never from a brush preview.
2. Checking the package is a job like Save or Cook. Starting and stopping are not: they are
   requests to a separate coordinator, they are not undo steps, and each gets exactly one
   answer, a refusal included.
3. There are two ways to play: inside the editor (`in_editor`) and as the game's own window
   (`external_game`).
4. A run is Starting, Running, Stopping or Ended. It is Running when the game is really
   ready, not when its process exists. It is Ended once, after the server the editor started
   has been cleaned up or the wait for that has run out.
5. Editing goes on during a run and never reaches the running game.
6. How a run ended and whether the server was cleaned up are two separate facts. A failed
   cleanup never hides a crash.
7. The editor is woken for every change of a run, also when it is otherwise asleep.
8. Closing the editor asks the run to stop and waits for its end.

### 1.3 Where Play lives in the window (proposed)

| Place | Holds |
|---|---|
| Toolbar, right end, before the panel switches | The Play control, at every window width. It never folds into another menu |
| The control's menu | The two ways to play, each said in full, and a foot that names the cooked level |
| Status bar | The run in words while it lasts, and how it ended when the person asked for that end |
| Notice, in the 3D view | Every end the person did not ask for, and a server that did not shut down |
| 3D view | In-editor play: the game, inside a frame in `signal-play`. Game window: the level as before, with a small box saying the game window is open |
| Dialog | Closing the editor during a run |

One control, one Stop, one place. Stop is never inside a menu and never moves between states.

### 1.4 The control in each state (proposed)

The left column is what the core reports; the rest is what the editor shows.

| Core reports | The control | Stop | Status bar |
|---|---|---|---|
| No run; a cooked level that matches the open level | "Play", opens the menu | none | "cooked 9f3a61c2" |
| No run; the level changed since the cook | "Play" with a warning dot | none | "cooked 9f3a61c2, older than the level" |
| No run; nothing cooked | "Play", triangle dim; both ways to play are off | none | "not cooked" |
| `VerifyPackage` running | "Checking" and the hash | cancels the check | "checking cooked 9f3a61c2" |
| Starting | "Starting" | stops | one line per process as it becomes ready (1.6) |
| Running, `in_editor` | "Playing in editor", in `signal-play` | stops | "playing cooked 9f3a61c2" |
| Running, `external_game` | "Game window open", in `signal-play` | stops | the same |
| Running; the level's source hash is no longer the run's | unchanged | stops | "playing cooked 9f3a61c2, older than the level" |
| Stopping | "Stopping" | shown, off | "stopping the game" |
| Ended | "Play" again | none | how it ended, if the person asked for it (1.7) |

In a window narrower than the toolbar's fold width the word may shorten to its dot; Stop
stays, and the status bar carries the words.

### 1.5 Starting (proposed)

1. The person opens the Play menu and chooses "Preview in Editor" or "Open Game Window".
2. If a brush stroke or a gizmo drag is open, it is committed first. The contract allows the
   start to need that; a refusal `transaction.busy` must never be what the person sees.
3. The editor asks for `VerifyPackage` with the identity it remembers from its last cook.
   The control reads "Checking".
4. On success the editor sends `StartPlay` with the mode and the receipt. On failure nothing
   starts and a Notice says what is wrong with the package, with the core's own code.
5. The control reads "Starting" until the core reports Running.

The menu's foot always says what would run, before anything is pressed:

| Situation | Foot | Its one button |
|---|---|---|
| Cooked, current | "cooked 9f3a61c2" | none |
| Cooked, older than the level | "The level has changed since this cook. Play runs the cooked level, without those changes." | "Cook first" |
| Not cooked | "Cook the level to play it." | "Cook" |
| Unsaved changes, cooked | the second row's sentence; saving is not forced | "Cook first" |

"Cook first" cooks the level as it is open in the editor, saved or not, and leaves the menu
open on the new hash. It does not save the level, and it does not start the game by itself:
the core cooks the open level, not the file. Playing an older cook stays possible on purpose:
it is the fast way to look at something again.

### 1.6 What "ready" shows (proposed)

A zone server and a client become ready in steps, and a start that hangs must say where.
While a run is Starting, and on request while it is Running, the status bar's run segment
opens a short list, one line per process:

| Line | Meaning |
|---|---|
| Server: level verified, collision ready | the server decoded the package and installed collision; it listens only after this |
| Client: level verified, collision ready | the same in the client; it says hello only after this |
| Client: accepted by the server | the handshake found the same code and the same level on both sides |
| Client: simulating | the client's clock is in step and it predicts |
| Client: level drawn | what is needed to draw the level is loaded; separate from collision |
| Client: first controllable frame | the player can move. This is when the run counts as Running |

A step that has no measurement shows as absent, never as done. The names of these steps come
from the editor plan's shared model of "ready"; the core has to deliver them (1.10).

### 1.7 How a run ends (proposed)

| Core's outcome | Asked for? | Where it is said | Words |
|---|---|---|---|
| `play.stopped` | yes | status bar | "Stopped" |
| `play.window_closed` | yes | status bar | "The game window was closed" |
| `play.crashed` | no | Notice, stays until dismissed | "The game stopped unexpectedly", exit code or signal, the log's path, "Open log", "Play again" |
| `play.start_failed` | no | Notice, stays | "The game did not start", the reason, the log's path |
| `play.server_cleanup_failed` | no | a second Notice, beside the first, never in its place | "The game's server did not shut down", the process number, "Try again" |

Refusals are one sentence each, in the status bar, because nothing happened:

| Core's code | Sentence |
|---|---|
| `play.already_active` | "A game is already running. Stop it first." |
| `play.package_required` | "Cook the level before playing." |
| `play.no_active_run` | nothing is shown: Stop was pressed as the run ended |
| `play.stale_run` | nothing is shown: the Stop belonged to a run that is over |
| `package.identity_mismatch`, `package.directory_identity`, `payload.hash_mismatch` | Notice: "The cooked level on disk is not the one this editor cooked. Cook again." with the field that differs |

### 1.8 Editing during a run, and closing (proposed)

- Editing, saving and cooking all stay available. A cook during a run makes a new package;
  the run keeps its own. When the open level moves on, one Notice offers "Cook and play
  again", which stops the run, cooks, checks and starts in the same mode.
- In-editor play: a click in the view gives the game the pointer and the keyboard; Escape
  takes them back. While the game has them, the editor's shortcuts and tools do nothing, as
  behind a menu. The view shows one line saying so. The landscape brush and the gizmo are
  not drawn into the game's picture.
- Closing the editor during a run: the usual question about unsaved changes first, with a
  line that a game is running; then a dialog "Stopping the game" that stays until the core
  reports Ended; and only when the server will not go, a last question with "Close anyway".

### 1.9 Play for an online game (proposed, beyond the contract)

The contract describes one game and its server. A zoned online game with real-time combat
is tested with more than that, and the game repository already has the pieces as a developer
command (`cargo xtask run-local --clients N --latency-ms --loss`). The proposal is to offer
them from the Play menu as **play settings**, remembered per level, shown in the menu's foot
beside the hash:

| Setting | Why a level designer needs it | Needs from outside the UI |
|---|---|---|
| Number of clients (1 to 4) | sightlines, spawn spacing and anything two players do to each other cannot be judged alone | the coordinator supervising several clients; a run is Running when all are ready |
| Bots (none, a few, a crowd) | how a place reads when it is full; where players collide | the game's bot swarm as a child of the run |
| Network conditions (none, typical, poor) | whether a jump, a doorway or a fight still works with delay and loss | the game's simulated latency and loss, set at start |
| Start at (a spawn point, or where the editor's view is) | testing one corner of a large zone without walking there | the game accepting a start position for a local run; never in a shipped build |
| Which zone | later, when a level is one zone among several | the multi-zone work of the game's roadmap |

These are settings of a run, not of the level: they are not saved into the level file and do
not change its hashes. Each is a separate piece of work and none is needed for the first Play.

### 1.10 What the UI needs from the core

Already in the contract: the two control requests with one answer each, `play_snapshot()`,
the waker, the outcome codes with exit facts and the log's path, and the receipt.

Asked for in addition, for the core's writer to accept or change:

1. The readiness steps of 1.6 as facts in the play status, each with the process it belongs
   to and absent until measured.
2. The run's own source hash in the play status, so that "older than the level" is a
   comparison of two values the core holds and not something the UI works out.
3. For 1.9, the list of processes of a run (server, clients, bots) with one state each, so
   that one client crashing is not shown as the whole run crashing.
4. A way to cancel a start that hangs before Running, with the same single Ended.
5. The launch itself as the game expects it: the receipt's folder, as a native path, and
   the hash of its manifest (`receipt.identity().manifest_hash`), passed as
   `--cooked-package` and `--cooked-package-hash` to the server and to every client, the
   same to each.
6. A refusal before anything is started when the cooked level holds a landscape, as long as
   the game cannot run one: `play.start_failed` after a process has died is a worse answer
   to a question the core can settle from the package.

One thing to settle with the game's side before P1: the editor names a cooked level to the
person by the first eight digits of its simulation hash, and the game names it on the wire
by the hash of its manifest. They are two hashes of the same package. Either the editor
shows the manifest's, or both are shown where a mismatch is reported; otherwise the hash in
a refusal from the game will not be the one in the status bar.

### 1.11 Order of work (proposed)

| Step | The person gets | Needs |
|---|---|---|
| P0, built | The Play control with its menu and foot; "Cook first"; "Checking" against the real `VerifyPackage`; the two ways to play shown as not available yet, with the reason | nothing new from the core |
| P1 | Open Game Window: Starting, Running, Stop, every end of 1.7, closing during a run | in the editor core: the coordinator, process launch and supervision, readiness (1.10). In the game: nothing more for levels of boxes and spawn points, once its branch is merged; landscape for levels that have one |
| P2 | Preview in Editor: the game in the 3D view, the pointer handed over and back | the game's client able to run inside the editor's window |
| P3 | Play settings of 1.9, one at a time | per setting, see its row |

### 1.12 Open, for the owner

- The keys. F5 and Shift+F5 are assumed on the design pages and not decided.
- Whether "Cook first" should start the game straight away. Proposed: no.
- Whether a level with unsaved changes may be played from its older cook without a question.
  Proposed: yes; the foot says what will run.
- Whether in-editor play is worth building before several clients. The game window gives
  several clients almost for free; the preview does not.

## Part 2: Grouping the editor's features

### 2.1 What exists today

Everything here is on the branch and covered by the scripted checks, except where it says
otherwise.

| Group | Implemented |
|---|---|
| Level | New, Open, Save, Save as, Cook; unsaved-changes questions; a failed save stays visible with "Try again"; one folder of levels |
| Objects | Boxes and spawn points: add where the view looks (on the landscape if it is there), select in the list or the view, move, rotate and scale with the gizmo, rename, type position, rotation and size, remove; every change one undo step |
| Hierarchy | A foldable tree. Nesting exists in the level and the core; the UI has no gesture for it |
| Landscape | Add a landscape of four tiles; sculpt (raise, lower, smooth, flatten); layers and painting; rules: grass scattered on a layer, a layer that fills on steep ground |
| Window | One toolbar row with menus, side panels that give way in a narrow window, status bar, notices |
| Play | The control with its menu: cook, the cooked level named and checked on disk. It cannot start the game (Part 1) |
| Checks | Scripted cases that play input into the built editor, and start-up timings in the log |

Outside the editor, on the game's branch `codex/cooked-level-play`: the game runs a cooked
level of boxes and spawn points, and server and client refuse each other unless they hold
the same one. A level with a landscape is refused there.

Not there at all: assets of any kind, materials and textures, prefabs, several selected
objects, snapping, groups from the UI, anything about gameplay beyond a spawn point. Play
is there in part: its controls in the editor and the game's side of it exist, and starting
the game from the editor, which joins them, does not.

### 2.2 The grouping (proposed)

The editor today has two kinds of tool, objects and landscape, chosen in the toolbar, and
the right-hand panel shows the chosen tool's settings. The proposal keeps that shape and
names the kinds, so that a new feature has an obvious home and the toolbar does not grow a
button per feature. Each group is a **workspace**: a tool set in the toolbar's tool menu and
its own right-hand panel. The hierarchy, the view, the status bar and Play are shared.

| Workspace | Is for | Today | Proposed additions |
|---|---|---|---|
| **Level** (the Level menu, not a tool) | the file, its cooked package and its settings | file actions, Cook | level settings (name, zone bounds, the collision profile); a cook report; the list of cooked packages with their hashes |
| **Objects** | what stands in the level | box, spawn point, gizmo | prefabs with overrides; several selected objects; snapping to grid and surface; turning an object to the slope; groups and reparenting by drag |
| **Landscape** | the ground | sculpt, paint, grass rule, steep-ground rule | mask layers; ground textures; noise and erosion brushes; height-map import and export; tiles loaded around the view |
| **Gameplay** | what the server acts on | spawn point (as an object) | volumes and markers with a meaning: zone exit to another zone, safe area, trigger, spawner for non-player characters, resource node, patrol path; walkable-area preview |
| **Combat** | whether a place fights well | nothing | cover and sightline overlays from chosen points; distance rings for weapon ranges; spawn-to-spawn sight check; height advantage tint |
| **Assets** | what the level is made from | nothing | browser, import and reimport, a model placed as an object, materials |
| **Play** (the Play control, not a tool) | running the level | cook, the cooked level named and checked; the game, started by hand, runs a cooked level of boxes and spawn points | starting and stopping the game from the editor, then the rest of Part 1 |
| **Review** | whether the level is fit to ship | status bar, notices | a problems list from the core's validation; what a change did to the simulation hash; cost readouts for grass and tiles; a summary of changes since the last save for a commit message |

Why these and not a list by technology: a person building a zone thinks "place things",
"shape ground", "say what happens here", "check the fight", "run it". Every workspace answers
one such question, and each maps to one part of the game's roadmap: Gameplay to accounts,
persistence and multi-zone; Combat to the combat model; Assets and Landscape to the art
pipeline; Review to deployment.

### 2.3 The rule that keeps server and client honest (proposed)

The editor already tells apart what both sides must agree on (heights, collision, spawn
points) from what only the picture needs (painted layers, grass): the first is in the
simulation hash, the second is not. Every new kind of thing joins one side explicitly, and
the editor shows which:

| Kind | Side | Consequence |
|---|---|---|
| Collision, heights, spawn points, zone exits, triggers, spawners, safe areas, walkable area | Affects simulation | changes the simulation hash; a client with another version is refused by the server |
| Ground textures, grass, lights, decoration, sound areas | Visual only | changes the package, not the simulation hash; the server never loads it |
| Cover and sightline overlays, distance rings, cost readouts | Editor only | never saved into the level, never cooked |

The label is set by the core, shown by the UI and not a switch a person flips. The Review
workspace's "what did this change do" is this table applied to the last edit.

### 2.4 Where it goes in the window (proposed)

- Toolbar: Level, Undo, Redo, the tool menu or buttons, Add, Remove, then Play and Panels.
  The tool buttons become the workspaces: Objects, Landscape, Gameplay, Combat. The three
  gizmo modes and the three landscape tools move from the toolbar into their workspace's
  panel, where the landscape's brush modes already are. That keeps one row as workspaces are
  added, at every width.
- Add menu: grows by workspace: objects, then gameplay markers, with a divider between.
- Right-hand panel: the inspector for the selection, or the chosen workspace's tool settings,
  as today.
- Bottom: an Assets panel, as the layouts already reserve it, hidden until there are assets.
- Left: the hierarchy, with a filter by workspace (only gameplay markers, only prefabs).

### 2.5 Order (proposed), and what each step waits for

| Order | Step | Waits for |
|---|---|---|
| 1 | Play P1 (P0 is built) | the editor core's coordinator and process launch; the game's branch merged |
| 2 | Objects: several selected, snapping, reparent by drag, turn to slope | nothing outside the editor; the core has reparenting |
| 3 | Review: problems list, change summary | the core's validation codes, which exist |
| 4 | Gameplay: zone exit, safe area, trigger as cooked volumes | the game's multi-zone and gateway work giving them a meaning; the format can come first |
| 5 | Assets: browser, import, models as objects, ground textures | the art pipeline and an asset format in the core |
| 6 | Combat overlays | editor only; most useful once Play with several clients exists |
| 7 | Landscape at scale: tiles around the view, import, erosion | the owner's answer on world size and ground detail |
| 8 | Prefabs with overrides | the core's reference and override model |

Steps 2, 3 and 6 need nothing from the game and little from the core, so they are what the
UI side can do while the game integration is being built.

### 2.6 Not proposed

Scripting or visual logic in the editor, quest and dialogue editing, an animation or
character editor, a material graph, live editing of a running server, several people in one
level at once. Each may become its own decision; none is needed for the steps above, and the
first four are tools of their own rather than parts of a level editor.

### 2.7 Open, for the owner

- Whether Gameplay markers are designed now, as data the server ignores until it knows them,
  or only when the game's multi-zone work starts.
- Whether the tool buttons become workspaces (2.4), which changes the toolbar again.
- Whether Combat overlays are wanted before combat exists in the game; they can be built on
  spawn points and collision alone.
- How large a zone is and how fine its ground: still the question that gates landscape at
  scale.
