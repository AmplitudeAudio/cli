# Amplitude Audio CLI

The official CLI tool to manage your Amplitude projects, local resources, and licenses.

## Prerequisites

- **Rust** (Edition 2024, requires Rust 1.85+) - [Install via rustup](https://rustup.rs/)
- **Amplitude Audio SDK** - Required at build time for FlatBuffer schema code generation
  - Clone from: https://github.com/AmplitudeAudio/sdk
  - Set the `AM_SDK_PATH` environment variable to point to the SDK root directory

## Quick Start

### Linux / macOS

```bash
# Clone the repository and the Amplitude SDK
git clone https://github.com/AmplitudeAudio/cli.git
git clone https://github.com/AmplitudeAudio/sdk.git

# Set AM_SDK_PATH to the absolute path of the cloned SDK
export AM_SDK_PATH="$(pwd)/sdk"

# Build and test
cd cli
cargo build
cargo test
```

### Windows

```powershell
# Clone the repository and the Amplitude SDK
git clone https://github.com/AmplitudeAudio/cli.git
git clone https://github.com/AmplitudeAudio/sdk.git

# Set AM_SDK_PATH to the absolute path of the cloned SDK
$env:AM_SDK_PATH = (Resolve-Path sdk).Path

# Build and test
cd cli
cargo build
cargo test
```

## Development

```bash
cargo fmt              # Format code
cargo clippy           # Run linter
cargo test             # Run all tests
cargo run -- --verbose # Run with debug logging
cargo run -- --help    # Show available commands
```

Tests use priority tags (P0 = critical, P1 = high, P2 = medium, P3 = low) for selective execution during local development:

```bash
cargo test p0          # Critical tests only (fastest)
cargo test p1          # High priority tests
```

## Asset commands

Every SDK asset type can be created, listed, updated and deleted through `am asset`:

| Command | Manages |
|---------|---------|
| `am asset sound`, `collection`, `effect`, `switch`, `switch-container`, `event`, `soundbank` | assets under `sources/` |
| `am asset attenuation`, `rtpc`, `pipeline` | attenuators, RTPCs and processing pipelines under `sources/` |
| `am asset bus` | buses inside a `*.buses.json` file |
| `am asset bus-file` | `<name>.buses.json` files in the sources dir (project root accepted; the last one cannot be deleted) |
| `am asset engine-config` | `<name>.config.json` files in the sources dir (project root accepted; the last one cannot be deleted) |

Each type has `create`, `list`, `update` and `delete` (`bus-file` has no `update`); run `am asset <type> --help` for the flags. Examples:

```bash
am asset sound create explosion --file audio/explosion.wav --bus sfx
am asset sound update explosion --gain 0.5
am asset sound delete explosion --force   # --yes is an alias
am asset rtpc create wind --min-value 0 --max-value 10
am asset attenuation create room --max-distance 50 --shape sphere --inner-radius 5 --outer-radius 10 --curve-part 0,1,1,0,Linear
am project validate --rtpc-only
```

Every `update` keeps the keys it does not know about, so hand-edited files are safe. References between assets (buses, attenuations, effects, switch groups, event targets, soundbank entries) are strict: they must exist, and `delete` refuses to remove an asset that is still referenced. Soundbank entries use runtime extensions; effects are compiled to `.amfx`.

`am project validate` checks every asset type plus the bus and engine config files. `--sounds-only`, `--collections-only`, `--effects-only`, `--switches-only`, `--switch-containers-only`, `--events-only`, `--soundbanks-only`, `--attenuations-only`, `--rtpc-only`, `--pipelines-only`, `--buses-only` and `--engine-configs-only` narrow the check. The RTPC rules (`min_value < max_value`, `default_value` inside the range) are stricter than the SDK, which clamps or yields NaN; they are reported as errors.

Tests change the working directory, so run them single-threaded and point `AM_SDK_PATH` at the SDK:

```bash
AM_SDK_PATH=../sdk cargo test --no-fail-fast -- --test-threads=1
```

## Breaking changes

Release notes are generated from commit messages, so the behavioural changes that affect scripts, agents and CI are listed here.

- An asset reference of ID `0` and a reference to an asset that does not exist are now validation errors (`ERR_VALIDATION_REFERENCE`, `ERR_VALIDATION_EMPTY_REFERENCE`) in `create`, `update` and `project validate`. Reference flags accept names or numeric IDs.
- `--bus` is required when creating a sound, a collection or a switch container (no prompt in `--json` or non-interactive runs; the error names the flag).
- `am asset switch-container create` requires `--switch-group`.
- `am asset effect create` requires `--effect-type`.
- A hand-written collection with a non-empty `sounds` list and no `sounds_type` no longer parses.
- `update` commands preserve unknown top-level keys of the file instead of dropping them.
- `delete` refuses an asset that is still referenced (`ERR_ASSET_IN_USE`, naming the assets, soundbanks or engine configs that use it). `--force` / `--yes` do not override it.
- `am build` writes effects as `.amfx` (previously `.amenv`); soundbank effect entries use `.amfx`.
- Switch state IDs are preserved across edits instead of being renumbered.
- Soundbanks always carry all eight asset lists, and an empty soundbank is valid.
- Files written by the previous CLI that the SDK could never load (bus ID `0`, a bus without `fader`, `null` fade blocks) now fail `update` until they are repaired; the error names the fix.
- `am project validate` now also checks attenuations, RTPCs, pipelines, `*.buses.json` and `*.config.json` files, so projects that passed before can report new errors. `am asset sound delete` is new.

## Install from release

Pre-built binaries are published on every release at
<https://github.com/AmplitudeAudio/cli/releases>.

Each release archive contains:

- `am` (or `am.exe` on Windows)
- Shell completion scripts under `completions/`
- `README.md` and `LICENSE`

### Supported targets

- Linux x86_64 (`x86_64-unknown-linux-gnu`)
- Linux aarch64 (`aarch64-unknown-linux-gnu`)
- macOS Intel (`x86_64-apple-darwin`)
- macOS Apple Silicon (`aarch64-apple-darwin`)
- Windows x86_64 (`x86_64-pc-windows-msvc`)

### Verification

Each archive ships with a sibling `.sha256` file. SLSA build provenance
attestations are attached to every release; verify with `gh attestation verify`.
