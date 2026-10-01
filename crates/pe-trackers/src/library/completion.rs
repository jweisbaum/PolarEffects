//! Scraping requires affirmative completion evidence, not an old last position.
use crate::{Result, TrackerEvent};

/// An unfinished leg can still identify other legs worth checking separately.
#[derive(Debug)]
pub enum ScrapeFetch {
    Finished(TrackerEvent),
    Skipped { legs: Option<(u32, u32)> },
}

pub fn terminal_status(status: &str) -> bool {
    matches!(
        status.trim().to_ascii_uppercase().as_str(),
        "FINISHED"
            | "COMPLETED"
            | "RETIRED"
            | "DNF"
            | "DNS"
            | "DSQ"
            | "ARR"
            | "ARV"
            | "RET"
            | "ABD"
    )
}

/// All boats must have a published terminal result. Tracking-window ends and
/// last fixes alone never prove that a race has finished. Future dates veto it.
pub fn is_finished(event: &TrackerEvent, now: i64) -> bool {
    let not_future = |t: Option<i64>| t.is_none_or(|t| t > 0 && t <= now);
    not_future(event.start)
        && not_future(event.stop)
        && !event.boats.is_empty()
        && event.boats.iter().all(|boat| {
            not_future(boat.start)
                && not_future(boat.finish)
                && boat.status.as_deref().is_some_and(terminal_status)
        })
}

impl ScrapeFetch {
    pub fn checked(event: TrackerEvent, now: i64) -> Self {
        if is_finished(&event, now) {
            Self::Finished(event)
        } else {
            Self::Skipped { legs: event.leg }
        }
    }
}

/// Keep the download behind the metadata check, including explicitly supplied
/// URLs. A second check refuses a race whose metadata changed during the fetch.
pub(crate) fn after_check(
    metadata: TrackerEvent,
    now: i64,
    download: impl FnOnce() -> Result<TrackerEvent>,
) -> Result<ScrapeFetch> {
    if !is_finished(&metadata, now) {
        return Ok(ScrapeFetch::Skipped { legs: metadata.leg });
    }
    Ok(ScrapeFetch::checked(download()?, now))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EventRef, TrackerBoat, event::PositionsFrom};
    use pe_core::track::Tracker;

    fn event(status: Option<&str>) -> TrackerEvent {
        TrackerEvent {
            event: EventRef {
                tracker: Tracker::YellowBrick,
                key: "race".into(),
                url: "race".into(),
            },
            title: "Race".into(),
            start: Some(10),
            stop: Some(30),
            boats: vec![TrackerBoat {
                details: Default::default(),
                id: "1".into(),
                name: "Boat".into(),
                sail: None,
                model: None,
                division: None,
                status: status.map(str::to_owned),
                start: Some(10),
                finish: Some(30),
                fixes: vec![],
            }],
            positions_from: PositionsFrom::Primary,
            leg: Some((2, 3)),
        }
    }

    #[test]
    fn only_terminal_results_for_every_boat_qualify() {
        for status in [
            None,
            Some("RACING"),
            Some("UNKNOWN"),
            Some(""),
            Some("STARTED"),
        ] {
            assert!(!is_finished(&event(status), 100));
        }
        for status in ["FINISHED", " retired ", "DNF", "DNS", "DSQ", "ARV", "ABD"] {
            assert!(is_finished(&event(Some(status)), 100));
        }
        let mut race = event(Some("FINISHED"));
        race.boats.push(event(Some("RACING")).boats.remove(0));
        assert!(!is_finished(&race, 100));
        race.boats.clear();
        assert!(!is_finished(&race, 100));
    }

    #[test]
    fn future_dates_and_invalid_timestamps_veto_completion() {
        let race = event(Some("FINISHED"));
        for field in 0..4 {
            for stamp in [0, -1, 101] {
                let mut race = race.clone();
                match field {
                    0 => race.start = Some(stamp),
                    1 => race.stop = Some(stamp),
                    2 => race.boats[0].start = Some(stamp),
                    _ => race.boats[0].finish = Some(stamp),
                }
                assert!(!is_finished(&race, 100));
            }
        }
    }

    #[test]
    fn unfinished_metadata_never_starts_download_and_preserves_legs() {
        let outcome = after_check(event(Some("RACING")), 100, || panic!("download started"));
        assert!(matches!(
            outcome,
            Ok(ScrapeFetch::Skipped { legs: Some((2, 3)) })
        ));
        let outcome = after_check(event(Some("FINISHED")), 100, || Ok(event(Some("RACING"))));
        assert!(matches!(outcome, Ok(ScrapeFetch::Skipped { .. })));
    }
}
