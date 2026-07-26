# Might and Magic Book One DOS Data Reference

This document describes the binary artifacts in [`dos/`](../dos/) and the parts
of the original DOS runtime needed to interpret them. Its purpose is to support
a native reimplementation without requiring the new engine to emulate the
original executable, segmented memory model, or overlay loader.

The findings were obtained by:

- inspecting file sizes, repeated structures, and cross-file invariants;
- parsing the MZ headers of `MM.EXE` and `GRAPHSET.EXE`;
- disassembling file I/O, graphics, movement, and overlay-loader routines;
- using `MM.RSM` to associate executable addresses with original symbol names;
- comparing data records against the mechanics described in `Manual.pdf`; and
- cross-checking disputed details against ScummVM's MM1 engine and independent
  MM1 graphics/save-file tools.

Unless noted otherwise, integers are unsigned and little-endian. File offsets
are hexadecimal when prefixed by `0x`.

## Confidence terminology

- **Exact**: established by loader code or an invariant holding for every file.
- **High**: supported by executable behavior and the supplied data.
- **Corroborated**: also implemented by an independent MM1-compatible engine or
  tool.
- **Unresolved**: the bytes are structurally located, but their complete game
  semantics have not yet been established.

## Artifact inventory

| Artifact | Size/count | Purpose |
| --- | ---: | --- |
| `MM.EXE` | 119,264 bytes | Main 16-bit DOS engine |
| `GRAPHSET.EXE` | 2,512 bytes | Graphics-adapter configuration utility |
| `*.OVR` | 55 files | Map-specific executable code, data, events, and text |
| `MAZEDATA.DTA` | 28,160 bytes | Geometry and cell properties for 55 maps |
| `ROSTER.DTA` | 2,304 bytes | Persistent character roster |
| `MONPIX.DTA` | 81,872 bytes | 76 compressed monster images |
| `WALLPIX.DTA` | 123,059 bytes | 18 compressed wall/environment sets |
| `SCREEN0`–`SCREEN9` | 10 files | Compressed 320×200 screens |
| `GACARD.DTA` | 1 byte | Selected graphics adapter |
| `MM.RSM` | 6,656 bytes | Linker/compiler symbol map for `MM.EXE` |
| `Manual.pdf` | 6 pages | Game manual and mechanics reference |
| `Readme.txt` | 4,235 bytes | Distribution/support information |

`Manual.pdf` and `Readme.txt` are not opened by `MM.EXE`. No proven runtime
reference to `MM.RSM` exists either; it is development/linker metadata rather
than game input.

# World and map data

## Map order

`MM.EXE` contains a table of 55 overlay base names. `MAZEDATA.DTA` contains
exactly 55 records, and the records correspond to overlays in this order:

| Index | Overlay | `MAZEDATA.DTA` offset |
| ---: | --- | ---: |
| 0 | `SORPIGAL.OVR` | `0x0000` |
| 1 | `PORTSMIT.OVR` | `0x0200` |
| 2 | `ALGARY.OVR` | `0x0400` |
| 3 | `DUSK.OVR` | `0x0600` |
| 4 | `ERLIQUIN.OVR` | `0x0800` |
| 5–13 | `CAVE1.OVR`–`CAVE9.OVR` | `index × 0x200` |
| 14–17 | `AREAA1.OVR`–`AREAA4.OVR` | `index × 0x200` |
| 18–21 | `AREAB1.OVR`–`AREAB4.OVR` | `index × 0x200` |
| 22–25 | `AREAC1.OVR`–`AREAC4.OVR` | `index × 0x200` |
| 26–29 | `AREAD1.OVR`–`AREAD4.OVR` | `index × 0x200` |
| 30–33 | `AREAE1.OVR`–`AREAE4.OVR` | `index × 0x200` |
| 34 | `DOOM.OVR` | `0x4400` |
| 35 | `BLACKRN.OVR` | `0x4600` |
| 36 | `BLACKRS.OVR` | `0x4800` |
| 37 | `QVL1.OVR` | `0x4A00` |
| 38 | `QVL2.OVR` | `0x4C00` |
| 39 | `RWL1.OVR` | `0x4E00` |
| 40 | `RWL2.OVR` | `0x5000` |
| 41 | `ENF1.OVR` | `0x5200` |
| 42 | `ENF2.OVR` | `0x5400` |
| 43 | `WHITEW.OVR` | `0x5600` |
| 44 | `DRAGAD.OVR` | `0x5800` |
| 45 | `UDRAG1.OVR` | `0x5A00` |
| 46 | `UDRAG2.OVR` | `0x5C00` |
| 47 | `UDRAG3.OVR` | `0x5E00` |
| 48 | `DEMON.OVR` | `0x6000` |
| 49 | `ALAMAR.OVR` | `0x6200` |
| 50 | `PP1.OVR` | `0x6400` |
| 51 | `PP2.OVR` | `0x6600` |
| 52 | `PP3.OVR` | `0x6800` |
| 53 | `PP4.OVR` | `0x6A00` |
| 54 | `ASTRAL.OVR` | `0x6C00` |

