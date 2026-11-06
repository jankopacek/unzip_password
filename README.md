# rentgeny

Extract password-protected zip files in bulk. It has a command-line interface and an [egui](https://github.com/emilk/egui) GUI.

`rentgeny` recursively finds every `.zip` under an input directory and extracts each one into an output directory with a given password. Each archive is fully verified (password and CRC) before anything is written. A wrong password or a corrupt archive therefore leaves the output untouched.

## Build

```sh
cargo build --release                  # CLI + GUI
cargo build --release --no-default-features   # CLI only
```

The binary is `target/release/rentgeny`.

## Usage

Start with no arguments (or with `--gui`) to open the GUI. The GUI remembers the last used input and output folders.

CLI:

```sh
rentgeny -p SECRET -i ./archives -o ./extracted [-r]
```

| Option | Description |
| --- | --- |
| `-p, --password` | Password (UTF-8, at most 64 characters). Can also be set via `RENTGENY_PASSWORD`. |
| `-i, --input` | Input directory, searched recursively (default `.`). |
| `-o, --output` | Output directory (default `.`). |
| `-r, --remove-zip` | Delete each zip after successful extraction (kept by default). |
| `--gui` | Open the GUI. |

Per-archive failures are reported on stderr. Only invalid directories make the run fail.

## Test

```sh
cargo test
```
