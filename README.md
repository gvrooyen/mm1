# Might and Magic Book One Reimplementation

This project rebuilds the original DOS version of **Might and Magic Book One:
Secret of the Inner Sanctum** in Rust.

The goal is not to emulate DOS or execute the original game binary indefinitely.
Instead, the project will recover the game's data formats and behavior, represent
them with native Rust types, and implement a portable engine that uses the
original game assets.

## Project goals

- Preserve the world, encounters, character system, and game mechanics of the
  original DOS release.
- Decode the original maps, graphics, character records, and map-specific event
  logic rather than replacing them with approximations.
- Build a native Rust engine without depending on segmented memory, executable
  overlays, or hardware-specific CGA/EGA rendering code.
- Support versioned save games that can resume the complete current game state,
  rather than only the character-centric persistence offered by the original
  inn and roster system.
- Keep the original binary formats documented so that conversions remain
  reproducible and unknown data is not silently discarded.

## Repository contents

```text
.agents/setup        Idempotent Debian/orb development setup
AGENTS.md            Contributor and coding-agent guidance
src/main.rs          Current game, renderer, and headless entry point
dos/                 Original DOS game artifacts
docs/REFERENCE.md    Reverse-engineered binary-format reference
Cargo.toml           Rust package and dependencies
rust-toolchain.toml  Pinned Rust toolchain
```

The `dos/` directory includes the original executable, map overlays, maze data,
character roster, compressed graphics, configuration utility, manual, and linker
symbol map. These files are the primary source material for the reimplementation.

The detailed findings are recorded in
[`docs/REFERENCE.md`](docs/REFERENCE.md). That document currently covers:

- the 55-map world layout and `MAZEDATA.DTA` cell encoding;
- the executable `.OVR` container and map-event tables;
- screen, monster, and wall graphics containers and compression;
- the complete physical layout of `ROSTER.DTA` character records;
- graphics-adapter configuration and `MM.RSM` symbols; and
- the state a native save-game format will need to preserve.

## Current status

The project now includes a native playable Sorpigal slice. Escape, Space, or
Enter leaves the animated title for the original-style options screen. `C`
opens character creation, `V` opens the roster, and Enter enters the Inn of
Sorpigal. At the inn, A-F view characters, Ctrl-A through Ctrl-F add or remove
up to six party members, and Enter starts town exploration. Arrow keys move and
turn through map 0's original walls.
Exploration renders the original perspective wall, door, and special-wall
components at four depths, with the command panel, party roster, and messages
composed in the logical 320x200 framebuffer. Number keys open character views.
Town services and special squares are player-facing, and encounters display the
monster artwork selected by the original monster record. Sorpigal encounters
use a small deterministic native combat subset; trap damage and destinations
outside Sorpigal remain explicitly unimplemented. The
executable also decodes the original `SCREEN0` through
`SCREEN9` artwork into a palette-indexed 320x200 framebuffer. It alternates
between the first two title images with the DOS release's outside-in rectangular
reveal in a 960x600 window using `pixels` and `winit`. Press Space to show or
advance the `SCREEN2` through `SCREEN9` slideshow; each image advances
automatically after ten seconds. The original PC-speaker title sequence is
rendered to MP3 and played through `rodio`.

Escape starts the game from the animated title and otherwise backs out; close
the window or press Ctrl-C/Ctrl-Q to quit. The engine atomically autosaves to
`savegame.json` after every completed command and on clean shutdown, then
resumes that state on the next graphical or headless invocation. The save
contains native mutable state while immutable definitions continue to load from
the original DOS assets. If
an audio device or the title music is unavailable, the executable reports the
problem on stderr and continues without audio.

Important remaining research includes:

1. Translating all map-overlay handlers into typed event definitions.
2. Identifying the remaining map descriptor fields and event return values.
3. Mapping character flags, conditions, resistances, items, spells, and monster
   identifiers to their exact game semantics.
4. Reconstructing rendering composition and palettes while retaining the
   original four-color source artwork.

Implementation should follow the documented formats and preserve unresolved
bytes losslessly until their meanings are established.

