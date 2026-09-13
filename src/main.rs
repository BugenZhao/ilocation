mod geocoding;

use std::{
    collections::HashMap,
    fs::File,
    io::BufReader,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    process,
    str::FromStr,
    time::Duration,
};

use anyhow::{Context, anyhow, bail};
use clap::{Parser, Subcommand, ValueEnum};
use gpx::{Gpx, Waypoint, read as read_gpx};
use idevice::{
    IdeviceService, ReadWrite,
    provider::RsdProvider,
    services::{
        core_device_proxy::CoreDeviceProxy,
        dvt::{location_simulation::LocationSimulationClient, remote_server::RemoteServerClient},
        rsd::RsdHandshake,
    },
    tcp::handle::AdapterHandle,
    tunneld::{DEFAULT_PORT, TunneldDevice, get_tunneld_devices},
    usbmuxd::{Connection, UsbmuxdAddr, UsbmuxdDevice},
};
use time::OffsetDateTime;

#[derive(Debug, Subcommand)]
enum Command {
    /// List available device UDIDs.
    List,
    /// Search Apple Maps and print candidates without connecting to a device.
    Search {
        /// Quoted place name or address.
        query: String,
        /// Search only points of interest.
        #[arg(long)]
        poi: bool,
    },
    /// Simulate coordinates or search for a place and keep it active until Ctrl-C.
    Set {
        /// Latitude, or a quoted place name/address.
        #[arg(allow_negative_numbers = true)]
        latitude_or_place: String,
        /// Longitude in decimal degrees when supplying a latitude.
        #[arg(allow_negative_numbers = true)]
        longitude: Option<f64>,
        /// Search only points of interest (buildings, shops, stations, etc.).
        #[arg(long, conflicts_with = "longitude")]
        poi: bool,
        /// Select a one-based search result without prompting.
        #[arg(long, conflicts_with = "longitude", value_parser = clap::value_parser!(u32).range(1..))]
        pick: Option<u32>,
        /// Automatically use the first place search result.
        #[arg(short = 'y', long, conflicts_with_all = ["longitude", "pick"])]
        yes: bool,
    },
    /// Replay points from a GPX file and keep the final point active until Ctrl-C.
    Gpx {
        /// GPX file containing track, route, or waypoint data.
        file: PathBuf,
        /// Fallback interval between points when GPX timestamps are ignored or missing.
        #[arg(long, default_value_t = 1.0)]
        interval: f64,
        /// Respect GPX timestamps when both adjacent points have a time.
        #[arg(long)]
        respect_time: bool,
        /// Speed multiplier applied when --respect-time is enabled.
        #[arg(long, default_value_t = 1.0)]
        time_scale: f64,
    },
    /// Clear an existing simulated location and exit.
    Clear,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
enum TunnelMode {
    /// Build a tunnel directly via usbmuxd and CoreDeviceProxy.
    SelfHosted,
    /// Reuse an already-running pymobiledevice3 tunneld instance.
    Tunneld,
}

#[derive(Debug, Parser)]
#[command(
    name = "ilocation",
    version,
    about = "Simulate GPS location on a connected iPhone",
    long_about = "Simulate GPS location on a connected iPhone via idevice.\n\nBy default this tool builds its own software tunnel through usbmuxd + CoreDeviceProxy, so it does not require pymobiledevice3 tunneld or Xcode tooling at runtime."
)]
struct Cli {
    /// Device UDID. Defaults to the first USB-connected device.
    #[arg(long)]
    udid: Option<String>,
    /// Tunnel backend to use.
    #[arg(long, value_enum, default_value_t = TunnelMode::SelfHosted)]
    mode: TunnelMode,
    /// Host for tunneld when --mode=tunneld.
    #[arg(long, default_value = "127.0.0.1")]
    host: IpAddr,
    /// Port for tunneld when --mode=tunneld.
    #[arg(long, default_value_t = DEFAULT_PORT)]
    port: u16,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug)]
enum TunnelKeeper {
    SelfHosted(AdapterHandle),
    None,
}

