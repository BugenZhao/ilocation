use std::io::{self, IsTerminal, Write};

use anyhow::{Context, bail};

#[derive(Debug)]
pub struct Place {
    pub name: String,
    pub address: String,
    pub latitude: f64,
    pub longitude: f64,
}

pub fn print_places(places: &[Place], output: &mut impl Write) -> io::Result<()> {
    if places.is_empty() {
        writeln!(output, "No places found.")?;
    }
    for (index, place) in places.iter().enumerate() {
        writeln!(
            output,
            "{}. {} -- {} ({:.7}, {:.7})",
            index + 1,
            place.name,
            place.address.replace(['\n', '\r'], ", "),
            place.latitude,
            place.longitude
        )?;
    }
    Ok(())
}

pub fn select(places: Vec<Place>, pick: Option<u32>) -> anyhow::Result<(f64, f64)> {
    if places.is_empty() {
        bail!("MapKit returned no places; try a more specific query or --poi");
    }
    print_places(&places, &mut io::stderr().lock())?;
    let index = match pick {
        Some(index) => index,
        None => {
            if !io::stdin().is_terminal() {
                bail!("select a result with --pick <NUMBER> when stdin is non-interactive");
            }
            eprint!("Choose a result [1-{}], or Enter to cancel: ", places.len());
            io::stderr().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if input.trim().is_empty() {
                bail!("location selection cancelled");
            }
            input
                .trim()
                .parse::<u32>()
                .context("selection must be a result number")?
        }
    };
    let place = index
        .checked_sub(1)
        .and_then(|i| places.get(i as usize))
        .context("selected result is out of range")?;
    crate::validate_coordinate("latitude", place.latitude)?;
    crate::validate_coordinate("longitude", place.longitude)?;
    eprintln!(
        "selected {} at {},{}",
        place.name, place.latitude, place.longitude
    );
    Ok((place.latitude, place.longitude))
}

#[cfg(not(target_os = "macos"))]
pub fn search(_: &str, _: bool) -> anyhow::Result<Vec<Place>> {
    bail!("place search requires macOS 26 or newer; supply latitude and longitude on this platform")
}

#[cfg(target_os = "macos")]
pub fn search(query: &str, poi: bool) -> anyhow::Result<Vec<Place>> {
    if query.trim().is_empty() {
        bail!("place query must contain text");
    }
    use block2::RcBlock;
    use objc2::AllocAnyThread;
    use objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSError, NSRunLoop, NSString};
    use objc2_map_kit::{
        MKLocalSearch, MKLocalSearchRequest, MKLocalSearchResponse, MKLocalSearchResultType,
    };
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    if !objc2::available!(macos = 26.0, ..) {
        bail!(
            "place search requires macOS 26 or newer; supply latitude and longitude on older systems"
        );
    }
    let (tx, rx) = mpsc::channel();
    // SAFETY: Called on the process main thread. Framework objects and the block
    // remain alive while pumping the run loop. Callback pointers are used only
    // during invocation; the channel carries owned Rust data. Late callbacks after
    // cancellation safely drop their result when the receiver is gone.
    unsafe {
        let request = MKLocalSearchRequest::new();
        request.setNaturalLanguageQuery(Some(&NSString::from_str(query)));
        if poi {
            request.setResultTypes(MKLocalSearchResultType::PointOfInterest);
        }
        let search = MKLocalSearch::initWithRequest(MKLocalSearch::alloc(), &request);
        let callback = RcBlock::new(
            move |response: *mut MKLocalSearchResponse, error: *mut NSError| {
                let result = if let Some(error) = error.as_ref() {
                    Err(anyhow::anyhow!("MapKit search failed: {error:?}"))
                } else if let Some(response) = response.as_ref() {
                    Ok(response
                        .mapItems()
                        .iter()
                        .map(|item| {
                            let coordinate = item.location().coordinate();
                            Place {
                                name: item.name().map(|s| s.to_string()).unwrap_or_default(),
                                address: item
                                    .address()
                                    .map(|a| a.fullAddress().to_string())
                                    .unwrap_or_default(),
                                latitude: coordinate.latitude,
                                longitude: coordinate.longitude,
                            }
                        })
                        .collect())
                } else {
                    Err(anyhow::anyhow!("MapKit returned an empty response"))
                };
                let _ = tx.send(result);
            },
        );
        eprintln!(
            "searching Apple Maps for {query:?}{}",
            if poi { " (POI only)" } else { "" }
        );
        search.startWithCompletionHandler(RcBlock::as_ptr(&callback));
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Ok(result) = rx.try_recv() {
                return result;
            }
            if Instant::now() >= deadline {
                search.cancel();
                bail!("MapKit search timed out after 20 seconds");
            }
            NSRunLoop::mainRunLoop().runMode_beforeDate(
                NSDefaultRunLoopMode,
                &NSDate::dateWithTimeIntervalSinceNow(0.1),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_selection_and_bounds() {
        let places = || {
            vec![Place {
                name: "Example".into(),
                address: "Address".into(),
                latitude: 1.0,
                longitude: 103.0,
            }]
        };
        assert_eq!(select(places(), Some(1)).unwrap(), (1.0, 103.0));
        assert!(select(places(), Some(0)).is_err());
        assert!(select(places(), Some(2)).is_err());
        assert!(select(vec![], Some(1)).is_err());
    }
}