The 20 `AREA` overlays form a 5×4 wilderness-sector grid. Each sector is still
an independent 16×16 local map. Pairs and groups such as `QVL1/QVL2` and
`UDRAG1`–`UDRAG3` are separate maps, not floors packed into one maze record.

## `MAZEDATA.DTA`

The file size gives an exact top-level framing:

```text
28,160 = 55 × 512
```

Conceptual schema:

```rust
struct MazeData {
    maps: [MazeRecord; 55],
}

struct MazeRecord {
    walls: [u8; 256],       // +0x000
    properties: [u8; 256],  // +0x100
}
```

The loader seeks to `map_id * 0x200` and reads exactly `0x200` bytes. The two
planes are indexed identically:

```rust
let cell_index = usize::from(y) * 16 + usize::from(x);
let wall = record.walls[cell_index];
let property = record.properties[cell_index];
```

Coordinates range from 0 to 15. In the engine's coordinate system:

```text
+y = north
+x = east
-y = south
-x = west
```

Increasing Y is therefore north in game space, even if an editor chooses to
draw north at the top and consequently reverses rows for display.

### Wall byte

The wall byte contains four directional two-bit values:

```text
bits 7..6  north
bits 5..4  east
bits 3..2  south
bits 1..0  west
```

```rust
struct CellWalls {
    north: u8,
    east: u8,
    south: u8,
    west: u8,
}

fn decode_walls(value: u8) -> CellWalls {
    CellWalls {
        north: (value >> 6) & 3,
        east: (value >> 4) & 3,
        south: (value >> 2) & 3,
        west: value & 3,
    }
}
```

The values are:

| Value | Meaning |
| ---: | --- |
| 0 | No wall |
| 1 | Normal wall |
| 2 | Door |
| 3 | Torch/special wall appearance |

The renderer extracts the low and high bit of the selected directional pair
and chooses one of three wall-graphics banks. This proves that value 3 is the
two-bit value `11`, not a separate four-bit wall code.

The engine can inspect both sides of an edge. Most adjoining cells agree, but
perfect symmetry should not be required: one-sided faces and map-specific
behavior exist in the source data.

### Property byte

The second plane is not another wall plane. Its currently established bits are:

| Bit | Mask | Meaning |
| ---: | ---: | --- |
| 0 | `0x01` | Block/restrict westward movement |
| 1 | `0x02` | Map-specific property; unresolved globally |
| 2 | `0x04` | Block/restrict southward movement |
| 3 | `0x08` | Map-specific property; unresolved globally |
| 4 | `0x10` | Block/restrict eastward movement |
| 5 | `0x20` | Dark cell |
| 6 | `0x40` | Block/restrict northward movement |
| 7 | `0x80` | Invoke the active overlay's special-event logic |

Movement isolates directional restrictions with `property & 0x55`. Rendering
tests `0x20` for darkness, and normal map processing tests `0x80` before calling
the overlay event dispatcher. Bits 1 and 3 must remain raw until their
map-specific consumers have been catalogued.