impl TunnelKeeper {
    fn touch(&self) {
        if let Self::SelfHosted(handle) = self {
            let _ = handle;
        }
    }
}

#[derive(Debug)]
struct ReplayPoint {
    latitude: f64,
    longitude: f64,
    time: Option<OffsetDateTime>,
}

#[derive(Debug)]
struct GpxReplay {
    points: Vec<ReplayPoint>,
    source: &'static str,
}

fn main() {
    if let Err(err) = main_result() {
        eprintln!("error: {err:#}");
        process::exit(1);
    }
}

fn main_result() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Command::Search { query, poi } = &cli.command {
        let places = geocoding::search(query, *poi)?;
        geocoding::print_places(&places, &mut std::io::stdout().lock())?;
        return Ok(());
    }
    // MapKit and its run loop run on the main thread, before opening any device session.
    let coordinate = match &cli.command {
        Command::Set {
            latitude_or_place,
            longitude,
            poi,
            pick,
            yes,
        } => Some(resolve_set(
            latitude_or_place,
            *longitude,
            *poi,
            if *yes { Some(1) } else { *pick },
        )?),
        _ => None,
    };
    tokio::runtime::Runtime::new()?.block_on(run(cli, coordinate))
}

fn resolve_set(
    input: &str,
    longitude: Option<f64>,
    poi: bool,
    pick: Option<u32>,
) -> anyhow::Result<(f64, f64)> {
    if let Some(longitude) = longitude {
        let latitude = input
            .parse::<f64>()
            .context("latitude must be a number when longitude is supplied")?;
        validate_coordinate("latitude", latitude)?;
        validate_coordinate("longitude", longitude)?;
        return Ok((latitude, longitude));
    }
    if input.parse::<f64>().is_ok() {
        bail!("a numeric latitude requires a longitude");
    }
    if input.trim().is_empty() {
        bail!("place query must contain text");
    }
    geocoding::select(geocoding::search(input, poi)?, pick)
}

async fn run(cli: Cli, coordinate: Option<(f64, f64)>) -> anyhow::Result<()> {
    if matches!(cli.command, Command::List) {
        list_available_devices(cli.mode, cli.tunneld_socket()).await?;
        return Ok(());
    }

    let (_udid, mut dvt, tunnel_keeper) = match cli.mode {
        TunnelMode::SelfHosted => connect_dvt_via_usbmuxd(cli.udid.as_deref())
            .await
            .context("failed to build a software tunnel via usbmuxd/CoreDeviceProxy")?,
        TunnelMode::Tunneld => connect_dvt_via_tunneld(cli.tunneld_socket(), cli.udid.as_deref())
            .await
            .with_context(|| {
                format!("failed to connect via tunneld at {}", cli.tunneld_socket())
            })?,
    };
    // Keep the software tunnel handle alive for the lifetime of the DVT session.
    tunnel_keeper.touch();

    let mut location = LocationSimulationClient::new(&mut dvt)
        .await
        .context("failed to open LocationSimulation service")?;

    match cli.command {
        Command::List | Command::Search { .. } => {
            unreachable!("read-only commands are handled before opening a location session")
        }
        Command::Set { .. } => {
            let (latitude, longitude) = coordinate.context("missing resolved coordinates")?;
            run_single_point(&mut location, latitude, longitude).await?;
        }
        Command::Gpx {
            file,
            interval,
            respect_time,
            time_scale,
        } => {
            ensure_positive("interval", interval)?;
            ensure_positive("time-scale", time_scale)?;
            run_gpx_replay(&mut location, &file, interval, respect_time, time_scale).await?;
        }
        Command::Clear => {
            location
                .clear()
                .await
                .context("failed to clear location simulation")?;
            eprintln!("location simulation cleared");
        }
    }

    Ok(())
}

impl Cli {
    fn tunneld_socket(&self) -> SocketAddr {
        SocketAddr::new(self.host, self.port)
    }
}

