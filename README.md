# ilocation

`ilocation` is a small Rust CLI for simulating GPS location on a paired iPhone over USB or Wi-Fi from macOS.

It builds its own tunnel through `usbmuxd + CoreDeviceProxy` by default. Location commands use the device's DVT developer services; Developer Mode and a usable Developer Disk Image (DDI) are prerequisites. Apple's device tooling can prepare those services when needed.

Version **0.1.1** was verified on a USB-connected iPhone 16 Pro running **iOS 27.0 RC (24A435)** on 2026-09-13. See [the validation record](docs/validation-ios-27.md) for coverage and reproduction steps.

This repo also ships an installable agent skill under [`skills/ilocation`](./skills/ilocation), so other users can install the skill with Vercel's `skills` CLI and let their agent install and operate the tool for them.

## Features

- List available device UDIDs
- Simulate a single latitude/longitude pair
- Search Apple Maps by place name or address, with optional POI filtering
- Replay coordinates from a GPX file
- Clear an existing simulated location
- Use either:
  - `self-hosted` mode: direct `usbmuxd + CoreDeviceProxy` tunnel
  - `tunneld` mode: reuse an already-running `pymobiledevice3 tunneld`

## Requirements

- macOS
- A trusted, unlocked iPhone connected over USB or discoverable over Wi-Fi after pairing
- Developer Mode enabled on the iPhone and usable DDI services
- Rust 1.94 or newer for building from source
- Working `usbmuxd` on the host system

For daily use, the default `self-hosted` mode is usually enough. You only need `tunneld` mode if you explicitly want to reuse an external tunnel.

## Install

