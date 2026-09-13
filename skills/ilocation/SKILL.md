---
name: ilocation
description: Install or use ilocation on macOS to search places, simulate a paired iPhone's GPS location over USB or Wi-Fi, replay GPX routes, and clear simulated location.
---

# ilocation

## Setup

Use an existing `ilocation` installation, or install/update from crates.io:

```bash
cargo install ilocation --locked
ilocation --version
```

Building requires Rust 1.94+. The bundled [installer](scripts/install-ilocation.sh)
runs the same command; resolve its path relative to this installed skill directory.
The binary lives in `${CARGO_HOME:-$HOME/.cargo}/bin`; use its absolute path when
needed. Prebuilt macOS arm64 binaries are also available from
[GitHub Releases](https://github.com/BugenZhao/ilocation/releases).

For device operations, ensure the iPhone is paired, unlocked, and has Developer
Mode and usable DDI services. Run `ilocation list` and select the intended UDID.
Prefer the default `self-hosted` mode, which creates its own tunnel.

## Command contract

Put connection options before the subcommand: `ilocation [--udid <UDID>]
[--mode self-hosted|tunneld] [--transport auto|usb|wifi] <COMMAND>`.
Use `ilocation <COMMAND> --help` to check the installed version's options.
Select an explicit UDID when operating among multiple phones.

```bash
ilocation search "Sonoma Plaza" --poi
ilocation --udid <UDID> set "Sonoma Plaza" --poi -y
ilocation --udid <UDID> set <LATITUDE> <LONGITUDE>
ilocation --udid <UDID> gpx <FILE.gpx> --interval 1
ilocation --udid <UDID> gpx <FILE.gpx> --respect-time --time-scale 2
ilocation --udid <UDID> clear
```

Place search requires macOS 26+ and internet access. `search` prints Apple Maps
candidates and exits independently of a device. Review names, addresses, and
coordinates when resolving an ambiguous request. `--poi` filters to points of
interest. For non-interactive `set`, use `-y` to choose the first result or
`--pick <NUMBER>` for a one-based result. Interactive `set` prompts for a choice;
Enter cancels. Use coordinates for repeatable automation because ranking can change.

`search` writes candidates to stdout and progress to stderr. `set` writes its
candidate list and selected coordinates to stderr. The output is human-readable.
A search with zero matches prints `No places found.` and succeeds; setting a place
with zero matches fails. Search has a 20-second timeout and completes before the
device connection opens.

`--yes` and `--pick` are mutually exclusive and apply to place queries, together
with `--poi`. Numeric `set` takes both latitude and longitude in that order,
including negative values; coordinates must be finite and within ±90 / ±180.
Interactive selection also prompts for a single match. For a reviewed search
result, pass its coordinates to `set` to preserve that exact choice across calls;
`set "<place>" --pick N` performs a fresh search.

## GPX and session lifecycle

GPX prefers tracks, then routes, then waypoints. `--respect-time` follows positive
timestamp differences divided by `--time-scale`, falling back to `--interval`
for missing, equal, or decreasing timestamps. The fallback interval stays fixed
regardless of time scale. Supply positive interval and time-scale values.

Keep the `set` or `gpx` process alive for the requested duration; GPX holds its
final point. On cleanup, send Ctrl-C to clear and exit, or run `clear` with the
same UDID, mode, and transport. Report the selected device, target coordinates,
and whether the session remains active. For terminal tools, use a persistent session that supports SIGINT/Ctrl-C, retain
its session ID, and wait for the active-location log before reporting success.
The expected long-running state continues through the requested hold duration.

Ctrl-C invokes the application's explicit clear path. Abrupt termination or a
connection error can bypass that path; reconnect and issue `clear` when cleanup
is requested, then check for `location simulation cleared`. Stop an active GPX
session before standalone cleanup so subsequent points cannot reapply a location.

CLI logs establish service-level success. To verify the phone's displayed
location, inspect coordinates in a device app such as Compass; city labels can
lag. For an end-to-end test, choose a target distinct from the phone's physical
location and verify coordinates again after clearing.

## Connection model

`self-hosted` is the default for both USB and Wi-Fi:

```text
usbmuxd device → CoreDeviceProxy → in-process software tunnel
              → RSD handshake → DVT → LocationSimulation
```

The process owns the tunnel for the lifetime of its location session. Device
listing proves discovery; opening the developer services is a separate step.
`--transport auto` sorts USB entries first, then UDID and usbmuxd device ID.
This USB preference also applies with an explicit UDID. `usb` and `wifi` strictly
filter eligible devices. A failed connection returns an error; changing routes
requires a new invocation with the desired transport.

For Wi-Fi, pair over USB using Apple's tools and keep both devices on the same
IPv6-capable network. Discovery uses macOS usbmuxd's paired network entries:

```bash
ilocation --transport wifi list
ilocation --transport wifi --udid <UDID> set "Sonoma Plaza" --poi -y
ilocation --transport wifi --udid <UDID> clear
```

Check the startup log for `network:<IP>` and the selected usbmuxd device ID.
To validate pure wireless operation, discover the network entry, unplug USB,
and open a fresh explicit-Wi-Fi session. Initial trust/pairing is handled by
Apple's tooling; ilocation reuses those records.

`tunneld` queries an existing external daemon, then connects through its advertised
RSD endpoint. Use this mode when reusing that infrastructure:

```bash
ilocation --mode tunneld --host 127.0.0.1 --port 49151 list
ilocation --mode tunneld --udid <UDID> set 38.292 -122.458
```

The endpoint shown is the default. The daemon manages the transport;
`--transport` must retain its default `auto` value in this mode. Preserve mode,
UDID, and any custom host/port during cleanup.

## Diagnose by failure stage

- **Search:** check macOS 26+, network access, and query specificity. Resolve ambiguous matches using returned addresses and coordinates, optionally with `--poi`.
- **usbmuxd connection or discovery:** check USB, unlock/trust state, and any `USBMUXD_SOCKET_ADDRESS` override. For Wi-Fi, check network isolation and reattach USB to verify pairing.
- **CoreDeviceProxy:** its connection attempt times out after 20 seconds. Check device reachability and unlock state, then developer-service readiness.
- **RSD / DVT / LocationSimulation:** verify Developer Mode and DDI services with the command below, then retry the same explicit device and transport.
- **tunneld query or endpoint:** verify the external daemon's host/port, device listing, and advertised tunnel reachability.

```bash
xcrun devicectl device info ddiServices --device <UDID>
```

With Command Line Tools CoreDevice, replace `xcrun devicectl` with
`DEVELOPER_DIR=/Library/Developer/CommandLineTools /usr/bin/devicectl`.
Retry after a relevant prerequisite or connection state changes. If the same
stage still fails, report that stage and the concrete error so the next action
can address it.