async fn connect_dvt_via_usbmuxd(
    requested_udid: Option<&str>,
) -> anyhow::Result<(String, RemoteServerClient<Box<dyn ReadWrite>>, TunnelKeeper)> {
    let addr = UsbmuxdAddr::from_env_var().context("invalid USBMUXD_SOCKET_ADDRESS")?;
    let mut mux = addr
        .connect(1)
        .await
        .context("failed to connect to usbmuxd")?;
    let devices = mux
        .get_devices()
        .await
        .context("failed to enumerate devices from usbmuxd")?;

    let device = pick_usbmuxd_device(devices, requested_udid)?;
    let udid = device.udid.clone();
    let provider = device.to_provider(addr, "ilocation");

    eprintln!("using device {udid} via usbmuxd/CoreDeviceProxy");

    let proxy = CoreDeviceProxy::connect(&provider)
        .await
        .context("failed to connect to CoreDeviceProxy")?;
    let rsd_port = proxy.tunnel_info().server_rsd_port;
    let adapter = proxy
        .create_software_tunnel()
        .context("failed to create software tunnel adapter")?;
    let mut handle = adapter.to_async_handle();

    let rsd_stream = handle
        .connect_to_service_port(rsd_port)
        .await
        .with_context(|| format!("failed to connect to shared RSD port {rsd_port}"))?;
    let mut handshake = RsdHandshake::new(rsd_stream)
        .await
        .context("failed during RSD handshake over self-hosted tunnel")?;

    let dvt = handshake
        .connect::<RemoteServerClient<Box<dyn ReadWrite>>>(&mut handle)
        .await
        .context("failed to connect to com.apple.instruments.dtservicehub")?;

    Ok((udid, dvt, TunnelKeeper::SelfHosted(handle)))
}

async fn connect_dvt_via_tunneld(
    host: SocketAddr,
    requested_udid: Option<&str>,
) -> anyhow::Result<(String, RemoteServerClient<Box<dyn ReadWrite>>, TunnelKeeper)> {
    let devices = get_tunneld_devices(host)
        .await
        .with_context(|| format!("failed to query tunneld at {host}"))?;
    let (udid, device) = pick_tunneld_device(devices, requested_udid)?;

    eprintln!(
        "using device {udid} via {}:{} ({})",
        device.tunnel_address, device.tunnel_port, device.interface
    );

    let rsd_socket = tokio::net::TcpStream::connect((
        IpAddr::from_str(&device.tunnel_address)
            .with_context(|| format!("invalid tunnel IP {}", device.tunnel_address))?,
        device.tunnel_port,
    ))
    .await
    .with_context(|| {
        format!(
            "failed to connect to tunnel endpoint {}:{}",
            device.tunnel_address, device.tunnel_port
        )
    })?;
    let mut handshake = RsdHandshake::new(rsd_socket)
        .await
        .context("failed during RSD handshake")?;

    let mut provider = IpAddr::from_str(&device.tunnel_address)
        .with_context(|| format!("invalid tunnel IP {}", device.tunnel_address))?;
    let dvt = handshake
        .connect::<RemoteServerClient<Box<dyn ReadWrite>>>(&mut provider)
        .await
        .context("failed to connect to com.apple.instruments.dtservicehub")?;

    Ok((udid, dvt, TunnelKeeper::None))
}

async fn list_available_devices(
    mode: TunnelMode,
    tunneld_socket: SocketAddr,
) -> anyhow::Result<()> {
    match mode {
        TunnelMode::SelfHosted => {
            let addr = UsbmuxdAddr::from_env_var().context("invalid USBMUXD_SOCKET_ADDRESS")?;
            let mut mux = addr
                .connect(1)
                .await
                .context("failed to connect to usbmuxd")?;
            let mut devices = mux
                .get_devices()
                .await
                .context("failed to enumerate devices from usbmuxd")?;

            if devices.is_empty() {
                bail!("no devices exposed by usbmuxd");
            }

            devices.sort_by(|a, b| a.udid.cmp(&b.udid));
            for device in devices {
                println!("{}\t{}", device.udid, usb_connection_label(&device));
            }
        }
        TunnelMode::Tunneld => {
            let devices = get_tunneld_devices(tunneld_socket)
                .await
                .with_context(|| format!("failed to query tunneld at {tunneld_socket}"))?;
            if devices.is_empty() {
                bail!("no devices exposed by tunneld");
            }

            let mut entries: Vec<_> = devices.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (udid, device) in entries {
                println!(
                    "{udid}\ttunneld\t{}:{}\t{}",
                    device.tunnel_address, device.tunnel_port, device.interface
                );
            }
        }
    }

    Ok(())
}

