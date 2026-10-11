# Play and Stop, and how the editor's features are grouped

A proposal, 2026-10-09, brought up to date on 2026-10-10 and three times on 2026-10-11 with
what has been built since. Nothing proposed in it is decided. It has two parts: how starting and stopping the game should look and behave in the editor, and how
the editor's features should be grouped as it grows into the production tool for a zoned
online game with real-time combat. Each part says first what exists today and then what is
proposed, and the two are never mixed in one table.

Progress, 2026-10-11. P1 of 1.11 is built: "Open Game Window" starts the game on the cooked
level, in its own window, and Stop ends it.

- The editor's side is on the branch `codex/play-backend`: the Play coordinator in the editor
  core, and the control, status bar, notices and closing flow in the UI.
- The game's side is on the branch `codex/play-readiness`: the game's server and client
  report their readiness steps to the editor and take its Stop.
- Both branches are local to the development machine and merged nowhere. Neither is pushed.
- Checked: the editor's full scripted suite passes with a scripted stand-in in the game's
  place; its current count is in the results below. The editor core's live smoke, with the
  real server and client on a cooked level, reached Running with the client's steps "level
  drawn" and "first controllable frame" measured, then stopped on request, and both processes
  ended by themselves and were collected.
- Not checked: the editor's own window has not been used to start the real game, and the new
  states have not been looked at on screen.

Sections 1.3 to 1.8 remain the design. What of them is built is said in 1.1 and 1.11; "Preview
in Editor" (P2), the play settings of 1.9 and all of Part 2's groupings are proposals as before.

Progress, 2026-10-11, later the same day. Two more things are built on the same two local,
unmerged branches.

- Blockout editing, the first half of step 2 of 2.5, in the editor on `codex/play-backend`:
  - Several objects are selected with Shift and a click, in the hierarchy and in the 3D view;
    the same click on a selected object takes it out, and a plain click selects one as before.
  - The inspector says how many are selected. Each number they differ in is a dash, as the
    design's PropertyRow has it; a number they share is a field, and what is typed there goes
    to all of them as one undo step. Remove takes all of them, as one undo step.
  - The gizmo moves, turns and sizes them together as one undo step: one move for all, one
    turn about the object the gizmo is on, so that they stand to each other as before.
  - Snap is a switch in the toolbar, as the design's ToolButton page has it, and a row of the
    tool menu in a narrow window; the 3D view says "Snap 0.5 m, 15°" or "Snap off". With it
    on, a moved object lands on the grid along the axes it is moved on, a turn goes in steps,
    and a sized box gets a size of whole steps. Of several, the object the gizmo is on lands
    on the grid and the others move by exactly as much. It is off until switched on.
- Landscape in Play, in part. The editor's check before a start now lets a level with ground
  and painted layers through, and refuses, in words, only a level whose landscape has rules,
  that is grass or a layer a rule fills. The game's side, ground as collision and painted
  layers as colour, is built on `codex/play-readiness`.

Results on the final build of that day, each of a different kind:

- Scripted, in the built editor: the suite plays real input into the editor's window, so
  picking, the gizmo and the widgets run for real, with a scripted stand-in wherever a case
  needs a game. 236 of 236 cases passed, 26 of them new for the above, at the default width,
  at 1261 and at 621 logical pixels.
- Unit tests of the editor's workspace: 265 passed, none failed. Among them, through the real
  check before a start: a level with ground and painted layers is played, and one whose
  landscape has a rule is refused before anything starts, with the reason. The linter passes
  with warnings refused, apart from one older finding in the editor core that is allowed by
  name.
- The real game, by the editor core's smoke example and not through the editor's window, as
  reported by the game's side: a fixture package with ground and painted layers, and a freshly
  cooked level of boxes, each reached Running, was stopped on request and ended as stopped; server
  and client ended with status 0 and were collected, with nothing forced. One warning stays
  in the client's log and is known to be harmless: the engine's window code reports a
  destroyed window with an ID it no longer knows as the client exits.
