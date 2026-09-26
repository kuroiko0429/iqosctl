# iqosctl

<div align="center">

**A command-line interface for controlling IQOS devices via Bluetooth Low Energy — a personal fork of [hauntedfail/iqos_cli](https://github.com/hauntedfail/iqos_cli), built on [hauntedfail/iqos](https://github.com/hauntedfail/iqos)**

[![Rust](https://img.shields.io/badge/rust-1.92%2B-orange.svg?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg?style=flat-square)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg?style=flat-square)](https://github.com/kuroiko0429/iqosctl)

[Features](#-features) • [Installation](#-installation) • [Quick Start](#-quick-start) • [Commands](#-commands-reference) • [Contributing](#-contributing)

</div>

---

## Table of Contents

- [Overview](#-overview)
- [Architecture](#-architecture)
- [Features](#-features)
- [Device Compatibility](#-device-compatibility)
- [Prerequisites](#-prerequisites)
- [Installation](#-installation)
- [Quick Start](#-quick-start)
- [Commands Reference](#-commands-reference)
- [Examples](#-examples)
- [Troubleshooting](#-troubleshooting)
- [Development](#-development)
- [Contributing](#-contributing)
- [License](#-license)

## Overview

iqosctl is a Rust-based command-line tool for controlling IQOS devices over Bluetooth Low Energy, built on top of [hauntedfail/iqos](https://github.com/hauntedfail/iqos). It supports both an interactive REPL and one-shot command execution, so you can either connect once and work from the `iqos>` prompt or run a single command directly from your shell.

This is a fork of [hauntedfail/iqos_cli](https://github.com/hauntedfail/iqos_cli) (the binary is still called `iqos`; only the project/crate name changed). On top of upstream it includes:

- A fix for a panic in `bluez-async`'s D-Bus match cleanup on Linux (harmless but noisy).
- A fix for a write-before-subscribe race in the SCP request/response path that could hang `info`/`diagnosis` indefinitely on some connections, plus a bounded timeout as a backstop.
- A fix for embedded NUL bytes in BLE device-information strings (model/serial/manufacturer/firmware) leaking into JSON output and the saved `config.toml`.
- `--format json` for `battery`, `info`, `diagnosis`, and `device list`.
- `battery --watch [--interval <secs>]` for continuous polling without reconnecting.
- `--retries <attempts>` / `IQOS_MAX_RETRIES` for automatic reconnect-and-retry on transport failures.
- `completions <bash|zsh|fish|elvish|powershell>` for shell completion scripts.
- `diagnosis --raw` to dump every telemetry tag/value block the device reports, including ones the reverse-engineered protocol doesn't decode into a named field yet.
- `info --raw` to dump the full firmware/battery-voltage response frames, which likewise carry undecoded (and non-zero) trailing bytes on real hardware.
- `raw <preset|hex-bytes>` for sending any SCP command and inspecting the raw response — the general-purpose tool behind both of the above.

## Architecture

All device protocol logic — BLE framing, capability negotiation, command encoding, response parsing — lives in the [iqos crate (hauntedfail/iqos)](https://github.com/hauntedfail/iqos). This repository is a thin CLI layer: it handles device discovery, user interaction, and argument parsing, then delegates every device operation to the crate's high-level API.

## Features

- **Automatic Device Discovery** — Scans and connects to IQOS devices via Bluetooth
- **Interactive Console** — REPL with command history (`iqos>` prompt)
- **One-Shot CLI Commands** — Run device commands directly, for example `iqos --model iluma battery`
- **Saved Device Labels** — Remember a connected device and target it later with `--model <label>`
- **Battery Management** — Real-time battery status
- **Brightness Control** — Set LED brightness (all ILUMA models)
- **Vibration Customization** — Configure vibration for heating, puff-end, etc.
- **FlexPuff** — Enable, disable, or check FlexPuff status (ILUMA i / ILUMA i Prime)
- **FlexBattery** — Performance/Eco mode and pause mode (ILUMA i / ILUMA i Prime)
- **Smart Gesture** — Enable/disable smart gesture recognition (ILUMA / ILUMA Prime / ILUMA i / ILUMA i One / ILUMA i Prime)
- **AutoStart** — Automatic heating start (ILUMA i series)
- **Device Lock/Unlock** — Lock and unlock the device
- **Diagnosis** — Puff count, days used, battery voltage
- **Device Status** — Firmware, product number, and voltage snapshot
- **Find My IQOS** — Trigger device vibration for locating

## Device Compatibility

| Feature | ILUMA i | ILUMA i One | ILUMA i Prime | ILUMA | ILUMA ONE | ILUMA Prime |
|---------|:-------:|:-----------:|:-------------:|:-----:|:---------:|:-----------:|
| Battery Status | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Device Info | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Diagnosis | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Find My IQOS | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Device Lock/Unlock | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Vibration Settings | ✅¹ | ✅ | ✅ | ✅¹ | ✅ | ✅ |
| Brightness | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Auto Start | ✅ | ✅ | ✅ | ❌ | ❌ | ❌ |
| Smart Gesture | ✅ | ✅ | ✅ | ✅ | ❌ | ✅ |
| Flex Puff | ✅ | ❌ | ✅ | ❌ | ❌ | ❌ |
| Flex Battery | ✅ | ❌ | ✅ | ❌ | ❌ | ❌ |

¹ The `charge` vibration flag is only available on ILUMA and ILUMA i (holder-based models with charge-start support).

## Prerequisites

- **Bluetooth adapter** — A working Bluetooth adapter on your system
- **Platform-specific dependencies**:

  **Linux:**
  ```bash
  sudo apt-get install libdbus-1-dev pkg-config
  ```

  **macOS / Windows:** No additional dependencies required.

## Installation

### Installer Script (Linux/macOS)

Install the latest GitHub Release without a local Rust toolchain:

```bash
curl -fsSL https://raw.githubusercontent.com/kuroiko0429/iqosctl/main/install.sh | sh
```

If `curl` is unavailable, `wget` also works:

```bash
wget -qO- https://raw.githubusercontent.com/kuroiko0429/iqosctl/main/install.sh | sh
```

The installer detects macOS or Linux, selects the matching release package, verifies `SHA256SUMS.txt` when available, and installs the executable as `iqos`. By default it installs to `~/.local/bin` when possible and falls back to `/usr/local/bin` with `sudo` when needed.

Use these environment variables to customise the install:

```bash
curl -fsSL https://raw.githubusercontent.com/kuroiko0429/iqosctl/main/install.sh | IQOSCTL_VERSION=v1.0.1 sh
curl -fsSL https://raw.githubusercontent.com/kuroiko0429/iqosctl/main/install.sh | IQOSCTL_INSTALL_DIR="$HOME/bin" sh
```

### From Source

Requires **Rust 1.92 or later**. [Install Rust](https://rustup.rs/) first.

```bash
git clone https://github.com/kuroiko0429/iqosctl.git
cd iqosctl
cargo build --release
./target/release/iqos
```

### Via Cargo

Requires **Rust 1.92 or later**.

```bash
cargo install --path .
```

## Quick Start

Examples below use `iqos` as the command name. When running directly from this repository, use `./target/release/iqos` or `cargo run --release --` in its place.

### Interactive Mode

1. Enable Bluetooth on your system
2. Turn on your IQOS device and ensure it's in range
3. Run IQOS CLI:
   ```bash
   iqos
   # or during development
   cargo run --release --
   ```
4. Select your device when prompted:
   ```
   Found IQOS: IQOS3_AABBCC (AA:BB:CC:DD:EE:FF)
   Connect to IQOS3_AABBCC (AA:BB:CC:DD:EE:FF)? [y/N]: y
   ```
5. Use commands in the interactive console:
   ```
   iqos> help
   iqos> battery
   Battery Level: 85%

   iqos> brightness high
   Brightness set to high

   iqos> flexbattery eco
   FlexBattery settings updated
   ```

### One-Shot CLI Mode

Run a single command without opening the REPL:

```bash
iqos --model iluma battery
iqos brightness high --model iluma
iqos vibration heating on --model iluma-i --timeout 5
```

`--model` accepts either a built-in model selector or a saved device label. Global options can be placed before the command, after the command, or between command arguments; the CLI normalizes them before execution.

If you pass only a target option and no subcommand, IQOS CLI connects to that target and then starts interactive mode:

```bash
iqos --model minera
```

This is useful after saving a device label, because it skips the manual "Connect?" prompt and connects directly to the saved device.

## Commands Reference

### CLI Invocation

| Command | Description |
|---------|-------------|
| `iqos` | Scan nearby IQOS devices, ask which one to connect to, then open interactive mode |
| `iqos --help` | Show top-level CLI help |
| `iqos help` | Same as `iqos --help` |
| `iqos -v` / `iqos --version` | Print the IQOS CLI version and exit without scanning |
| `iqos --model <model-or-label>` | Connect to a built-in model selector or saved label, then open interactive mode |
| `iqos --model <model-or-label> <command>` | Connect to the selected target and run one command |
| `iqos <command> --model <model-or-label>` | Same as above; global options may be placed after the command |
| `iqos --timeout <secs> ...` | Override the BLE scan timeout |
| `iqos --format json ...` | Emit machine-readable JSON for `battery`, `info`, `diagnosis`, and `device list` instead of text |
| `iqos --retries <attempts> ...` | Override the maximum connection/command attempts (default `3`; also settable via `IQOS_MAX_RETRIES`) |
| `iqos battery --watch [--interval <secs>]` | Connect once, then keep printing the battery level on an interval (default `2`s) until interrupted with Ctrl+C |
| `iqos completions <bash\|zsh\|fish\|elvish\|powershell>` | Print a shell completion script to stdout |

Built-in model selectors include `iluma`, `iluma-one`, `iluma-prime`, `iluma-i`, `iluma-i-one`, and `iluma-i-prime`. Saved labels are managed with the `device` command.

`-v` / `--version` takes precedence over other arguments before `--`; it prints the CLI version and exits without scanning or connecting.

On any connection or transport failure, IQOS CLI automatically retries the whole operation (re-scan, re-connect, re-run) up to `--retries` times with a short backoff between attempts, printing a warning before each retry. Failures that aren't connection-related (e.g. invalid arguments, an unknown saved label) are never retried.

To enable shell completions, add the output of `iqos completions <shell>` to your shell's completion path, e.g. for bash:

```bash
iqos completions bash | sudo tee /etc/bash_completion.d/iqos > /dev/null
# or, for a user-local install:
mkdir -p ~/.local/share/bash-completion/completions
iqos completions bash > ~/.local/share/bash-completion/completions/iqos
```

### General

| Command | Description |
|---------|-------------|
| `help` | List all available commands |
| `version` | Show the IQOS CLI version |
| `info [--raw]` | Show device model, serial number, GATT metadata, firmware, product number, and battery voltage. `--raw` also dumps the full response frame behind each firmware/voltage read |
| `battery` | Show current battery level |
| `diagnosis [--raw]` | Show puff count, days used, and battery voltage. `--raw` also lists every telemetry tag the device reports, including ones not yet mapped to a named field |
| `lock` | Lock the device |
| `unlock` | Unlock the device |
| `findmyiqos` | Vibrate the device until Enter is pressed |
| `exit` / `quit` | Exit the CLI |

### Device Memory

| Command | Description |
|---------|-------------|
| `device list` | List saved device labels and metadata |
| `device save <label>` | Save the current or targeted device under a label |
| `device remove <label>` | Remove a saved device label |

Device memory is stored in `config.toml` under the user config directory. The CLI also remembers the last successfully connected device as the default target. That lets you run commands like `iqos battery` after a device has been remembered once. Use labels when you want a stable name for a specific device:

```bash
iqos --model iluma device save minera
iqos device list
iqos --model minera battery
iqos --model minera
iqos device remove minera
```

### Display & Feedback

| Command | Description | Compatibility |
|---------|-------------|---------------|
| `brightness` | Show current brightness level | All models |
| `brightness <low\|high>` | Set LED brightness | All models |
| `vibration` | Show current vibration settings | All models |
| `vibration <flag> <on\|off> ...` | Set one or more vibration flags | All models |

Vibration flags: `heating`, `starting`, `puffend`, `terminated`, `charge`¹

### Advanced Features

| Command | Description | Compatibility |
|---------|-------------|---------------|
| `flexpuff <enable\|disable\|status>` | Manage FlexPuff | ILUMA i / i Prime |
| `flexbattery` | Show FlexBattery mode and pause state | ILUMA i / i Prime |
| `flexbattery <performance\|eco>` | Set battery mode | ILUMA i / i Prime |
| `flexbattery pause <on\|off>` | Toggle pause mode | ILUMA i / i Prime |
| `smartgesture <enable\|disable>` | Toggle Smart Gesture | ILUMA / ILUMA Prime / ILUMA i / ILUMA i One / ILUMA i Prime |
| `autostart <on\|off\|status>` | Show or toggle automatic heating start | ILUMA i / ILUMA i One / ILUMA i Prime |

### Protocol Debugging

| Command | Description |
|---------|-------------|
| `raw <preset\|hex-bytes>` | Send a raw SCP command and dump the full raw response frame |

Presets (reuse the exact command bytes the decoded commands send): `brightness`,
`firmware-stick`, `firmware-holder`, `autostart`, `flexpuff`, `flexbattery`,
`pausemode`, `vibration`, `vibration-charge-start`, `battery-voltage`,
`telemetry`, `timestamp`, `product-stick`, `product-holder`. Or pass raw hex
bytes directly, e.g. `raw "00 C0 02 23 C3"` / `raw 00C00223C3`.

This exists because several response frames are longer than what gets
decoded — `diagnosis --raw` and `info --raw` already formalize the two
confirmed cases (undecoded telemetry tags, undecoded firmware/battery-voltage
trailing bytes); `raw` is the general tool for finding the next one.

## Examples

### Battery & Diagnosis
```
iqos> battery
Battery Level: 85%

iqos> diagnosis
Diagnosis:
  Total puffs:     1234
  Days used:       42
  Battery voltage: 3.87V
```

### Raw Diagnosis Tags

The reverse-engineered protocol only decodes two of the tag/value blocks the
device actually reports (`total_smoking_count`, `days_used`); `--raw` shows
all of them:

```
iqos> diagnosis --raw
Diagnosis:
  Total puffs:     1939
  Days used:       1414
  Battery voltage: 9.41V
Raw telemetry tags (8 total):
  tag=0x8E value=1939   (total_smoking_count)
  tag=0x20 value=2042   (unrecognized)
  tag=0x17 value=1414   (days_used)
  tag=0x18 value=29     (unrecognized)
  tag=0x8E value=1939   (total_smoking_count)
  tag=0x20 value=2042   (unrecognized)
  tag=0x17 value=1414   (days_used)
  tag=0x18 value=29     (unrecognized)
```

Tags 0x20 and 0x18 are real device telemetry (confirmed against a physical
ILUMA i) with no known meaning yet — the device sends the same frame twice
per read, hence each tag appearing twice. `--format json diagnosis --raw`
includes the same `telemetry_tags` array unconditionally.

### Raw Firmware/Battery-Voltage Frames

Same story for `info`: `FirmwareVersion` only decodes 4 bytes out of each
19-byte firmware response, and battery voltage only 2 bytes out of 19. On a
real ILUMA i, `--raw` shows the rest isn't just padding:

```
iqos> info --raw
...
Stick firmware raw (19 bytes):
  [  0] 00 C0 88 00 10 00 87 05
  [  8] 00 21 A0 C0 01 14 01 01
  [ 16] 06 06 3F
Holder firmware raw (19 bytes):
  [  0] 00 08 88 00 0E 00 7C 06
  [  8] 00 72 B2 C0 01 14 02 01
  [ 16] 0A 06 FE
Battery voltage raw (19 bytes):
  [  0] 00 C0 88 21 E9 C9 24 00
  [  8] 66 06 00 00 00 00 00 00
  [ 16] 00 00 B9
```

Bytes 10-17 of both firmware frames share a structured pattern (`C0 01 14 ..
01 .. 06`) that isn't random padding, and battery voltage's bytes 8-9
(`66 06`, i.e. 1638) are consistently non-zero across reads — neither is
currently decoded into anything. Bytes[0..4] are the header, `battery_voltage`
lives in bytes[5..7]; everything else here is unmapped.

### Generic Protocol Probing

```
iqos> raw telemetry
Command (8 bytes):
  [  0] 00 C9 10 02 01 01 75 D6
Response (40 bytes):
  [  0] 00 08 90 22 01 01 00 00
  [  8] 00 00 93 07 00 8E 01 00
  [ 16] 00 00 FA 07 00 20 02 00
  [ 24] 00 00 86 05 00 17 03 00
  [ 32] 00 00 1D 00 00 18 F4 C8
```

`diagnosis`'s telemetry parser only reads 38 of these 40 bytes (4 blocks
starting at offset 6) — the trailing 2 bytes (`F4 C8`) aren't consumed by
anything yet. Send `raw <preset>` for any decoded command to see what its
parser leaves on the table before deciding whether it's worth decoding.

### JSON Output & Watch Mode
```bash
$ iqos --model iluma-i --format json battery
{
  "battery_percent": 85
}

$ iqos --model iluma-i --format json diagnosis
{
  "battery_voltage": 3.87,
  "days_used": 42,
  "total_puffs": 1234
}

$ iqos --model iluma-i battery --watch --interval 5
Watching battery level every 5s. Press Ctrl+C to stop.
[+    0s] Battery: 85%
[+    5s] Battery: 84%
```

### Brightness
```
iqos> brightness
Brightness: low

iqos> brightness high
Brightness set to high
```

### Vibration
```
iqos> vibration
VibrationSettings { heating_start: true, starting_to_use: true, puff_end: false, manually_terminated: false, charge_start: None }

iqos> vibration heating on puffend off
Vibration settings updated
```

### FlexBattery
```
iqos> flexbattery
FlexBattery: mode=Eco, pause=Some(false)

iqos> flexbattery performance
FlexBattery settings updated

iqos> flexbattery pause on
FlexBattery settings updated
```

### Device Information
```
iqos> info
Device Information:
  Model:           IlumaI
  Model number:    A123
  Serial number:   XXXXXXXXXXXX
  Manufacturer:    Philip Morris International
  Software rev:    X.X.X
  Product number:  XXXXXXXXXXXX
  Stick firmware:  v1.2.3.24
  Holder product:  XXXXXXXXXXXX
  Holder firmware: v1.2.3.24
  Battery voltage: 3.870V
```

## Troubleshooting

### Device Not Found

- Ensure Bluetooth is enabled
- Make sure the IQOS device is powered on and in range
- Restart the device or the CLI and try again

### Connection Failed

- Ensure no other application (e.g. IQOS app) is connected to the device
- Restart the device's Bluetooth and try again
- On macOS, check Bluetooth permissions for the terminal

### Permission Denied (Linux)

```bash
sudo usermod -a -G bluetooth $USER
# Log out and log back in
```

### Command Not Available

- Check the compatibility table — some features are model-specific
- Run `info` to verify the connected device model

## Development

### Project Structure

```
iqosctl/
├── src/
│   ├── main.rs              # Entry point and BLE device discovery
│   ├── cli.rs                # clap argument definitions
│   ├── json_output.rs        # JSON formatters for --format json
│   └── loader/               # CLI interface
│       ├── mod.rs           # Console runner (run_console)
│       ├── parser.rs        # IQOSConsole REPL and command dispatch
│       └── cmds/            # Per-command implementations
├── Cargo.toml
└── README.md
```

### Build Commands

```bash
cargo build            # Debug build
cargo build --release  # Optimized build
cargo test             # Run tests
cargo fmt              # Format code
cargo clippy -- -D warnings  # Lint
cargo check            # Fast type-check without linking
```

### Release Workflow

Releases are created automatically when a semantic version tag is pushed:

```bash
git tag -s v0.1.0 -m "v0.1.0"
git push origin v0.1.0
```

The release workflow validates tags like `v1.2.3` or `v1.2.3-rc.1`, verifies that the pushed tag already exists, builds packages for Linux, Windows, and macOS, then creates a GitHub Release with generated notes. Tags with a prerelease suffix, such as `-rc.1`, are published as prereleases.

Release packages contain the executable as `iqos` on Linux/macOS and `iqos.exe` on Windows, so users can place the extracted binary on `PATH` and run the `iqos` command directly.

Generated release notes are grouped by PR labels using `.github/release.yml`. Use clear PR titles and apply one of these labels before merging:

| Label | Release notes section |
|-------|-----------------------|
| `breaking`, `breaking-change`, `semver-major` | Breaking Changes |
| `feature`, `enhancement`, `semver-minor` | Features |
| `bug`, `fix`, `semver-patch` | Fixes |
| `documentation`, `docs` | Documentation |
| `chore`, `refactor`, `dependencies`, `ci`, `tests` | Maintenance |
| `skip-changelog`, `ignore-for-release` | Excluded from release notes |

GitHub Releases are the project changelog. Keep merged PR titles user-facing because they become the release-note entries.

## Contributing

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/your-feature`)
3. Add tests for new functionality
4. Ensure all tests pass and `cargo clippy` is clean
5. Open a Pull Request

### Reporting Bugs

Open an issue with:
- Clear description of the problem
- Steps to reproduce
- Expected vs actual behavior
- OS, Rust version, and IQOS device model

## License

GNU General Public License v3.0 — see [LICENSE](LICENSE) for details.

## Acknowledgments

Built with [btleplug](https://github.com/deviceplug/btleplug) for Bluetooth Low Energy support.

---

<div align="center">

**Issues:** [github.com/kuroiko0429/iqosctl/issues](https://github.com/kuroiko0429/iqosctl/issues)

</div>
