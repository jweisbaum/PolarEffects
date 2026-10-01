//! Manual, opt-in catalogue/code verification. Reads credentials from local
//! app settings; neither command-line arguments nor output contain secrets.
use pe_trackers::{
    Fetcher,
    library::yellowbrick::{Credentials, discover},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if !(3..=4).contains(&args.len()) {
        return Err(
            "usage: discover_yellowbrick SETTINGS_FILE REPORT_FILE [KNOWN_ID_URLS_JSON]".into(),
        );
    }
    let settings: serde_json::Value = serde_json::from_slice(&std::fs::read(&args[1])?)?;
    let db = &settings["database"];
    let credentials = Credentials {
        user_key: db["yellowbrick_user_key"]
            .as_str()
            .ok_or("Missing YellowBrick user key")?,
        device_id: db["yellowbrick_device_id"]
            .as_str()
            .ok_or("Missing YellowBrick device ID")?,
    };
    let fetcher = Fetcher::new(
        "YellowBrick",
        Duration::from_secs(30),
        Arc::new(AtomicBool::new(false)),
    )?;
    let known = if let Some(path) = args.get(3) {
        serde_json::from_slice(&std::fs::read(path)?)?
    } else {
        BTreeMap::new()
    };
    let report = discover(&fetcher, Some(&credentials), &known, &mut |message| {
        eprintln!("{message}")
    })?;
    std::fs::write(&args[2], serde_json::to_vec_pretty(&report)?)?;
    println!(
        "{} catalogue races; {} URLs; {} unresolved; {} association warnings",
        report.races.len(),
        report.urls().len(),
        report.unresolved(),
        report.warnings.len()
    );
    // Verify actual public RaceSetup responses, not just URL syntax. Spread
    // the sample over the catalogue and avoid fetching any boat tracks here.
    let mut verified = 0;
    for index in [
        0,
        report.races.len() / 2,
        report.races.len().saturating_sub(1),
        report
            .races
            .iter()
            .position(|r| r.id == "2634")
            .unwrap_or(0),
    ] {
        let Some(race) = report.races.get(index) else {
            continue;
        };
        let Some(url) = race.urls.first() else {
            continue;
        };
        let event = pe_trackers::library::resolve_source("YELLOWBRICK", url)?;
        let result = fetcher
            .get(
                &format!(
                    "{}/JSON/{}/RaceSetup",
                    pe_trackers::yellowbrick::CDN,
                    event.key
                ),
                &mut |_, _| {},
            )
            .and_then(|bytes| pe_trackers::yellowbrick::parse_race_setup(&bytes));
        match result {
            Ok(setup) => {
                verified += 1;
                println!("Verified {}: {} ({})", race.id, setup.title, url);
            }
            Err(error) => eprintln!("Unavailable race {} ({}): {}", race.id, url, error),
        }
    }
    if verified == 0 {
        return Err("No sampled public race endpoint was available".into());
    }
    Ok(())
}
