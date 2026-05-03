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