fn usb_connection_label(device: &UsbmuxdDevice) -> String {
    match &device.connection_type {
        Connection::Usb => "usb".to_string(),
        Connection::Network(ip) => format!("network:{ip}"),
        Connection::Unknown(kind) => format!("unknown:{kind}"),
    }
}

async fn run_single_point(
    location: &mut LocationSimulationClient<'_, Box<dyn ReadWrite>>,
    latitude: f64,
    longitude: f64,
) -> anyhow::Result<()> {
    validate_coordinate("latitude", latitude)?;
    validate_coordinate("longitude", longitude)?;

    location
        .set(latitude, longitude)
        .await
        .with_context(|| format!("failed to set location to {latitude},{longitude}"))?;
    eprintln!(
        "location simulation active at {latitude},{longitude}; press Ctrl-C to clear and exit"
    );

    let session_result = wait_for_ctrl_c().await;
    clear_location(location).await?;
    session_result
}

async fn run_gpx_replay(
    location: &mut LocationSimulationClient<'_, Box<dyn ReadWrite>>,
    path: &Path,
    interval_seconds: f64,
    respect_time: bool,
    time_scale: f64,
) -> anyhow::Result<()> {
    let replay = load_gpx_replay(path)?;
    eprintln!(
        "loaded {} {} point(s) from {}",
        replay.points.len(),
        replay.source,
        path.display()
    );

    for (index, point) in replay.points.iter().enumerate() {
        location
            .set(point.latitude, point.longitude)
            .await
            .with_context(|| {
                format!(
                    "failed to set GPX point {}/{} to {},{}",
                    index + 1,
                    replay.points.len(),
                    point.latitude,
                    point.longitude
                )
            })?;
        eprintln!(
            "point {}/{} -> {},{}",
            index + 1,
            replay.points.len(),
            point.latitude,
            point.longitude
        );

        if index + 1 == replay.points.len() {
            break;
        }

        let next_point = &replay.points[index + 1];
        let delay = replay_delay(
            point,
            next_point,
            Duration::from_secs_f64(interval_seconds),
            respect_time,
            time_scale,
        );
        if wait_for_ctrl_c_or_timeout(delay).await? {
            clear_location(location).await?;
            return Ok(());
        }
    }

    eprintln!("gpx replay finished; final point is still active. Press Ctrl-C to clear and exit");
    let session_result = wait_for_ctrl_c().await;
    clear_location(location).await?;
    session_result
}

fn load_gpx_replay(path: &Path) -> anyhow::Result<GpxReplay> {
    let file =
        File::open(path).with_context(|| format!("failed to open GPX file {}", path.display()))?;
    let reader = BufReader::new(file);
    let gpx =
        read_gpx(reader).with_context(|| format!("failed to parse GPX file {}", path.display()))?;
    extract_replay_points(&gpx)
}

fn extract_replay_points(gpx: &Gpx) -> anyhow::Result<GpxReplay> {
    let track_points: Vec<_> = gpx
        .tracks
        .iter()
        .flat_map(|track| track.segments.iter())
        .flat_map(|segment| segment.points.iter())
        .map(replay_point_from_waypoint)
        .collect::<anyhow::Result<_>>()?;
    if !track_points.is_empty() {
        return Ok(GpxReplay {
            points: track_points,
            source: "track",
        });
    }

    let route_points: Vec<_> = gpx
        .routes
        .iter()
        .flat_map(|route| route.points.iter())
        .map(replay_point_from_waypoint)
        .collect::<anyhow::Result<_>>()?;
    if !route_points.is_empty() {
        return Ok(GpxReplay {
            points: route_points,
            source: "route",
        });
    }

    let waypoint_points: Vec<_> = gpx
        .waypoints
        .iter()
        .map(replay_point_from_waypoint)
        .collect::<anyhow::Result<_>>()?;
    if !waypoint_points.is_empty() {
        return Ok(GpxReplay {
            points: waypoint_points,
            source: "waypoint",
        });
    }

    bail!("GPX file did not contain any track, route, or waypoint coordinates")
}