- Not checked by anyone: nobody has looked at any of this on screen, and the real game has
  still not been started from the editor's window.
- Known and left as it is:
  - The game runs none of what a landscape's rules make. Such a level is refused; its ground
    and painted layers alone cannot be played while the rules are in it.
  - Starting the game works on Unix only, and both branches are local and unmerged.
  - The gizmo is on the first selected object in the core's order, which is by the objects'
    IDs and not by which was clicked last.
  - An object seen behind a handle of the gizmo cannot be clicked there: the handle takes the
    click.
  - A number the selected objects differ in cannot be typed over to make it the same.
  - The step sizes are fixed at half a metre and fifteen degrees; the design names neither.

Proposed next, in this order, none decided:

1. One key to cook, check and play, and one to stop; "Cook and play again", "Play again" and
   "Open log" on the notices (1.5, 1.7, 1.12).
2. The rest of step 2 of 2.5: duplicate, a setting for the snap steps, typing over a dash, a
   choice of which selected object the gizmo is on, a drag in the hierarchy to reparent.
3. The problems list of step 3 of 2.5, beginning with what stops Play: no spawn point, a
   landscape with rules.
4. A person at the window: the real game started and stopped from the Play control, and the
   new states looked at on screen.

Progress, 2026-10-11, the third note. The first three of the steps proposed above are built,
on `codex/play-backend`, which is now pushed to the editor's repository and merged nowhere.
The editor core's side of them (settings of a session, duplicating, reparenting, giving several
objects one value, the review of a level, showing a file) is in the same branch.

- Play from the keyboard, and notices that offer what mends them:
  - F5 cooks the level if it has changed since its cook, has the cooked level checked and
    starts the game in its own window. The Play menu's row "Cook and Play" does the same.
    Shift+F5 is Stop, at whatever step that has reached.
  - The key cooks nothing and says why in one sentence when a game runs already, when no game
    is set up, and when the level has something the game cannot run.
  - When the level moves on from the cooked level a run plays, a notice offers "Cook and play
    again": the run is stopped, the level cooked as it is, checked and played.
  - A game that stopped by itself, or did not start, offers "Play again" and, where the game
    wrote a log, "Open log". A start refused for the level itself offers no second try of the
    same cooked level.
- The rest of step 2 of 2.5, in the Objects tools:
  - Duplicate, in the Add menu and with Ctrl+D: each selected object is copied with what is
    under it, a metre beside it, and the copies are selected. One undo step. A landscape is
    not copied; the menu's row is off and says why.
  - The steps of snapping are two number fields at the foot of the inspector, kept by the
    core for the session.
  - A dash in the inspector is a field: a number typed over it goes to every selected object.
  - Of several selected objects the gizmo is on the one clicked last, whose row carries a
    dot; a Shift-click on another selected one moves it there. Two buttons of the inspector
    put it on one object or at the middle of them all, which is the mean of where the selected
    boxes and spawn points are. A turn is about the gizmo's place.
  - A row of the hierarchy is dragged onto another row to put its object under that one, or
    onto the list where no row is to bring it to the top of the level. The object stays where
    it is in the level. Selected rows go together, as one undo step.
- Review, the first part of step 3 of 2.5: a tool whose panel, in the inspector's place, lists
  what the core's review of the level finds, under "Play" what stops the game from running it
  and under "Level" what is wrong with the level itself. A problem about one object has a
  button that selects it. F5 on a level the game cannot run chooses this tool.
- The toolbar's row with every tool as a button had grown longer than the 900 pixels the
  design folds it at. The tools now fold into their menu whenever the row cannot hold them,
  which is measured, and the panel switches stay buttons from 900 pixels up as designed.

Results on the final build of that note, each of a different kind:

- Scripted, in the built editor, with a scripted stand-in wherever a case needs a game:
  348 of 348 cases passed, at the default width, at 1261, 1100, 1000, 900 and 621 logical
  pixels.
