# ilocation

`ilocation` is a small Rust CLI for simulating GPS location on a USB-connected iPhone from macOS.

It is designed around modern iOS device services and, by default, builds its own tunnel through `usbmuxd + CoreDeviceProxy`, so it does not require `pymobiledevice3 tunneld` or Xcode tooling at runtime.

This repo also ships an installable agent skill under [`skills/ilocation`](./skills/ilocation), so other users can install the skill with Vercel's `skills` CLI and let their agent install and operate the tool for them.

## Features

- List available device UDIDs
- Simulate a single latitude/longitude pair
- Replay coordinates from a GPX file
- Clear an existing simulated location
- Use either:
  - `self-hosted` mode: direct `usbmuxd + CoreDeviceProxy` tunnel
  - `tunneld` mode: reuse an already-running `pymobiledevice3 tunneld`

## Requirements

- macOS
- A trusted, unlocked iPhone connected over USB
- Rust toolchain for building from source
- Working `usbmuxd` on the host system

For daily use, the default `self-hosted` mode is usually enough. You only need `tunneld` mode if you explicitly want to reuse an external tunnel.

## Build

```bash
cargo build --release
```

The release binary will be available at:

```bash
target/release/ilocation
```

## Install The Agent Skill

Browse the skills in this repo:

```bash
npx skills add BugenZhao/ilocation --list
```

Install the `ilocation` skill globally for Codex:

```bash
npx skills add BugenZhao/ilocation --skill ilocation -g -a codex -y
```

Install the same skill from the direct GitHub path:

```bash
npx skills add https://github.com/BugenZhao/ilocation/tree/main/skills/ilocation -g -a codex -y
```

The skill teaches a fresh agent how to:

- install or update the `ilocation` binary from GitHub
- verify the binary and discover device UDIDs
- run `set`, `gpx`, and `clear`
- prefer the default self-hosted mode unless the user explicitly asks for `tunneld`

The `skills` CLI and source-format examples are documented by Vercel here:

- [skills.sh](https://skills.sh)
- [Vercel Agent Skills docs](https://vercel.com/docs/agent-resources/skills)

## Usage

Show top-level help:

```bash
ilocation --help
```

List available devices:

```bash
ilocation list
```

Example output:

```text
00008140-001969981412801C    usb
```

List devices from an existing `tunneld` instance:

```bash
ilocation --mode tunneld list
```

Example output:

```text
00008140-001969981412801C    tunneld    fd8b:b98f:e833::1:64724    2409:8a28:5244:9f91:183b:466a:88ee:5fc7
```

Set a single coordinate and keep it active until `Ctrl-C`:

```bash
ilocation --udid 00008140-001969981412801C set 34.7570038 138.9875358
```

Replay a GPX file and keep the last point active until `Ctrl-C`:

```bash
ilocation --udid 00008140-001969981412801C gpx examples/two-points.gpx --interval 0.5
```

Replay a GPX file using embedded timestamps:

```bash
ilocation --udid 00008140-001969981412801C gpx route.gpx --respect-time --time-scale 2
```

Clear simulated location:

```bash
ilocation --udid 00008140-001969981412801C clear
```

## GPX behavior

- `ilocation` prefers `track` points first
- If there are no tracks, it falls back to `route` points
- If there are no routes, it falls back to top-level `waypoint` entries
- Without `--respect-time`, points are replayed using `--interval`
- With `--respect-time`, adjacent GPX timestamps are used when both points have timestamps
- After replay finishes, the last point remains active until you press `Ctrl-C`

## Tunnel modes

### `self-hosted`

Default mode.

`ilocation` discovers the device through `usbmuxd`, opens `CoreDeviceProxy`, creates a software tunnel, performs the RSD handshake, and talks to the `LocationSimulation` service directly.

```bash
ilocation list
ilocation --udid <UDID> set <LAT> <LON>
```

### `tunneld`

Optional compatibility mode when you already have `pymobiledevice3 tunneld` running.

```bash
ilocation --mode tunneld list
ilocation --mode tunneld --udid <UDID> set <LAT> <LON>
```

You can also point it at a non-default host or port:

```bash
ilocation --mode tunneld --host 127.0.0.1 --port 49151 list
```

## Notes

- Location simulation remains active only while the session is kept alive
- Pressing `Ctrl-C` clears the simulated location before exit
- If multiple devices are connected and `--udid` is omitted, the first matching device is used
- `list` is the fastest way to discover a usable UDID before running `set`, `gpx`, or `clear`
- The repo-level skill can be installed with `npx skills add BugenZhao/ilocation --skill ilocation`