fn replay_point_from_waypoint(waypoint: &Waypoint) -> anyhow::Result<ReplayPoint> {
    let point = waypoint.point();
    let longitude = point.x();
    let latitude = point.y();
    validate_coordinate("latitude", latitude)?;
    validate_coordinate("longitude", longitude)?;

    Ok(ReplayPoint {
        latitude,
        longitude,
        time: waypoint.time.map(Into::into),
    })
}

fn replay_delay(
    current: &ReplayPoint,
    next: &ReplayPoint,
    fallback: Duration,
    respect_time: bool,
    time_scale: f64,
) -> Duration {
    if respect_time && let (Some(current_time), Some(next_time)) = (current.time, next.time) {
        let delta = next_time - current_time;
        if delta.is_positive() {
            return Duration::from_secs_f64(delta.as_seconds_f64() / time_scale);
        }
    }

    fallback
}

async fn wait_for_ctrl_c_or_timeout(duration: Duration) -> anyhow::Result<bool> {
    if duration.is_zero() {
        return Ok(false);
    }

    tokio::select! {
        result = tokio::signal::ctrl_c() => {
            result.context("failed while waiting for Ctrl-C")?;
            Ok(true)
        }
        _ = tokio::time::sleep(duration) => Ok(false),
    }
}

async fn wait_for_ctrl_c() -> anyhow::Result<()> {
    tokio::signal::ctrl_c()
        .await
        .context("failed while waiting for Ctrl-C")
}

async fn clear_location(
    location: &mut LocationSimulationClient<'_, Box<dyn ReadWrite>>,
) -> anyhow::Result<()> {
    location
        .clear()
        .await
        .context("failed to clear location during shutdown")?;
    eprintln!("location simulation cleared");
    Ok(())
}

fn pick_tunneld_device(
    devices: HashMap<String, TunneldDevice>,
    requested_udid: Option<&str>,
) -> anyhow::Result<(String, TunneldDevice)> {
    if devices.is_empty() {
        bail!("no devices exposed by tunneld");
    }

    if let Some(udid) = requested_udid {
        return devices
            .into_iter()
            .find(|(candidate, _)| candidate == udid)
            .ok_or_else(|| anyhow!("device {udid} not found in tunneld output"));
    }

    let mut entries: Vec<_> = devices.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    if entries.len() > 1 {
        eprintln!(
            "multiple devices found; defaulting to the first UDID {} (pass --udid to choose another)",
            entries[0].0
        );
    }

    entries
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no devices available after filtering"))
}

fn pick_usbmuxd_device(
    devices: Vec<UsbmuxdDevice>,
    requested_udid: Option<&str>,
) -> anyhow::Result<UsbmuxdDevice> {
    if devices.is_empty() {
        bail!("no devices exposed by usbmuxd");
    }

    if let Some(udid) = requested_udid {
        return devices
            .into_iter()
            .find(|device| device.udid == udid)
            .ok_or_else(|| anyhow!("device {udid} not found in usbmuxd output"));
    }

    let mut usb_devices: Vec<_> = devices
        .iter()
        .filter(|device| device.connection_type == Connection::Usb)
        .cloned()
        .collect();
    usb_devices.sort_by(|a, b| a.udid.cmp(&b.udid));

    if let Some(device) = usb_devices.into_iter().next() {
        return Ok(device);
    }

    let mut devices = devices;
    devices.sort_by(|a, b| a.udid.cmp(&b.udid));
    devices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no devices available after filtering"))
}

fn ensure_positive(label: &str, value: f64) -> anyhow::Result<()> {
    if value <= 0.0 {
        bail!("{label} must be greater than 0, got {value}");
    }
    Ok(())
}