- Unit tests of the editor's workspace: 320 passed, none failed; 165 of them are
  the UI's and 21 the application's. The linter passes with warnings refused, apart
  from the one older finding that is allowed by name.
- The real game: not run again for this note. What the second note says of it still holds
  and nothing more: it was started by the editor core's smoke example, never from the editor's
  window.
- Not checked by anyone: nobody has looked at any of this on screen. "Open log" has only
  shown a file in a scripted check, where nothing is opened; the core's way to open one is
  covered by the core's own tests.

Known and left as it is:

- The keys are the ones this milestone was given. The design page proposes F5 for "Preview in
  Editor" and Shift+F5 for the game window and decides neither; the editor has no preview. The
  proposal stays open for when it has one (1.12).
- F5 while a game runs is refused in one sentence. It does not stop and restart the game;
  the notice's "Cook and play again" does.
- In the 3D view the middle of the object that has the gizmo is behind the gizmo's handles:
  a Shift-click there is a hold on a handle and takes nothing out of the selection. Its row
  does, and so does a click on the object beside the handles.
- Sizing at the middle of several objects sizes each box about its own middle. It does not
  move them apart.
- Review lists what the core's review lists. In a session that is nothing under "Level": the
  core refuses an edit that would make the level invalid. A summary of changes and cost
  readouts (2.2) are not built; nothing in the core provides them yet.
- The design has no page for: the row "Cook and Play", the snap steps, the two buttons and
  the line for the gizmo, the dot on a row, the ring round a place to drop on, the Review
  panel, and a third button on a notice. Each is built from existing parts and tokens; none
  adds a token.

Proposed next, in this order, none decided:

1. A person at the window: the real game started and stopped from the Play control and with
   F5, a log opened from a notice, and the states of all three notes looked at on screen.
2. The designs this note lists as missing, drawn, and the keys decided.
3. The rest of step 3 of 2.5: a summary of changes since the last save, and what a change did
   to the simulation hash, once the core provides them.
4. Play P2 or the play settings of 1.9, whichever the owner's answer in 1.12 puts first.

Sources:

- `crates/editor-services/docs/play-boundary.md`: the agreed contract between the UI and the
  editor core for Play. Both halves are implemented on `codex/play-backend`: checking a
  cooked package, and the lifecycle of a run in the game's own window.
- The Claude Design system "Arc Editor": the pages PlayControl, Menu, StatusBar, Notice and
  Dialog, and `layouts.md`; the canvas boards "Play: the control in each state", "Playing:
  preview in the editor", "Playing: the game in its own window", "Closing the editor during
  a run".
- The game's foundation design (`arc-game`, `docs/superpowers/specs/2026-10-03-foundation-design.md`):
  zoned shards with one server process per zone, client-predicted movement with server
  reconciliation, and a roadmap of accounts, multi-zone, art pipeline, combat and deployment.
- The editor as it is on the branch `codex/play-backend`, which continues
  `codex/play-workflow`. Local, not merged.
- The game as it is on its branch `codex/cooked-level-play` (commit `df87d04`), with that
  branch's own account in `docs/cooked-package-runtime.md`, and on `codex/play-readiness`,
  which continues it. Neither is merged into the game's `main`; `codex/play-readiness` is
  local.

Words used here. The **open level** is the level as it is in the editor now, with every
change made to it; the **saved file** is what the last Save wrote, and the two differ while
there are unsaved changes. A **cooked level** is the package the cook step makes from the
open level as it is at that moment, saved or not; it is what the game loads. Its identity is
the hash of its manifest, the small file that lists its contents and their hashes: the cooked
folder is named by that hash, the game is started with it, and client and server compare it.
The editor shows a cooked level by the first eight digits of that same hash. A **receipt** is the core's record that
a cooked package on disk was read and verified. A **run** is one start of the game from the
editor, from the request to the end of its cleanup.

## Part 1: Play and Stop

### 1.1 What exists today