## Running the scaffold

On Debian or in an Amp orb, install the pinned Rust toolchain and native window
system dependencies with the idempotent setup script:

```sh
.agents/setup
```

Run the graphical title screen:

```sh
cargo run
```

Browse the original game assets without showing the title screen:

```sh
cargo run -- --browse
```

The browser's arrow-key-driven top-level menu contains maps, monsters, walls,
images, and roster entries. `MAPS` shows all 55 mazes from `dos/MAZEDATA.DTA`
with north at the top; normal walls are white, doors cyan, and special walls
magenta. `MONSTERS` shows all 76 pictures decoded from
`dos/MONPIX.DTA`; `WALLS` shows representative perspective corridors for all 18
environment sets decoded from `dos/WALLPIX.DTA`; and `IMAGES` shows the ten
full-screen pictures decoded from `dos/SCREEN0` through `dos/SCREEN9`. Use the
arrow keys to move through each collection. `ROSTER` shows the six populated
static characters decoded from `dos/ROSTER.DTA`, including their attributes and
resources. Press Escape to return to the previous menu, or Ctrl-C/Ctrl-Q to
quit.

For automated tests and non-graphical clients, the executable can apply a
complete command script and report the final versioned JSON player view without
initializing graphical or audio devices. `--commands` is repeatable and each
value may be comma-separated:

```sh
cargo run -- --headless
cargo run -- --headless --commands start,toggle:1,toggle:2,confirm,forward,left
```

Pass `--reset` to delete the saved game and start over from the beginning. It
works with both headless and windowed modes:

```sh
cargo run -- --reset --headless
cargo run -- --reset
```

For an interactive agent session, pass `--interactive`. The process emits the
initial view as newline-delimited JSON (NDJSON), then reads one command per line
from stdin and emits the resulting view as another NDJSON line. Empty input
lines are ignored, and EOF ends the session:

```sh
printf 'start\ntoggle:1\nconfirm\n' | cargo run --quiet -- --headless --interactive
```

Commands include `start`, `toggle:N`, `confirm`, `forward`, `back`, `left`,
`right`, `unlock`, `bash`, `choose:N`, `food`, `drink`, `tip`, `rumor`, `gather`,
`restore`, `realign`, `donate`, `train`, `yes`, `no`, and `escape`.
Combat adds `fight`, `attack:N`, `defend`, `cast` (an explicit unsupported
response), `flee`, and treasure `open`/`leave`. Monster names, HP, rounds, and
the active party member are exposed in the schema-v2 combat object. The subset
decodes monster statistics and rewards from `MM.EXE`, but simplifies initiative,
physical attacks, fleeing, conditions, and treasure handling.

Each JSON view lists the commands valid on its current screen. Output uses
schema version 2 and includes options, party resources, messages,
position/facing, and directional wall/exits. Non-interactive headless mode
always outputs one JSON document; interactive mode outputs one document per
NDJSON line. Commands continue from `savegame.json`; without an existing save,
the initial view is the title. The engine writes only the native save and never
modifies a DOS asset.

Run the development checks with:

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
```

## Intended architecture

The original files naturally divide into static definitions and mutable runtime
state:

```text
MAZEDATA.DTA ──► map geometry and cell properties
*.OVR        ──► map descriptors, events, encounters, text, and initial state
graphics     ──► adapter-neutral logical-color images
ROSTER.DTA   ──► import/export compatibility for original characters
Rust saves   ──► party, position, map state, encounters, effects, and RNG state
```

The native engine should preserve the behavior encoded by the original overlay
machine code, but it should express that behavior through normal Rust data and
functions rather than emulating fixed addresses or the 8086 overlay ABI. The
renderer currently keeps a four-color indexed 320x200 framebuffer and converts
it to RGBA only when copying it to the `pixels` surface. Game state and player
view descriptions should remain independent of window creation so the same
state is available through `--headless`.

## Legal note

The DOS files are original copyrighted game assets. Anyone distributing or using
this project is responsible for ensuring that they have the right to use those
assets. A native engine implementation should remain logically separate from
the proprietary data it loads.
