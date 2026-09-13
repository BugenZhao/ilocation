# iOS 27.0 RC validation

Verified on 2026-09-13 using the local 0.1.1 release build.

## Environment

- Physical iPhone 16 Pro (`iPhone17,1`), USB connected and paired
- iOS 27.0 RC, reported OS version `27.0`, build `24A435`
- Developer Mode enabled; device already prepared for developer services
- macOS 27.0 (`26A428`), Xcode 27.0 (`27A266a`)
- Rust 1.94.0, `idevice` 0.1.65, default `self-hosted` tunnel

The tool built and ran with the existing Rust source unchanged. All location commands used ilocation's own usbmuxd/CoreDeviceProxy tunnel. Apple's `devicectl` inspected the device, launched Compass, and captured screenshots.

## Results

| Check | Result |
| --- | --- |
| Release build with refreshed lockfile | Passed |
| `--version` | `ilocation 0.1.1` |
| `list` | Connected target reported as `usb` |
| `set 34.7570038 138.9875358` | Compass showed `34°45′25″ N 138°59′15″ E`, Kawazu, Shizuoka |
| `gpx examples/two-points.gpx --interval 0.5` | Both points sent; Compass showed final latitude `34°45′26″ N` |
| `gpx examples/two-points.gpx --respect-time --time-scale 2` | Both points sent; final session remained active |
| Ctrl-C after set and both GPX replays | Clear succeeded and each process exited with status 0 |
| Standalone `clear` during an active set session | Exited with status 0; Compass returned to Singapore coordinates |
| Final cleanup | Remaining set session stopped with Ctrl-C and clear succeeded |
| Unit tests | 3 passed |
| Formatting and Clippy (`-D warnings`) | Passed |

Compass displays whole arcseconds, so device screenshots verify coordinates to that display precision. Timestamp replay was checked for successful completion; exact scheduling latency was outside this run's coverage. External `tunneld` mode and fresh-device DDI preparation remain untested in this run.

## Reproduce

Keep the phone unlocked, open Compass with location access enabled, and use the explicit USB UDID from `list`:

```bash
cargo build --release --locked
target/release/ilocation --version
target/release/ilocation list
export ILOCATION_TEST_UDID='<USB UDID>'
target/release/ilocation --udid "$ILOCATION_TEST_UDID" set 34.7570038 138.9875358
# Check Compass, then press Ctrl-C before running the next command.
target/release/ilocation --udid "$ILOCATION_TEST_UDID" gpx examples/two-points.gpx --interval 0.5
# Check the final coordinate, then press Ctrl-C.
target/release/ilocation --udid "$ILOCATION_TEST_UDID" gpx examples/two-points.gpx --respect-time --time-scale 2
# Press Ctrl-C, then verify standalone clear.
target/release/ilocation --udid "$ILOCATION_TEST_UDID" clear
```

For standalone-clear verification against an active simulation, run `set` in one terminal, run `clear` in another, and verify Compass returns to the real location. Stop the remaining set process with Ctrl-C.

Local screenshots from this run are in the ignored `target/e2e-ios27/` directory: `before.png`, `set.png`, `gpx.png`, and `clear.png`. They are local evidence and are excluded from the repository.