| Piece | Where | State |
|---|---|---|
| Cook | editor | Built. Level menu and the Play menu's foot; the status bar names the cooked level |
| Checking a cooked package (`VerifyPackage`, `VerifiedPackageReceipt`) | editor core and UI | Built. Asked for from the Play menu's row "Check cooked level"; a refusal is a notice in words, and Stop cancels a check that is running |
| The Play control (P0 of 1.11) | editor UI | Built. At the right end of the toolbar at every width. Its menu holds the two ways to play; the foot names the cooked level, says when the level has moved on from it, and offers "Cook" or "Cook first" |
| The game running a cooked level | game, branch `codex/cooked-level-play` | Built for levels of boxes and spawn points. Server, client and `cargo xtask run-local` take `--cooked-package <folder> --cooked-package-hash <manifest hash>`. Each process verifies the package itself before it opens a port or a window, and builds collision, spawn points and the client's boxes from it |
| The same level on both sides | game, the same branch | Built. The client's hello carries the hash of the package's manifest, the server accepts only its own, and the client checks the hash the server answers with. A cooked level against the built-in one, or against another cooked level, is refused before a character exists |
| Landscape in the game | game, branch `codex/play-readiness` (local, not merged) | Built for ground and painted layers, and run in the live smoke: the cooked ground is the game's collision, and the painted layers its colour. What a landscape's rules make (grass, layers a rule fills) is not run, and a level that has any is refused, in words; nothing is dropped silently |
| Starting the game from the editor (P1) | editor core, branch `codex/play-backend` (local, not merged) | Built for the game's own window. A Play coordinator starts one server and one client on the checked cooked level, reports each step they reach, stops them on request, and says how the run ended and whether each process was seen to end. A level whose landscape has rules, or a level without a spawn point, is refused before anything is started; ground and painted layers are let through |
| Open Game Window in the UI (P1) | editor UI, the same branch | Built. The control reads "Starting", "Game window open" and "Stopping"; Stop is beside it and also cancels a check or a start. The status bar names the steps reached and an end the person asked for; an end nobody asked for is a notice that stays, and a process that was started and never seen to end is a second notice. Closing the editor during a run stops the game first. The editor does not redraw while a game runs; the coordinator wakes it. The game's folder is named with `--game <folder>`; without it the control says no game is set up |
| The game reporting readiness and taking Stop | game, branch `codex/play-readiness` (local, not merged) | Built. Server and client, started by the editor, report their steps over a local connection and stop when told; the client says when its window was closed by the player |
| Keys, and what a notice offers | editor UI, the same branch | Built (third progress note). F5 cooks, checks and plays in the game's window and Shift+F5 stops; a notice offers "Play again", "Open log" and "Cook and play again" |
| Of the design below, not built | editor UI | The box in the 3D view saying a game window is open; "Try again" for a server that did not shut down |
| Running the game inside the editor's view | editor and game | Not built. P2. "Preview in Editor" is in the menu, switched off, with the reason |
| Design | Claude Design | Drawn: five boards and the PlayControl page. P0 follows them where it can; the Play menu is as wide as any menu, because the design has no token for the wider menu it draws |

So the two ends are joined for the game's own window. A person can cook in the editor, have
the cooked level checked, and start and stop the game on it from the Play control.

What has been run, and this note does not claim more. The editor's scripted suite, 236 of
236 cases, plays the whole of this through the built editor with a scripted stand-in for the
game's processes. The editor core's live smoke ran the real server and client on a level of
boxes cooked for the run and on a fixture package with ground and painted layers: each run reached
Running with "level drawn" and "first controllable frame" measured, was stopped on request,
and both processes ended with status 0 and were collected. The two have not been put together: nobody has yet started the real game from the
editor's window, and the new states have not been looked at on screen. Neither repository has
been through its full checks with these branches, and the process launch works on Unix only.

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

Asked for in addition. As of 2026-10-11 the core on `codex/play-backend` delivers 1, 4, 5 and
6, and 3 for one server and one client. For 2 it holds the run's own package identity; the UI
still makes the comparison with the open level. Bots and several clients (the rest of 3)
belong to 1.9 and are not built.

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

