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
dos/                Original DOS game artifacts
docs/REFERENCE.md   Reverse-engineered binary-format reference
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

The project is currently in the reverse-engineering and specification phase.
The major binary containers are understood well enough to build lossless readers,
but no Rust game engine has been implemented yet.

Important remaining research includes:

1. Translating all map-overlay handlers into typed event definitions.
2. Identifying the remaining map descriptor fields and event return values.
3. Mapping character flags, conditions, resistances, items, spells, and monster
   identifiers to their exact game semantics.
4. Defining which runtime and map-local values belong in a resumable save game.
5. Reconstructing rendering composition and palettes while retaining the
   original four-color source artwork.

Implementation should follow the documented formats and preserve unresolved
bytes losslessly until their meanings are established.

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
functions rather than emulating fixed addresses or the 8086 overlay ABI.

## Legal note

The DOS files are original copyrighted game assets. Anyone distributing or using
this project is responsible for ensuring that they have the right to use those
assets. A native engine implementation should remain logically separate from
the proprietary data it loads.
