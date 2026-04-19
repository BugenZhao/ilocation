---
name: ilocation
description: Install, update, and use the ilocation CLI from https://github.com/BugenZhao/ilocation to list connected iPhone UDIDs, set or clear simulated GPS coordinates, or replay GPX tracks on macOS. Use when the user wants the ilocation tool itself, asks to install it on a machine, or wants an agent to run ilocation commands against a trusted USB-connected iPhone.
---

# ilocation Skill

## Overview

Use this skill when the user wants the exact `ilocation` CLI, not a generic `pymobiledevice3` or `go-ios` workflow.

`ilocation` is a Rust CLI hosted at:

- `https://github.com/BugenZhao/ilocation`

Its default mode is self-hosted:

- discover devices through `usbmuxd`
- open `CoreDeviceProxy`
- create a software tunnel
- connect to the DVT `LocationSimulation` service

Prefer that path unless the user explicitly asks to reuse `pymobiledevice3 tunneld`.

## Preconditions

Before installation or execution, verify:

- the machine is macOS
- the iPhone is connected over USB
- the iPhone is unlocked and trusted by the Mac
- `cargo` is available, or the user is willing to install Rust first

If `cargo` is missing, pause and tell the user that `ilocation` is distributed as a Rust CLI and needs a Rust toolchain. Ask before installing Rust because that is a machine-wide change.

## Install Or Update ilocation

Prefer the bundled installer script:

```bash
bash skills/ilocation/scripts/install-ilocation.sh
```

That script installs the latest `ilocation` binary from GitHub with:

```bash
cargo install --git https://github.com/BugenZhao/ilocation --locked --force ilocation
```

The binary is normally placed at:

```bash
${CARGO_HOME:-$HOME/.cargo}/bin/ilocation
```

If `ilocation` is not on `PATH`, invoke it by absolute path instead of trying to edit shell startup files unless the user asks for that.

## Verify Installation

After installation, run:

```bash
ilocation --help
ilocation list
```

If `ilocation --help` works but `ilocation list` shows no devices, do not treat that as an install failure. It usually means the phone is not connected, not trusted, or still locked.

## Common Workflows

### Discover a device UDID

Use:

```bash
ilocation list
```

If the user explicitly wants the external tunnel mode:

```bash
ilocation --mode tunneld list
```

### Set a single coordinate

Use:

```bash
ilocation --udid <UDID> set <LATITUDE> <LONGITUDE>
```

Keep the process alive while the simulated location should remain active. When the user wants to stop spoofing, send `Ctrl-C`; `ilocation` clears the simulated location on exit.

### Replay a GPX route

Use:

```bash
ilocation --udid <UDID> gpx <FILE.gpx> --interval 1
```

If the user wants to respect GPX timestamps:

```bash
ilocation --udid <UDID> gpx <FILE.gpx> --respect-time --time-scale 2
```

`ilocation` prefers GPX tracks first, then routes, then top-level waypoints.

### Clear the simulated location

Use:

```bash
ilocation --udid <UDID> clear
```

## Execution Order

When acting for a fresh machine, follow this sequence:

1. Confirm macOS, USB connection, trust state, and `cargo`.
2. Install or update `ilocation` with `scripts/install-ilocation.sh`.
3. Verify the binary with `ilocation --help`.
4. Discover the device with `ilocation list`.
5. Run `set`, `gpx`, or `clear` depending on the user’s request.
6. If you started a long-lived `set` or `gpx` session, keep it attached until the user wants to stop or until the task explicitly says to clean up.

## Fallbacks

If the user wants local source code instead of a binary install:

```bash
git clone https://github.com/BugenZhao/ilocation
cd ilocation
cargo install --path . --locked
```

Only use `--mode tunneld` when the user asks for it or already has an external `tunneld` workflow they want to preserve.

## Troubleshooting

- `cargo: command not found`
  Rust is not installed. Explain the blocker and ask before installing Rust.
- `ilocation list` shows no devices
  Recheck USB, trust, and unlock state on the phone.
- `set` or `gpx` exits immediately with no active spoof
  Re-run against the explicit `--udid` from `ilocation list`.
- The user wants the real location back
  Run `ilocation --udid <UDID> clear`, or if a long-running session is still attached, stop it with `Ctrl-C`.

## Communication Pattern

Tell the user:

- whether you installed or reused `ilocation`
- the exact binary path if it is not on `PATH`
- the exact UDID you selected
- the exact command you are running
- whether the location spoof is being held by a long-running foreground session
