# ilocation

Simulate iPhone GPS locations and replay GPX routes from macOS over USB or Wi-Fi.

## Install

Install or update from [crates.io](https://crates.io/crates/ilocation) with Rust 1.94+:

```bash
cargo install ilocation --locked
```

[GitHub Releases](https://github.com/BugenZhao/ilocation/releases) also provides
macOS arm64 binaries. Extract the archive and run `./ilocation --version`.
The binary supports macOS 13+; place search requires macOS 26+ and internet access.

The iPhone must be paired with the Mac, unlocked, and have Developer Mode enabled
with usable Developer Disk Image (DDI) services.

## Usage

```bash
ilocation list
ilocation search "Sonoma Plaza" --poi
ilocation set "Sonoma Plaza" --poi -y
ilocation set 38.292 -122.458
ilocation gpx route.gpx --interval 1
ilocation gpx route.gpx --respect-time --time-scale 2
ilocation clear
```

Use `--udid <UDID>` before the subcommand to select a specific phone.
`set` and `gpx` keep the session alive; GPX replay holds its final point.
Press **Ctrl-C** to clear the simulated location and exit, or run `clear`
with the same device and connection options.

Place search uses Apple Maps through native MapKit, with no API key required:

- `search` prints names, addresses, and coordinates, then exits; it works independently of a phone.
- `set "<place>"` prompts for a numbered result; Enter cancels.
- `--poi` restricts results to points of interest.
- `-y` / `--yes` selects the first result; `--pick <NUMBER>` selects a one-based result. Choose one for non-interactive use. Use coordinates for repeatable scripts, since rankings can change.

GPX input prefers tracks, then routes, then waypoints. `--respect-time` uses
positive timestamp differences, falling back to `--interval` when needed;
`--time-scale 2` doubles playback speed.
See `ilocation --help` or `ilocation <COMMAND> --help` for all options.

## Connections

The default `--mode self-hosted` creates its own CoreDeviceProxy tunnel.
`--transport auto` prefers USB, then Wi-Fi; `usb` and `wifi` select a specific transport.

For Wi-Fi, first pair over USB using Apple's device tooling. Keep both devices
on the same IPv6-capable network and check that a network device appears:

```bash
ilocation --transport wifi list
ilocation --transport wifi --udid <UDID> set "Sonoma Plaza" --poi -y
ilocation --transport wifi --udid <UDID> clear
```

To reuse an existing `pymobiledevice3 tunneld`, use `--mode tunneld`.
That daemon manages transport; its endpoint defaults to `127.0.0.1:49151`
and can be changed with `--host` and `--port`.

If discovery or connection fails, check the phone's unlock/trust state and
Developer Mode. For Wi-Fi, also check network isolation and reconnect USB to
verify pairing. Check or prepare DDI services with:

```bash
xcrun devicectl device info ddiServices --device <UDID>
```

With Command Line Tools CoreDevice, use
`DEVELOPER_DIR=/Library/Developer/CommandLineTools /usr/bin/devicectl`
in place of `xcrun devicectl`.

## Agent skill

Install the bundled [skill](skills/ilocation/SKILL.md) for your agent:

```bash
npx skills add BugenZhao/ilocation --skill ilocation
```

Add `-g -a codex -y` to install globally for Codex.

## Development and releases

```bash
cargo build --release --locked
cargo fmt --check
cargo test --release --locked
cargo clippy --release --locked --all-targets -- -D warnings
```

Push a `vX.Y.Z` tag matching `Cargo.toml` to run the
[release workflow](.github/workflows/release.yml): checks, crates.io Trusted
Publishing, and a GitHub Release with a macOS arm64 archive and SHA-256 checksum.
The workflow can also retry an existing tag through **Run workflow**.

The crates.io Trusted Publisher uses owner `BugenZhao`, repository `ilocation`,
workflow `release.yml`, and an empty environment field.