### Mutability

`MM.EXE` opens and reads `MAZEDATA.DTA`; no maze writer exists. The on-disk file
is immutable world data. Runtime changes such as an opened door or one-shot
event belong in game state rather than being written into this file.

# Map overlays

## Purpose

The `.OVR` files are raw loadable 8086 modules containing map-specific code and
initialized data. They are neither compressed map files nor standalone DOS MZ
executables.

Evidence includes:

- a common loader header;
- sustained, coherent 8086 control flow in the first payload;
- calls to resident `MM.EXE` routines;
- fixed linked addresses;
- map-specific descriptor tables, strings, and mutable variables; and
- an `MM.EXE` diagnostic explicitly referring to an overlay-load error.

## Header

Every supplied overlay has this seven-word header:

```rust
struct OverlayHeader {
    magic: u16,       // 0x00F2
    code_addr: u16,   // 0xF48F in the supplied set
    code_size: u16,
    data_addr: u16,   // 0xC940
    data_size: u16,
    extras_size: u16, // 0 in the supplied set
    entry_addr: u16,
}
```

On disk:

```text
+0x00  14-byte OverlayHeader
+0x0E  code_size bytes
        data_size bytes
```

For all 55 overlays:

```text
file_size = 14 + code_size + data_size + extras_size
extras_size = 0
```

Representative values:

| Overlay | Code size | Data size | Entry address |
| --- | ---: | ---: | ---: |
| `SORPIGAL.OVR` | `0x0346` | `0x04E0` | `0xF797` |
| `AREAA1.OVR` | `0x0253` | `0x0196` | `0xF6A4` |
| `CAVE1.OVR` | `0x03DD` | `0x02F6` | `0xF82E` |
| `ALAMAR.OVR` | `0x0447` | `0x03C4` | `0xF898` |
| `ASTRAL.OVR` | `0x02B5` | `0x0530` | `0xF706` |

The original loader reads 14 bytes, verifies `0x00F2`, loads both payloads at
their fixed addresses, and invokes `entry_addr`. There is no relocation table.

The entry is primarily compiler overlay machinery: it saves/restores runtime
state, initializes overlay globals, and publishes the map's normal event
callback and data address. It is not itself the per-cell event handler.

## Common data descriptor

The second overlay payload begins with a common map descriptor. Established
offsets relative to `data_addr` are:

| Offset | Type | Meaning |
| ---: | --- | --- |
| `0x00` | `u8` | Map/town ID |
| `0x01` | `u8` | Wall-set area |
| `0x02` | `u16` | First wall-resource ID |
| `0x04` | `u16` | Second wall-resource ID |
| `0x06` | `u16` | Third wall-resource ID |
| `0x08` | `u16` | North exit map ID |
| `0x0A` | `u8` | North destination section/coordinate |
| `0x0B` | `u16` | East exit map ID |
| `0x0D` | `u8` | East destination section/coordinate |
| `0x0E` | `u16` | South exit map ID |
| `0x10` | `u8` | South destination section/coordinate |
| `0x11` | `u16` | West exit map ID |
| `0x13` | `u8` | West destination section/coordinate |
| `0x16` | `u8` | Flee threshold |
| `0x17` | `u8` | Flee destination X |
| `0x18` | `u8` | Flee destination Y |
| `0x19` | `u8` | Surrender threshold |
| `0x1A` | `u8` | Surrender destination X |
| `0x1B` | `u8` | Surrender destination Y |
| `0x1C` | `u8` | Bribe threshold |
| `0x22` | `u8` | Maximum monsters |
| `0x23` | `u8` | Sector parameter 1 |
| `0x24` | `u8` | Sector parameter 2 |
| `0x25` | `u8` | Map type |
| `0x26` | `u8` | Dispel threshold |
| `0x27` | `u16` | Surface map ID |
| `0x29` | `u8` | Surface destination section |
| `0x2A` | `u8` | Surface destination X |
| `0x2B` | `u8` | Surface destination Y |
| `0x2E` | `u8` | Map flags |
| `0x30` | `u8` | Trap threshold |
| `0x32` | `u8` | Special-event count |