The editor and the game name a cooked level by the same hash. The editor shows the first
eight digits of the manifest's hash, taken from the receipt of its own cook, in the Play
menu's foot, in the word "Checking" and in the status bar; the game is started with that
hash and compares it on the wire. So the digits in a refusal from the game are the digits in
the status bar. Only without a receipt, when the level's status says it is cooked and this
editor did not cook it, does the editor show the simulation data's hash instead.

### 1.11 Order of work (proposed)

| Step | The person gets | Needs |
|---|---|---|
| P0, built | The Play control with its menu and foot; "Cook first"; "Checking" against the real `VerifyPackage`; the two ways to play shown as not available yet, with the reason | nothing new from the core |
| P1, built on `codex/play-backend` and `codex/play-readiness` (neither merged) | Open Game Window: Starting, Running, Stop, every end of 1.7, closing during a run; one key to cook, check and play and one to stop; "Open log", "Play again" and "Cook and play again" on the notices. Left for later: the box in the 3D view | merging the two branches; in the game, what a landscape's rules make |
| P2 | Preview in Editor: the game in the 3D view, the pointer handed over and back | the game's client able to run inside the editor's window |
| P3 | Play settings of 1.9, one at a time | per setting, see its row |

### 1.12 Open, for the owner

- The keys. The design pages assume F5 for "Preview in Editor" and Shift+F5 for the game
  window and decide neither. Built for now, while there is no preview: F5 cooks, checks and
  plays in the game's window, Shift+F5 stops. Which it is to be once a preview exists is open.
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
| Objects | Boxes and spawn points: add where the view looks (on the landscape if it is there), select in the list or the view, several with Shift, move, rotate and scale with the gizmo, alone or several together, on one of them or at their middle, with or without snapping to a grid and to steps of a turn that can be set, duplicate, rename, type position, rotation and size, also over a value several differ in, remove; every change one undo step |
| Hierarchy | A foldable tree. A row is dragged onto another to put its object under that one, or onto the list to bring it to the top |
| Review | A tool whose panel lists what the core's review of the level finds: what stops the game from running it, and what is wrong with the level |
| Landscape | Add a landscape of four tiles; sculpt (raise, lower, smooth, flatten); layers and painting; rules: grass scattered on a layer, a layer that fills on steep ground |
| Window | One toolbar row with menus, whose tools fold into a menu when the row cannot hold them, side panels that give way in a narrow window, status bar, notices |
| Play | The control with its menu: cook, the cooked level named and checked on disk, and the game started in its own window and stopped, also with one key each (Part 1). On `codex/play-backend`, not merged |
| Checks | Scripted cases that play input into the built editor, with a scripted stand-in for the game where a case needs one, and start-up timings in the log |

Outside the editor, on the game's branches `codex/cooked-level-play` and, continuing it,
`codex/play-readiness` (local, not merged): the game runs a cooked level of boxes and spawn
points, server and client refuse each other unless they hold the same one, and both report
their readiness to an editor that started them. A level with a landscape is played with its
ground and its painted layers; one whose landscape has rules is not, and the editor refuses to
start it.

Not there at all: assets of any kind, materials and textures, prefabs, making an empty group
from the UI, anything about gameplay beyond a spawn point. Play
is there in part: the game in its own window is; the game inside the editor's view and the
play settings of 1.9 are not.

### 2.2 The grouping (proposed)

The editor today has two kinds of tool, objects and landscape, chosen in the toolbar, and
the right-hand panel shows the chosen tool's settings. The proposal keeps that shape and
names the kinds, so that a new feature has an obvious home and the toolbar does not grow a
button per feature. Each group is a **workspace**: a tool set in the toolbar's tool menu and
its own right-hand panel. The hierarchy, the view, the status bar and Play are shared.

