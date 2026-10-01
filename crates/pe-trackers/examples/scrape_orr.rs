//! Refresh the bundled public ORR snapshot, explicitly invoked by a maintainer.
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let year: i32 = args
        .get(1)
        .ok_or("usage: scrape_orr YEAR OUTPUT.json")?
        .parse()?;
    let path = args.get(2).ok_or("usage: scrape_orr YEAR OUTPUT.json")?;
    let fetcher = pe_trackers::Fetcher::new(
        "ORR",
        Duration::from_secs(60),
        Arc::new(AtomicBool::new(false)),
    )?;
    let mut failures = Vec::new();
    let records = pe_trackers::orr::scrape(
        &fetcher,
        year,
        &mut |done, total| {
            if done % 10 == 0 || done == total {
                eprintln!("ORR: {done}/{total} certificates");
            }
        },
        &mut |cert, why| {
            eprintln!("ORR skipped {cert}: {why}");
            failures.push(cert.to_owned());
        },
    )?;
    if !failures.is_empty() {
        return Err(format!(
            "{} certificates failed; snapshot was not replaced",
            failures.len()
        )
        .into());
    }
    if records.is_empty() {
        return Err("no boat-speed polars found; snapshot was not replaced".into());
    }
    pe_core::io::write_atomic(Path::new(path), &serde_json::to_vec(&records)?)?;
    eprintln!(
        "Saved {} distinct certificate variants to {path}",
        records.len()
    );
    Ok(())
}
