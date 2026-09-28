#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Live requests to the trackers. Never in the default suite: `#[ignore]`
//! and gated on `PE_TEST_LIVE=1`.
//!
//! ```text
//! PE_TEST_LIVE=1 cargo test -p pe-trackers --test live -- --ignored --nocapture
//! ```

use std::time::{Duration, Instant};

use pe_trackers::yellowbrick;

fn live() -> bool {
    std::env::var("PE_TEST_LIVE").is_ok_and(|v| v == "1")
}

/// The Fastnet 2025 event end to end, as Appendix B recorded it.
#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn yellowbrick_fastnet_2025() {
    if !live() {
        return;
    }
    let start = Instant::now();
    let key = yellowbrick::race_key("https://yb.tl/fastnet2025").unwrap();
    let event = yellowbrick::fetch_event(&key, Duration::from_secs(60)).expect("fetches");
    let fixes: usize = event.positions.teams.iter().map(|t| t.moments.len()).sum();
    println!(
        "M3 | YellowBrick fastnet2025 live: {} teams, {fixes} fixes, {:.2} s",
        event.positions.teams.len(),
        start.elapsed().as_secs_f64()
    );
    assert_eq!(event.setup.title, "Rolex Fastnet 2025");
    assert_eq!(event.setup.teams.len(), 444);
    assert_eq!(event.positions.teams.len(), 444);
}