Unlisted bytes in the first `0x32` bytes should be preserved losslessly until
their generic engine consumers are fully decoded.

## Special-event table

The common dispatcher treats the data beginning at offset `0x32` as parallel
arrays:

```text
+0x32                         event_count: u8
+0x33                         coordinates[event_count]: u8
+0x33 + event_count           direction_masks[event_count]: u8
+0x33 + 2 * event_count       handler_addresses[event_count]: u16
```

Coordinates pack two four-bit components:

```rust
fn unpack_coordinate(value: u8) -> (u8, u8) {
    (value & 0x0f, value >> 4)
}
```

Conceptual dispatch:

```rust
fn dispatch(map: &Map, party: &Party) -> EventResult {
    let coordinate = party.position.pack();

    if let Some(event) = map.events.iter().find(|event| {
        event.coordinate == coordinate
            && event.direction_mask & party.direction_mask != 0
    }) {
        run_event(event)
    } else {
        run_default_map_behavior(map)
    }
}
```

The order is significant: the original code scans from entry zero upward and
uses the first matching coordinate/mask. Several coordinates can share one
handler address.

Observed masks include `0x03`, `0x0C`, `0x30`, `0xC0`, `0x3C`, `0xCC`, `0xF0`,
and `0xFF`. A native importer should initially retain the raw mask rather than
prematurely reducing all cases to a four-value direction enum.

## Representative event behavior

Overlay code and strings directly establish behavior such as:

- town inns, food shops, temples, training, and blacksmiths;
- stairs, map-edge transitions, caves, passages, and trap doors;
- fixed and generated encounters;
- attribute increases and resource deductions;
- quest prerequisites and per-character flags;
- castle guards, traps, teleporters, and arena logic;
- yes/no and numeric prompts;
- one-shot mutable overlay flags; and
- endgame scoring and rewards in `ASTRAL.OVR`.

For example, a `CAVE1` stair handler loads registers with a destination map,
packed destination coordinate, and facing value before transferring to the
resident `loadnext` routine. In Rust this should become a typed result:

```rust
enum EventResult {
    Continue,
    BlockMovement,
    StartEncounter(EncounterSpec),
    Transition {
        map: MapId,
        position: Coord,
        facing: Direction,
    },
    OpenService(Service),
    Teleport(TeleportSpec),
}
```

## What should and should not be preserved

The native implementation must preserve:

- map descriptors and exits;
- ordered events and raw eligibility masks;
- event side effects and return behavior;
- shared event behavior;
- encounters and monster initialization;
- prompts and text;
- initial and mutable map state; and
- transition destinations and facing.

It does not need to preserve:

- fixed addresses `0xF48F` and `0xC940`;
- segmented memory;
- the overlay stack/register wrapper;
- absolute 8086 handler pointers;
- callback cells in `MM.EXE`;
- the original register calling convention; or
- loader memory-overlap checks.

# Graphics resources

## Shared RLE codec

`SCREEN*`, `MONPIX.DTA`, and `WALLPIX.DTA` use the same byte-oriented codec.
Each compressed object begins with its compressed payload length:

```rust
struct CompressedObject {
    compressed_size: u16,
    compressed: [u8; compressed_size],
}
```

The stream grammar is:

```text
byte other than 0x7B: emit it once
0x7B, count_minus_one, value: emit value count_minus_one + 1 times
```

Decoder pseudocode:

```rust
fn decode_rle(input: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut i = 0;

    while i < input.len() {
        let value = input[i];
        i += 1;

        if value != 0x7b {
            output.push(value);
        } else {
            let count = usize::from(input[i]) + 1;
            let repeated = input[i + 1];
            i += 2;
            output.extend(std::iter::repeat_n(repeated, count));
        }
    }

    output
}
```

A literal `0x7B` is encoded as `7B 00 7B`. The maximum single run is 256
bytes.

## Pixel packing and stream traversal

Every decoded graphics byte contains four two-bit pixels, most-significant pair
first:

```text
pixel 0 = bits 7..6
pixel 1 = bits 5..4
pixel 2 = bits 3..2
pixel 3 = bits 1..0
```

```rust
fn unpack_pixels(value: u8) -> [u8; 4] {
    [
        (value >> 6) & 3,
        (value >> 4) & 3,
        (value >> 2) & 3,
        value & 3,
    ]
}
```

The RLE stream traverses four-pixel-wide byte columns rather than scanlines.
For an image with `row_stride = width / 4`:

```rust
for byte_x in 0..row_stride {
    for y in 0..height {
        row_major[y * row_stride + byte_x] = next_decoded_byte();
    }
}
```

Equivalently, expanded stream index `k` maps as:

```text
byte_x = k / height
y      = k % height
destination = y * row_stride + byte_x
```

No palette is embedded. Values 0 through 3 are logical colors translated by
adapter-specific rendering code.

## `SCREEN0`–`SCREEN9`

Each screen consists of one compressed object with no index:

```text
+0x00 compressed_size: u16
+0x02 compressed bytes
```

For all ten files:

```text
file_size = 2 + compressed_size
decoded_size = 16,000 bytes
```

The decoded format is:

```text
width       320 pixels
height      200 pixels
depth       2 bits per pixel
row stride  80 bytes
```

The arithmetic is exact:

```text
320 × 200 × 2 / 8 = 16,000
```

Applying the column traversal and most-significant-pair-first unpacking produces
coherent, correctly oriented title artwork and text.

## Indexed graphics container

`MONPIX.DTA` and `WALLPIX.DTA` share this container:

```text
+0x00 index_bytes: u16
+0x02 offsets[index_bytes / 4]: u32
+0x02 + index_bytes object_data
```

Each offset is relative to `object_data`, not the start of the file. Thus:

```rust
let object_data_start = 2 + index_bytes;
let record_start = object_data_start + offsets[index];
```

The record begins with the shared `u16 compressed_size` and RLE payload. The
offset table and each record's own compressed length agree exactly.

## `MONPIX.DTA`

Exact container values:

```text
file size          81,872
index_bytes        304 (0x0130)
offset count       76
object-data start  306 (0x0132)
```

Every one of the 76 records expands to exactly 2,496 bytes. Each is one
104×96, two-bit-per-pixel monster image:

```text
104 × 96 × 2 / 8 = 2,496
row stride = 26 bytes
```

There is no second 2,496-byte mask payload. The original graphics path derives
the adapter-specific shape and transparency behavior from the decoded data.
The exact logical-color transparency rule should be preserved from the drawing
code rather than guessed as “color zero is always transparent.”

## `WALLPIX.DTA`

Exact container values:

```text
file size          123,059
index_bytes        72 (0x0048)
offset count       18
object-data start  74 (0x004A)
```

Every wall-set record expands to 11,200 bytes. It is not one 224×200 image.
Instead, each record contains twelve sequential two-bit-per-pixel perspective
components:

| Component | Dimensions | Packed bytes |
| ---: | ---: | ---: |
| 0 | 32×128 | 1,024 |
| 1 | 40×96 | 960 |
| 2 | 24×64 | 384 |
| 3 | 16×32 | 128 |
| 4 | 32×128 | 1,024 |
| 5 | 40×96 | 960 |
| 6 | 24×64 | 384 |
| 7 | 16×32 | 128 |
| 8 | 176×96 | 4,224 |
| 9 | 96×64 | 1,536 |
| 10 | 48×32 | 384 |
| 11 | 16×16 | 64 |
| **Total** | | **11,200** |

The first eight pieces form paired side-wall perspective shapes at four depths;
the final four are front-facing wall pieces at decreasing depths. Rendering
positions are runtime knowledge and are not included as metadata in the file.
The following canonical positions within the 240×128 maze viewport are
corroborated by ScummVM's MM1 renderer:

| Depth | Left `(x, y)` | Right `(x, y)` | Front `(x, y)` |
| ---: | ---: | ---: | ---: |
| 0 (near) | `(0, 0)` | `(208, 0)` | `(32, 16)` |
| 1 | `(32, 16)` | `(168, 16)` | `(72, 32)` |
| 2 | `(72, 32)` | `(144, 32)` | `(96, 48)` |
| 3 (far) | `(96, 48)` | `(128, 48)` | `(112, 56)` |

The native asset browser previews each set as a representative corridor using
all eight side pieces and the farthest front piece. The other front pieces are
mutually exclusive alternatives selected by actual map geometry, so drawing all
twelve components at once would not represent a valid player view.

## Graphics adapters

The assets are adapter-neutral packed four-color data. `MM.EXE` selects a
rendering path based on `GACARD.DTA`:

- CGA receives mostly direct packed transfers into its banked framebuffer.
- Hercules converts/dithers the logical colors to monochrome banked output.
- Tandy converts to its banked graphics representation.
- EGA expands logical pixels into hardware planes at runtime.

The source files are not EGA-planar and should be decoded once into logical
color indices in the Rust engine.

# Character roster

## File framing

`ROSTER.DTA` is exactly 2,304 bytes:

```text
18 × 127-byte character records = 2,286 (0x08EE)
18 × 1-byte roster metadata     =    18
                                  -----
                                  2,304 (0x0900)
```

```rust
struct RosterFile {
    characters: [CharacterRecord; 18],
    presence_or_town: [u8; 18],
}
```

The final 18 bytes in the supplied file are:

```text
01 01 01 01 01 01 00 00 00 00 00 00 00 00 00 00 00 00
```

They correspond to six populated slots followed by twelve empty slots. The
external MM1 loader also models these as character presence/town values, so a
value may carry more information than a boolean in other saves.

The 127-byte stride is exact. Record 1 begins at file offset `0x7F` with
`SIR GALAND`. Treating the file as 18 records of 128 bytes causes every later
name and field to drift by one byte.

## Character schema

Each pair below is serialized as two adjacent bytes. Depending on the field,
the pair represents initial/current, base/current, or current/maximum state.
The exact order shown follows the independently implemented MM1 serializer;
fresh characters commonly have equal values, so the supplied roster alone
cannot distinguish pair order.

| Offset | Size | Field |
| ---: | ---: | --- |
| `0x00` | 16 | NUL-padded name; at most 15 visible characters |
| `0x10` | 1 | Sex |
| `0x11` | 1 | Initial alignment |
| `0x12` | 1 | Current alignment |
| `0x13` | 1 | Race |
| `0x14` | 1 | Class |
| `0x15` | 2 | Intellect pair |
| `0x17` | 2 | Might pair |
| `0x19` | 2 | Personality pair |
| `0x1B` | 2 | Endurance pair |
| `0x1D` | 2 | Speed pair |
| `0x1F` | 2 | Accuracy pair |
| `0x21` | 2 | Luck pair |
| `0x23` | 2 | Level pair |
| `0x25` | 1 | Age in years |
| `0x26` | 1 | Age/rest-day counter |
| `0x27` | 4 | Experience |
| `0x2B` | 2 | Current spell points |
| `0x2D` | 2 | Maximum/base spell points |
| `0x2F` | 2 | Spell-level pair |
| `0x31` | 2 | Gems |
| `0x33` | 2 | Current hit points |
| `0x35` | 2 | Temporary/effective maximum hit points |
| `0x37` | 2 | Base maximum hit points |
| `0x39` | 3 | Gold, unsigned 24-bit integer |
| `0x3C` | 2 | Armor-class pair |
| `0x3E` | 1 | Food |
| `0x3F` | 1 | Condition |
| `0x40` | 6 | Equipped item IDs |
| `0x46` | 6 | Backpack item IDs |
| `0x4C` | 6 | Equipped-item charges/uses |
| `0x52` | 6 | Backpack-item charges/uses |
| `0x58` | 16 | Eight resistance pairs |
| `0x68` | 2 | Physical-attack attribute pair |
| `0x6A` | 2 | Missile-attack attribute pair |
| `0x6C` | 1 | Trap counter |
| `0x6D` | 1 | Active quest |
| `0x6E` | 1 | Worthiness |
| `0x6F` | 1 | Alignment counter |
| `0x70` | 14 | Persistent flags |
| `0x7E` | 1 | Roster/portrait index |