| Workspace | Is for | Today | Proposed additions |
|---|---|---|---|
| **Level** (the Level menu, not a tool) | the file, its cooked package and its settings | file actions, Cook | level settings (name, zone bounds, the collision profile); a cook report; the list of cooked packages with their hashes |
| **Objects** | what stands in the level | box, spawn point, gizmo; several selected objects; snapping to a grid; duplicate; reparenting by drag | prefabs with overrides; snapping to a surface; turning an object to the slope; empty groups |
| **Landscape** | the ground | sculpt, paint, grass rule, steep-ground rule | mask layers; ground textures; noise and erosion brushes; height-map import and export; tiles loaded around the view |
| **Gameplay** | what the server acts on | spawn point (as an object) | volumes and markers with a meaning: zone exit to another zone, safe area, trigger, spawner for non-player characters, resource node, patrol path; walkable-area preview |
| **Combat** | whether a place fights well | nothing | cover and sightline overlays from chosen points; distance rings for weapon ranges; spawn-to-spawn sight check; height advantage tint |
| **Assets** | what the level is made from | nothing | browser, import and reimport, a model placed as an object, materials |
| **Play** (the Play control, not a tool) | running the level | cook, the cooked level named and checked; the game started in its own window and stopped, for a level of boxes and spawn points | the rest of Part 1: the game in the editor's view, play settings |
| **Review** | whether the level is fit to ship | status bar, notices; a problems list from the core's review | what a change did to the simulation hash; cost readouts for grass and tiles; a summary of changes since the last save for a commit message |

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
| Collision, heights, spawn points, zone exits, triggers, spawners, safe areas, walkable area | Affects simulation | changes the simulation hash, and with it the package's identity; a client with another version is refused by the server |
| Ground textures, grass, lights, decoration, sound areas | Visual only | leaves the simulation hash as it is and changes the package's identity. As the game's handshake stands, server and client must hold the same package, so a client that differs only in decoration is refused too |
| Cover and sightline overlays, distance rings, cost readouts | Editor only | never saved into the level, never cooked |

Whether the game should accept a client whose package differs only in what is visual is
open: its handshake compares the manifest's hash, which covers both kinds, and the simulation
hash alone would let such a client in.

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
| 1 | Play P1: implemented on `codex/play-backend`; game runtime support is on `codex/play-readiness` | integrate both branches into their respective main lines |
| 2 | Objects: multi-selection, snapping, duplicate and drag reparenting implemented. Left: turn to slope | nothing outside the editor |
| 3 | Review: core problems listed. Left: change summary | the core providing what changed |
| 4 | Rendered viewport: Game Look now consumes the shared game look profile; compare captures with the client while the display is awake, then add published models and cooked-scene parity | shared presentation contract, capture verification and a real level/model asset path |
| 5 | Assets: browser, import and reimport, models as objects, ground textures | the art pipeline and an asset reference in the scene/cooked format |
| 6 | Particles: shared v1 spec, emitter services and the first editor authoring/reference-preview tool are implemented; add the Hanabi runtime adapter, then place and cook emitters | pin and compile Hanabi in editor and client; prove GPU preview and runtime parity |
| 7 | Render tools: material and lighting preview modes, then scene lighting controls | the Game view and asset/material references |
| 8 | Gameplay: zone exit, safe area, trigger as cooked volumes | the game's multi-zone and gateway work giving them a meaning; the format can come first |
| 9 | Combat overlays | editor only; most useful once Play with several clients exists |
| 10 | Landscape at scale: tiles around the view, import, erosion | the owner's answer on world size and ground detail |
| 11 | Prefabs with overrides | the core's reference and override model |