fn validate_coordinate(label: &str, value: f64) -> anyhow::Result<()> {
    if !value.is_finite() {
        bail!("{label} must be a finite number");
    }

    match label {
        "latitude" if !(-90.0..=90.0).contains(&value) => {
            bail!("{label} out of range: {value}");
        }
        "longitude" if !(-180.0..=180.0).contains(&value) => {
            bail!("{label} out of range: {value}");
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn set_parses_coordinates_and_place_options() {
        let cli = Cli::try_parse_from(["ilocation", "set", "-33.9", "-151.2"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Set {
                longitude: Some(-151.2),
                ..
            }
        ));
        let cli = Cli::try_parse_from(["ilocation", "set", "Pasir Ris 8", "--poi", "--pick", "1"])
            .unwrap();
        assert!(matches!(
            cli.command,
            Command::Set {
                longitude: None,
                poi: true,
                pick: Some(1),
                ..
            }
        ));
        assert!(Cli::try_parse_from(["ilocation", "set", "1", "103", "--poi"]).is_err());
        assert!(Cli::try_parse_from(["ilocation", "set", "place", "--pick", "0"]).is_err());
        for flag in ["--yes", "-y"] {
            let cli = Cli::try_parse_from(["ilocation", "set", "place", flag]).unwrap();
            assert!(matches!(cli.command, Command::Set { yes: true, .. }));
        }
        assert!(
            Cli::try_parse_from(["ilocation", "set", "place", "--yes", "--pick", "2"]).is_err()
        );
        assert!(Cli::try_parse_from(["ilocation", "set", "1", "103", "--yes"]).is_err());
        assert_eq!(
            resolve_set("-33.9", Some(-151.2), false, None).unwrap(),
            (-33.9, -151.2)
        );
        assert!(resolve_set("91", Some(103.0), false, None).is_err());
        assert!(resolve_set("1.3", None, false, None).is_err());
        assert!(resolve_set(" ", None, false, None).is_err());
    }

    #[test]
    fn prefers_track_points_over_waypoints() {
        let gpx = read_gpx(Cursor::new(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="test" xmlns="http://www.topografix.com/GPX/1/1">
  <wpt lat="35.0" lon="139.0"></wpt>
  <trk>
    <name>demo</name>
    <trkseg>
      <trkpt lat="34.7570038" lon="138.9875358">
        <time>2026-04-19T12:00:00Z</time>
      </trkpt>
      <trkpt lat="34.7571038" lon="138.9876358">
        <time>2026-04-19T12:00:02Z</time>
      </trkpt>
    </trkseg>
  </trk>
</gpx>"#,
        ))
        .unwrap();

        let replay = extract_replay_points(&gpx).unwrap();
        assert_eq!(replay.source, "track");
        assert_eq!(replay.points.len(), 2);
        assert_eq!(replay.points[0].latitude, 34.7570038);
        assert_eq!(replay.points[0].longitude, 138.9875358);
        assert!(replay.points[0].time.is_some());
    }

    #[test]
    fn falls_back_to_route_points() {
        let gpx = read_gpx(Cursor::new(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="test" xmlns="http://www.topografix.com/GPX/1/1">
  <rte>
    <rtept lat="34.0" lon="138.0"></rtept>
    <rtept lat="34.1" lon="138.1"></rtept>
  </rte>
</gpx>"#,
        ))
        .unwrap();

        let replay = extract_replay_points(&gpx).unwrap();
        assert_eq!(replay.source, "route");
        assert_eq!(replay.points.len(), 2);
    }

    #[test]
    fn uses_fixed_interval_when_no_timestamps_are_available() {
        let current = ReplayPoint {
            latitude: 34.0,
            longitude: 138.0,
            time: None,
        };
        let next = ReplayPoint {
            latitude: 34.1,
            longitude: 138.1,
            time: None,
        };

        let delay = replay_delay(&current, &next, Duration::from_secs_f64(1.5), true, 2.0);
        assert_eq!(delay, Duration::from_secs_f64(1.5));
    }
}
