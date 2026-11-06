# CLAUDE.md

Rust (edition 2024) tool that extracts password-protected zips, with a CLI and an optional egui GUI.

## Commands

- `cargo build` / `cargo build --no-default-features` (CLI only, no eframe/rfd)
- `cargo test` (unit tests live in `src/core.rs`)
- `cargo clippy --all-features` and `cargo clippy --no-default-features`: check both feature configurations

## Layout

- `src/core.rs`: front-end independent logic (`find_zip_files`, `extract_zip`, `run`, `Event`, `parse_password`). Keep it free of CLI/GUI dependencies.
- `src/main.rs`: clap args and entry point. No args and no `RENTGENY_PASSWORD` starts the GUI; otherwise the CLI runs.
- `src/gui.rs`: egui front end, compiled only with the `gui` feature. Persists last input/output folders.

## Conventions

- `extract_zip` verifies all entries before writing, so failures must leave the output dir untouched. Preserve this.
- `run` reports per-archive failures as events; only invalid directories return `Err`. Cancellation is checked between archives.
- GUI code must stay behind `#[cfg(feature = "gui")]`.
- `test.jpg` and `test/out/` are untracked scratch files; don't commit them.