Steps 1–3 are implemented on their feature branches. Game Look is implemented in both the
client and editor from `assets/render/look-v1.json`; package-scoped editor checks and the
`view-game-look` scripted state check passed. Image parity has not yet been inspected: recent
captures were taken while macOS had the display asleep, and both current and pre-A0 editor
captures were uniformly black. Capture with the display awake before claiming visual parity.
The current Game Look is an open-level preview: it uses the editor camera and retains editor
overlays, spawn markers and authored decorations. The profile is loaded at startup, so edits
require restarting the editor. The shared particle contract is now in
`editor/crates/particle-spec`, with a 16 KiB closed JSON schema, canonical writer, validation,
stable ID/path helpers, and conservative live-particle estimate. Hanabi compatibility has only
been checked from package metadata so far; compile and GPU pipeline validation remain open.
The Particles tool now lists reusable game-folder emitters, creates and duplicates them, edits
their shared spec fields with per-emitter undo/redo, saves explicitly, and previews a fixed-seed
CPU reference simulation on a dedicated stage. The preview is clearly labeled as a reference;
it is not the game's renderer. Targeted service/UI tests, the app build and two particle scripts
passed. Emitter file operations serialize cooperating editor instances with a sidecar lock;
non-cooperating writers can still race an overwrite, and stale locks after a crash require manual
removal. Do not call particle preview parity complete until the shared Hanabi adapter is active.

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

## Part 3: Render and effects authoring

The next editor phase adds art iteration to the level workflow. “Rendered” means the actual
game presentation path, not a second editor-only approximation: a level should look the same
in the editor's Game view and in the client, including its published meshes, materials,
lighting, sky, shadows, exposure and tone mapping. Keep the current editor view for selection,
collision and painting. Game view and later material/lighting previews are view modes over the
same scene, with editor overlays available in both.

The work is staged so each step has a usable result:

1. **Game view parity.** Continue the render contract for the game's level meshes and terrain,
   using the shared look profile already consumed by the client and editor. Compare a deterministic
   editor capture with a client capture when the display is awake. Add published GLB models to the editor after scene data and cooking can carry asset references. Do not call a
   view “Game” while it silently substitutes editor materials or lighting. The first Game-look
   view renders the open editor draft, so label it as a preview; it is not proof of the cooked
   package or a running client's final image. Keep selection and editing overlays legible without
   replacing the authored material.
2. **Assets workspace.** Browse the game's published assets, show load/validation problems,
   and place a model into a level. Keep authored sources and export tools separate from runtime
   assets; cooked levels refer to published asset identities rather than machine-specific
   source paths.
3. **Particle workspace.** The shared renderer-independent v1 emitter document and validation
   crate now exist at `editor/crates/particle-spec`; the editor and game depend on that same
   source. Its defined semantics cover point, sphere, cone and box emission, uniform speed and
   lifetime, billboard width, linear colour/size interpolation and world-space simulation after
   spawn. The editor has reusable asset services and authoring UI with explicit save and a
   dedicated fixed-seed reference preview. Next, translate the shared spec through one Hanabi
   adapter used by the game and editor preview. Keep texture dependencies out of v1. A later step
   places emitters in levels and adds their references to the cooked visual payload. GPU
   preview requires a rendering-capable editor; headless checks validate and round-trip the
   asset without opening a window.
4. **Material and lighting previews.** Add material-ball/material-swatch previews and a small
   lighting stage, followed by view modes such as Lit, Unlit and Wireframe in the level viewport.
   Scene-authored lights and environment settings come only after the scene/cook schema can
   represent them and the client consumes that same data.

The editor and game both use Bevy 0.19.1, and the foundation plan already names `bevy_hanabi`
0.19.0 for the particle sub-project. Metadata indicates the matching Bevy generation, but the
custom Arc Bevy fork has not yet been proven to compile with Hanabi. Hanabi is the rendering backend, while the authored
contract stays smaller and renderer-independent so file stability does not depend on registered
engine modifier types. Verify its Bevy/wgpu/naga dependency graph against the pinned engine
before integration. This does not settle texture/material references, GPU feature support,
asset digest rules, or how the level cook includes effect dependencies; those are gates before
emitters can be placed in a shipped level.

For editor layout, keep authoring domains as workspaces rather than adding permanent toolbar
buttons. The Particles workspace owns the emitter list, preview controls and selected-emitter
properties; the shared viewport can switch between Level/Game and asset preview. Assets remain
a bottom browser, and Level/Play/Review keep their existing roles. Defer a general-purpose
material graph: the first material tools are preview and a constrained set of authored values.
