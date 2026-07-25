# Project guidance

## Purpose and source material

- This project is a native Rust reimplementation of the DOS release of *Might
  and Magic Book One*, not a DOS emulator.
- Treat files under `dos/` as immutable primary-source artifacts. Do not rewrite,
  normalize, or silently discard bytes from them.
- Record reverse-engineering conclusions in `docs/REFERENCE.md`. Clearly
  distinguish verified behavior, corroborated behavior, and unresolved details.
- Keep proprietary input data logically separate from native engine state and
  code. Do not embed original assets into the executable unless explicitly
  required.

## Runtime invariants

- Preserve a logical 320x200 palette-indexed framebuffer. Convert logical color
  indices to RGBA at the rendering boundary rather than throughout game code.
- Keep game state and behavior independent from `winit`, `pixels`, and window
  lifecycle types. Platform rendering should consume state, not own it.
- The executable must always support `--headless` without initializing a window,
  display server, or graphics device.
- Headless stdout must contain only one valid JSON player-view document. Put
  diagnostics on stderr. Keep `schema_version` explicit and update it when a
  breaking descriptor change is intentional.
- A graphical and headless invocation at the same state must describe the same
  player-visible information.

## Rust conventions

- Use the Rust version pinned in `rust-toolchain.toml` and keep `Cargo.lock`
  committed.
- Prefer small native Rust types for decoded formats and game state. Preserve
  unknown fields losslessly until their semantics are established.
- Follow existing code style and run `cargo fmt`; avoid new frameworks or
  abstractions unless they remove concrete complexity.
- Keep the dependency set small. `pixels` and `winit` are the intended rendering
  and windowing stack for the current engine scaffold.

## Verification

For code changes, run the narrowest relevant checks and normally finish with:

```sh
cargo fmt --check
cargo check --locked
cargo test --locked
cargo run --quiet --locked -- --headless
```

Parse headless output as JSON in tests; do not validate it only as a text
snapshot. For graphical changes, also launch the executable and inspect a real
or virtual-display capture when the environment permits.

The `.agents/setup` script must remain executable, non-interactive, and
idempotent. When changing native graphics dependencies, validate setup and a
graphical smoke test in a fresh orb.

## Documentation

- Keep `README.md` aligned with commands, dependencies, current behavior, and
  known scaffold limitations.
- Update `docs/REFERENCE.md` when a change establishes or revises knowledge of
  an original binary format or DOS behavior.