Conceptual Rust representation:

```rust
struct ValuePair {
    base: u8,
    current: u8,
}

struct CharacterRecord {
    name: [u8; 16],
    sex: u8,
    initial_alignment: u8,
    current_alignment: u8,
    race: u8,
    class: u8,
    intellect: ValuePair,
    might: ValuePair,
    personality: ValuePair,
    endurance: ValuePair,
    speed: ValuePair,
    accuracy: ValuePair,
    luck: ValuePair,
    level: ValuePair,
    age: u8,
    age_counter: u8,
    experience: u32,
    current_spell_points: u16,
    maximum_spell_points: u16,
    spell_level: ValuePair,
    gems: u16,
    current_hp: u16,
    effective_max_hp: u16,
    base_max_hp: u16,
    gold: U24,
    armor_class: ValuePair,
    food: u8,
    condition: u8,
    equipped_items: [u8; 6],
    backpack_items: [u8; 6],
    equipped_charges: [u8; 6],
    backpack_charges: [u8; 6],
    resistances: [ValuePair; 8],
    physical_attribute: ValuePair,
    missile_attribute: ValuePair,
    trap_counter: u8,
    active_quest: u8,
    worthiness: u8,
    alignment_counter: u8,
    persistent_flags: [u8; 14],
    roster_index: u8,
}
```

This is a semantic representation, not a suggestion to use Rust's native
in-memory struct layout for parsing. Read fields explicitly to avoid padding and
endianness errors.

### Enum values

Established values are:

```text
Sex
  1 male
  2 female

Alignment
  1 good
  2 neutral
  3 evil

Race
  1 human
  2 elf
  3 dwarf
  4 gnome
  5 half-orc

Class
  1 knight
  2 paladin
  3 archer
  4 cleric
  5 sorcerer
  6 robber
```

### Supplied characters

| Slot | Name | Sex | Alignment | Race | Class |
| ---: | --- | --- | --- | --- | --- |
| 0 | `CRAG THE HACK` | Male | Neutral | Human | Knight |
| 1 | `SIR GALAND` | Male | Good | Dwarf | Paladin |
| 2 | `ZENON III` | Male | Evil | Half-Orc | Archer |
| 3 | `SWIFTY SARG` | Male | Neutral | Gnome | Robber |
| 4 | `SERENA` | Female | Good | Human | Cleric |
| 5 | `WIZZ BANE` | Male | Good | Elf | Sorcerer |

Slots 6 through 17 have empty names and zeroed state, but their final record
byte still contains their zero-based slot number.

### Persistence behavior

The original game saves characters at inns. `MM.EXE` reads or writes the full
`0x900`-byte roster file at once. There is no per-record checksum, file
checksum, encryption, or obfuscation.

The roster is not a complete arbitrary-position game snapshot. It stores
character progression and quest-related state, but not all runtime state needed
to resume in the middle of a map or encounter.

# Configuration and executable metadata

## `GACARD.DTA` and `GRAPHSET.EXE`

`GACARD.DTA` consists of one byte:

```rust
struct GraphicsConfiguration {
    adapter: u8,
}
```

Values written by `GRAPHSET.EXE` are:

| Value | Adapter |
| ---: | --- |
| 0 | CGA |
| 1 | Hercules |
| 2 | Tandy 1000 |
| 3 | EGA |

The supplied file contains `0x03`, selecting EGA.

`GRAPHSET.EXE` prompts for choices 1 through 4, subtracts ASCII `'1'`, creates
or truncates `gacard.dta`, and writes the resulting byte. It does not open or
convert `SCREEN*`, `MONPIX.DTA`, or `WALLPIX.DTA`.

## `MM.RSM`

`MM.RSM` is a symbol map for `MM.EXE`, not a game resource or save file. It
contains approximately 578 named code and data symbols, including:

