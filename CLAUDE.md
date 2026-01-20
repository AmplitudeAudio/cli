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
3. `src/commands/` - Command handlers (project.rs, sudo.rs)

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

The `*.config.json`, and `*.buses.json` files in project roots configure the SDK's audio engine, and bus hierarchy (for mixing/ducking) respectively.
