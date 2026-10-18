# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Amplitude Audio CLI (`am`) - A Rust command-line tool for managing projects that use the [Amplitude Audio SDK](https://github.com/AmplitudeAudio/sdk), a cross-platform audio engine designed for games.

The SDK uses a **data-driven approach** where developers define audio behavior through JSON configuration files rather than code. This CLI tool manages the creation, registration, and organization of these project files and directory structures.

**Local state:** `~/.amplitude/am.db` (SQLite database tracking registered projects and templates)

## Build Commands

```bash
cargo build              # Debug build
cargo build --release    # Release build
cargo run                # Run the CLI
cargo run -- --verbose   # Run with debug/trace logging
cargo run -- --help      # Show available commands
```

## Architecture

### Command Flow
1. `src/main.rs` - Entry point: parses args, initializes logger/database, routes commands, handles signals
2. `src/app.rs` - CLI definition using clap derive macros, defines `Commands` enum
3. `src/commands/` - Command handlers (project.rs, sudo.rs, asset/)
   - `src/commands/asset/` holds one module family per asset type: `<type>.rs` (clap subcommands and routing), `<type>_args.rs` (flags), `<type>_prompts.rs` (interactive flow) and `<type>_list.rs` (list/delete where split out). Never prompt in `--json` or non-interactive runs: fail with an error naming the missing flag.
   - Asset commands: `am asset sound|collection|effect|switch|switch-container|event|soundbank|attenuation|rtpc|pipeline create|list|update|delete`, plus `am asset bus ...` (buses inside a `*.buses.json` file), `am asset bus-file ...` and `am asset engine-config ...` (named files in the project root; deleting the last one is refused).
   - `am project validate` checks every asset type and the `*.buses.json` / `*.config.json` files; `--*-only` flags narrow it (`src/commands/project_root_files.rs` handles the root-level files).

### Forms and services (`src/forms/`, `src/services/`)
- `src/forms/` is the front-end independent field model: `field.rs` (field kinds), `coverage.rs` (which schema fields create/update cover), `pick.rs` (reference pickers), `rtpc_value.rs`, `curve.rs`, `zone.rs`, `soundbank_refs.rs` and `in_use.rs`.
- `src/services/` holds the create/update logic shared by flags, prompts and (later) the TUI. Services return new values and never mutate their inputs; commands only gather values.
- **References are strict.** An ID of 0 or an ID that resolves to nothing is a validation error (`ERR_VALIDATION_REFERENCE` / `ERR_VALIDATION_EMPTY_REFERENCE`) in create, update and `project validate`. Flags accept names or numeric IDs. The reference rules live in `forms::references` (a hand-maintained table that a test checks against the schema); `forms::validate_refs` applies them.
- **Deletes check usage first.** `forms::in_use::assert_not_in_use` runs before any confirmation and refuses with `ERR_ASSET_IN_USE`, naming the referrers (assets and soundbanks); `--force` / `--yes` never override it.
- Update commands merge into the existing JSON (`common::json_merge`), so unknown top-level keys of hand-edited files survive.
- Soundbank entries use runtime (binary) extensions (`.amsound`, `.amfx` for effects, ...); validation maps them back to the `.json` sources. `am build` writes effects as `.amfx` (older CLI versions wrote `.amenv`, still accepted in existing banks).
- Scalar fields with a zero schema default carry `#[serde(default)]` in the build.rs-generated types, so an absent key parses like the SDK's flatc loader reads it.

### Database Layer (`src/database/`)
- `connection.rs` - Thread-safe SQLite wrapper using `Arc<Mutex<Connection>>`
- `migrations.rs` - Version-based schema migrations with checksum verification
- `entities.rs` - Data models (`Project`, `Template`, `ProjectConfiguration`)
- `mod.rs` - CRUD helper functions (`db_get_*`, `db_create_*`, etc.)

### Logging (`src/common/logger.rs`)
- Custom `log` trait implementation with colored console output
- In-memory buffer (1000 entries) for crash log writing to `~/.amplitude/`
- Use `success!()` macro for green checkmark success messages

### Resource Embedding
Default project templates in `resources/` are compiled into the binary via `rust-embed`. These templates define starting configurations for SDK features (buses, pipelines, audio configs).

## Testing

```bash
cargo test --no-fail-fast -- --test-threads=1
```

- Run with `--test-threads=1`: many tests change the process working directory (`CwdGuard`), which is process-wide.
- `AM_SDK_PATH` must point at a checkout of the Amplitude SDK (schemas are read at build and test time).
- `tests/common/scripted_input.rs` (`ScriptedInput`) drives interactive flows with queued answers and records the prompts; `NonInteractiveInput` asserts that nothing prompts.
- `tests/common/flatc.rs` checks that a written JSON file is accepted by `flatc --binary`; it skips visibly when `flatc` is not installed.
- `AssetTestFixture` (`tests/common/fixtures.rs`) builds a throwaway project with a `main.buses.json`; `CaptureOutput` captures JSON/interactive output.

## Adding New Features

**New Command:**
1. Create handler in `src/commands/your_command.rs`
2. Add variant to `Commands` enum in `src/app.rs`
3. Wire handler in `run_command()` in `src/main.rs`

**Database Changes:**
1. Add migration in `src/database/migrations.rs` (increment version)
2. Add CRUD helpers in `src/database/mod.rs`

## Key Patterns

- Error handling: Use `anyhow::Result<T>` with `?` operator
- Async runtime: Tokio (full features)
- Interactive prompts: `inquire` crate with validators
- Project config stored as `.amproject` JSON files

## SDK Project Structure

Projects created by `am project init` follow the SDK's data-driven architecture:

```
project_name/
├── .amproject           # Project configuration (name, paths, version)
├── sources/             # Audio asset definitions (JSON files)
│   ├── attenuators/     # Distance-based volume falloff curves
│   ├── collections/     # Grouped sound variations
│   ├── effects/         # Audio effects (reverb, EQ, etc.)
│   ├── events/          # Triggerable audio events
│   ├── pipelines/       # Audio processing chains
│   ├── rtpc/            # Real-Time Parameter Controls
│   ├── soundbanks/      # Packaged audio assets for runtime
│   ├── sounds/          # Individual sound definitions
│   ├── switch_containers/ # State-based sound switching
│   └── switches/        # Switch state definitions
├── build/               # Compiled/processed assets output
├── data/                # Raw audio files (wav, ogg, etc.)
└── plugins/             # Custom SDK plugins
```

The `*.config.json`, and `*.buses.json` files in project roots (`am project init` currently writes its defaults under `sources/`; `am project validate` looks in both places) configure the SDK's audio engine, and bus hierarchy (for mixing/ducking) respectively.
