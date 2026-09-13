# Wi-Fi validation on iOS 27.0 RC

Verified on 2026-09-13 with the local 0.2.0 build, macOS 27.0 and an iPhone 16 Pro
running iOS 27.0 RC (24A435). The phone was already paired with this Mac and had
Developer Mode and developer services prepared.

## Coverage

- Before cable removal, usbmuxd exposed USB and network entries for the same UDID.
  Explicit `--transport wifi` selected the network device ID and established a
  CoreDeviceProxy / RSD / DVT session successfully.
- The user then unplugged USB. `list` exposed only a network entry and
  `--transport usb list` failed as expected.
- A fresh `--transport wifi ... gpx examples/two-points.gpx --interval 0.5`
  session sent both points. Compass showed Kawazu, Shizuoka at
  `34°45′26″ N 138°59′15″ E`, matching the final point to display precision.
- Standalone `--transport wifi ... clear` returned success while GPX was held.
  Ctrl-C also cleared the held session and exited successfully.
- With USB still unplugged, default `auto` selected the network entry and
  `set "Kawazu Station Japan" --poi -y` resolved and applied
  `34.7475208,138.9958628`. Ctrl-C cleared that session successfully.
- After final cleanup and reopening Compass, coordinates returned to Singapore
  (`1°22′23″ N 103°56′55″ E`). The place-name label briefly retained Kawazu while
  the coordinates refreshed. All test simulation sessions were stopped.
- Six unit tests passed, including explicit transport filtering, duplicate-UDID
  USB preference, auto network selection, and absence of USB fallback when forced.
  Clippy with `-D warnings` passed.

This run verifies reuse of existing pairing over local Wi-Fi. First-time wireless
pairing, long-duration sessions, and reconnection after network changes remain
outside this run's coverage. Auto chooses a discovered transport before connection;
it does not retry another transport after a selected connection fails.

Local screenshots are retained under ignored `target/e2e-wifi/`.