```text
ovloader_  readmaze_  readrost_  writrost_  readwall_  readmon_
training   temple     tavern     combat     create     setspell
saveros    viewch     equip      trade      setcondi   useitem
comcast    roster_    endroster  mondata
```

Normal entries have this packed shape:

```rust
struct RsmSymbol {
    kind: u8,
    marker: u8,   // generally 0x28
    address: u16,
    name: CStr,
}
```

Names are NUL-terminated and records have no alignment padding. Observed kinds
primarily distinguish code and data symbols.

`MM.EXE` has a `0x200`-byte MZ header. For ordinary code symbols in this file:

```text
raw executable file offset = RSM code address + 0x200
```

For example, `readrost_` at symbol address `0x0188` appears at raw executable
offset `0x0388`.

The symbol addresses also independently verify roster framing:

```text
roster_   = 0x3CFA
endroster = 0x45E8
difference = 0x08EE = 18 × 127
```

# Save-game implications for the Rust engine

The original roster format should be treated as a compatibility import/export
format, not as the new engine's complete save representation.

A resumable native save needs at least:

```rust
struct SaveGame {
    format_version: u32,
    characters: Vec<Character>,
    party: Vec<CharacterId>,
    active_map: MapId,
    position: Coord,
    facing: Direction,
    map_states: Map<MapId, MapRuntimeState>,
    encounter: Option<EncounterState>,
    runtime_flags: RuntimeFlags,
    rng_state: RngState,
}
```

The static/runtime boundary should be:

```text
MAZEDATA.DTA ──► immutable wall and cell-property definitions
      .OVR    ──► immutable descriptors, event definitions, text, initial state
Rust save     ──► party, position, mutable events/maps, encounters, RNG state
```

In particular, a save capable of resuming at any moment should capture:

- active party composition and ordering;
- the complete character state;
- map ID, coordinate, and facing;
- opened/unlocked doors if the new engine makes them persistent;
- one-shot event flags and map-local counters;
- active quests and global story flags;
- encounter participants, initiative, conditions, and turn state when saving in
  combat is allowed;
- temporary spell effects and light/darkness state;
- inventory transaction state if saving inside services is allowed; and
- random-number generator state for deterministic continuation.

The original engine may reset some overlay-local variables on reload. The Rust
design should make a deliberate compatibility decision for each such variable
rather than accidentally serializing every implementation detail.

# Remaining reverse-engineering work

The binary containers are sufficiently understood to write lossless readers.
The remaining work is primarily semantic:

1. Name every unidentified byte in the overlay common descriptor.
2. Translate all 55 overlay event handlers into typed event definitions.
3. Catalogue event return values and their effects on movement/dispatch.
4. Map all character condition and resistance enum values.
5. Identify every item, charge behavior, spell, and monster ID.
6. Map the 14 persistent character-flag bytes to quests and accomplishments.
7. Determine which overlay-local mutable variables reset on map entry and which
   represent meaningful session state.
8. Record the exact rendering coordinates for the twelve wall components.
9. Reconstruct original hardware palette choices where visual compatibility is
   desired.

Until these are resolved, importers should preserve unknown fields and bits
verbatim. Unknown bytes should not be normalized to zero or represented as
exhaustive enums.

# External corroboration

The following external implementations were used only to cross-check details
that were first inferred from the supplied artifacts:

- [ScummVM MM1 engine](https://github.com/scummvm/scummvm/tree/master/engines/mm/mm1)
  implements the original MM1 DOS formats, including roster serialization, map
  loading, overlay-data access, RLE decoding, and image dimensions.
- [chikuzen/mm1cheat](https://github.com/chikuzen/mm1cheat) independently models
  the roster as 18 packed 127-byte records followed by 18 metadata bytes.
- [lagdotcom/might-and-magic-1-graphics-tools](https://github.com/lagdotcom/might-and-magic-1-graphics-tools)
  independently implements the indexed graphics containers, `0x7B` RLE,
  column traversal, and MM1 image dimensions.

Code for later Might and Magic games, including the Xeen engine, was not used
to infer MM1 formats.