Install the latest release from [crates.io](https://crates.io/crates/ilocation):

```bash
cargo install ilocation --locked
ilocation --version
```

Run the same install command to upgrade when a newer release is available.

## Build From Source

```bash
cargo build --release --locked
```

The release binary will be available at:

```bash
target/release/ilocation
```

Install the checked-out version with `cargo install --path . --locked --force`.
Check the installed version with `ilocation --version`.

## Install The Agent Skill

The skill follows the [Agent Skills specification](https://agentskills.io/specification):
`skills/ilocation/SKILL.md` provides the name, description, and instructions;
`scripts/` contains the bundled installer. The `skills` CLI discovers this directory
directly from GitHub. Script paths are resolved relative to the installed skill.

Install into the current project and select your agent interactively:

```bash
npx skills add BugenZhao/ilocation --skill ilocation
```

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

- install or update the `ilocation` binary from crates.io
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

## Place Search

Starting with 0.2.0, `set` accepts a quoted place name or address on macOS 26+.
Search uses native Apple MapKit and requires an internet connection. The CLI uses
system frameworks and needs no API key or separate app bundle.

```bash
ilocation search "pasir ris 8, singapore"
ilocation search "Pasir Ris 8" --poi
ilocation set "pasir ris 8, singapore"
ilocation set "Pasir Ris 8" --poi
ilocation set "Pasir Ris 8" --poi -y
ilocation set "Pasir Ris 8" --poi --pick 1
```

`search` prints candidates to stdout and exits, with no device connection or selection
prompt. It also works when the phone is unplugged; progress messages go to stderr.

The default search can return addresses, roads, and places. `--poi` restricts results
to points of interest such as buildings, shops, and stations. Results include names,
addresses, and coordinates; choose a numbered result or press Enter to cancel.
Even a single result is shown for selection, since similar queries can match different
places. For example, the local MapKit probe matched `pasir ris 8, singapore` to
8 Pasir Ris Way, while `Pasir Ris 8 --poi` matched the condominium.

For scripts, `--yes` (`-y`) automatically uses the first result; `--pick <NUMBER>` selects the one-based result directly. Result ordering
can change; use explicit coordinates for repeatable automation. Non-interactive
input requires `--yes` or `--pick`. Search and selection complete before opening a device
session, and searches time out after 20 seconds. Numeric `set <LAT> <LON>` retains
its existing behavior; `--poi`, `--yes`, and `--pick` apply to place queries. `--yes` and `--pick` are mutually exclusive.

## GPX behavior

- `ilocation` prefers `track` points first
- If there are no tracks, it falls back to `route` points
- If there are no routes, it falls back to top-level `waypoint` entries
- Without `--respect-time`, points are replayed using `--interval`
- With `--respect-time`, adjacent GPX timestamps are used when both points have timestamps
- After replay finishes, the last point remains active until you press `Ctrl-C`

## Device Transport

Self-hosted mode supports `--transport auto|usb|wifi`. [Pure Wi-Fi validation on iOS 27.0 RC](docs/validation-wifi-ios-27.md) covers cable removal, fresh sessions, GPX, and cleanup:

```bash
ilocation --transport wifi list
ilocation --transport wifi --udid <UDID> set "Pasir Ris 8" --poi -y
ilocation --transport wifi --udid <UDID> gpx examples/two-points.gpx
ilocation --transport wifi --udid <UDID> clear
```

`auto` prefers USB, including when `--udid` is supplied, then selects an available
network device. `usb` and `wifi` strictly filter the device list and connection;
`wifi` selects a network entry exposed by macOS usbmuxd. Startup logs show the
selected transport and usbmuxd device ID. Keep the session running to maintain
simulation, and use Ctrl-C or `clear` to stop it.

For initial setup, connect by USB, trust the Mac, enable Developer Mode, and prepare
developer services with Apple's device tooling. Keep both devices on the same
network with IPv6 support. Verify a `network:...` entry appears in
`ilocation --transport wifi list` before disconnecting USB. This workflow reuses
existing pairing records; initial pairing is managed by Apple's tools.

If Wi-Fi discovery or connection fails, unlock the phone, check network isolation
and reachability, and reconnect USB to check pairing and developer services.
CoreDeviceProxy setup times out after 20 seconds. The `tunneld` backend manages
its own transport and accepts the default `auto` value only.

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
- If `--udid` is omitted, self-hosted mode selects the first USB device sorted by UDID, then falls back to network devices; tunneld mode selects the first sorted UDID
- `list` is the fastest way to discover a usable UDID before running `set`, `gpx`, or `clear`
- The repo-level skill can be installed with `npx skills add BugenZhao/ilocation --skill ilocation`

## Developer services troubleshooting

If opening `CoreDeviceProxy` or `com.apple.instruments.dtservicehub` fails, unlock and trust the phone, enable Developer Mode, and check DDI readiness:

```bash
xcrun devicectl device info ddiServices --device <UDID>
```

On a Command Line Tools installation that provides CoreDevice, use:

```bash
DEVELOPER_DIR=/Library/Developer/CommandLineTools /usr/bin/devicectl device info ddiServices --device <UDID>
```

Once developer services are ready, retry the same `ilocation` command. The default self-hosted mode manages its tunnel within the process.

## Development checks

```bash
cargo fmt --check
cargo test --release --locked
cargo clippy --release --locked --all-targets -- -D warnings
```

## Releases

Pushing a stable version tag such as `v0.2.0` runs
[the release workflow](.github/workflows/release.yml). The tag must match the
version in `Cargo.toml`. The workflow runs tests and Clippy, builds the
`aarch64-apple-darwin` executable, publishes to crates.io using Trusted Publishing,
and creates a GitHub Release containing a `.tar.gz` archive and SHA-256 checksum.
The archive includes the binary, license, README, and example GPX files.

Maintainers configure the crate's GitHub Trusted Publisher with owner `BugenZhao`,
repository `ilocation`, workflow `release.yml`, and an empty environment field.
GitHub Actions obtains a short-lived crates.io token through OIDC. The workflow
also accepts an existing tag through **Run workflow** to retry a release; an
already-published crate version is skipped and GitHub assets are replaced.

```bash
git tag v0.2.0
git push origin v0.2.0
```

Prebuilt macOS arm64 binaries are available from
[GitHub Releases](https://github.com/BugenZhao/ilocation/releases). Extract the
archive and run `./ilocation --version`. The binary targets macOS 13+, with
place search requiring macOS 26+.
