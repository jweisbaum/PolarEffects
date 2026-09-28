#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Every sample polar in `polar_examples/` parses (M9b, user request).
//!
//! The directory holds 613 Adrena-style `.pol` grids and 75 Expedition
//! `.txt` files collected from real boats, not written for this test. They
//! exercise variants the golden/malformed fixtures don't: a TWA/TWS axis
//! running to 70 kn, a `TWA` grid corner without `\TWS`, and two Expedition
//! files whose first row is a label row (`twa0 bsp0 TwaUp bspUp …`, or
//! `pol Twa0 Bsp0 UpTwa UpBsp …` space-separated) naming the columns instead
//! of holding data.

use std::path::{Path, PathBuf};

use pe_polar::{Polar, read};

/// Files that are not boat-speed polars at all, so they are excluded from
/// "every file parses" rather than loosening what a polar file may hold:
///
/// - `polars/J46 heel.txt` is a **heel angle** table (Expedition-shaped:
///   `TWS` then `TWA angle` pairs), not boat speed. Its angles are the boat's
///   heel in degrees, several of them negative (heeled to port), which the
///   `negative` refusal correctly rejects — a real polar file never holds a
///   negative speed. Loosening that check to admit this file would also
///   admit corrupt boat-speed files with a stray negative sign.
const EXCLUDED: &[&str] = &["polars/J46 heel.txt"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../polar_examples")
}

/// Every `.pol`/`.txt` file under `polar_examples/`, recursively, with its
/// path relative to that root (`/`-separated, for a stable [`EXCLUDED`]).
fn all_files() -> Vec<(String, PathBuf)> {
    let root = root();
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("polar_examples/ is a checked-in fixture") {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase);
            if !matches!(ext.as_deref(), Some("pol" | "txt")) {
                continue; // .DS_Store and anything else that isn't a polar.
            }
            let relative = path
                .strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push((relative, path));
        }
    }
    out.sort();
    out
}

fn cell(polar: &Polar, twa: f64, tws: f64) -> Option<f64> {
    let i = polar.twa.iter().position(|a| *a == twa)?;
    let j = polar.tws.iter().position(|s| *s == tws)?;
    polar.get(i, j)
}

#[test]
fn every_sample_polar_parses_except_the_excluded_ones() {
    let files = all_files();
    assert_eq!(files.len(), 688, "polar_examples/ should hold 688 files");

    let mut parsed_count = 0;
    for (relative, path) in &files {
        let bytes = std::fs::read(path).unwrap();
        if EXCLUDED.contains(&relative.as_str()) {
            assert!(
                read(&bytes).is_err(),
                "{relative} is in EXCLUDED as not a boat-speed polar, but it parsed"
            );
            continue;
        }
        let parsed = read(&bytes).unwrap_or_else(|e| panic!("{relative}: {e}"));
        parsed
            .polar
            .validate()
            .unwrap_or_else(|e| panic!("{relative}: {e}"));
        parsed_count += 1;
    }
    assert_eq!(parsed_count, files.len() - EXCLUDED.len());
}

#[test]
fn the_excluded_heel_table_fails_as_a_negative_speed() {
    let bytes = std::fs::read(root().join("polars/J46 heel.txt")).unwrap();
    let err = read(&bytes).unwrap_err();
    assert_eq!(err.reason.code(), "negative");
}

/// Hand-read spot checks against the file's own numbers, not the code's.
#[test]
fn spot_checks_match_the_files_by_hand() {
    // Farr 40.txt: `!Expedition polar` comment, then headerless rows.
    // "6\t30\t3.72\t44.3\t5.60\t50\t6.09\t60\t6.67\t70\t6.99\t75\t7.08\t80\t
    // 7.12\t90\t7.09\t…" — TWS 6, TWA 90 = 7.09.
    let farr40 = read(&std::fs::read(root().join("polars/Farr 40.txt")).unwrap()).unwrap();
    assert_eq!(cell(&farr40.polar, 90.0, 6.0), Some(7.09));

    // VR_IMOCA.pol: Adrena grid, TWA\TWS corner, TWS axis 0..70. Row "TWA=1"
    // is "1 0 0.02 0.04 0.051 0.061 0.072 0.069 …" for TWS 0,1,2,3,4,5,6,7:
    // TWS 5 = 0.072, and the TWS axis does reach 70 without refusal.
    let imoca = read(&std::fs::read(root().join("Polaires - Copy/VR_IMOCA.pol")).unwrap()).unwrap();
    assert_eq!(cell(&imoca.polar, 1.0, 5.0), Some(0.072));
    assert_eq!(imoca.polar.tws.last(), Some(&70.0));
    // TWA 0 is a whole row of zeros, and TWS 0 a whole column of zeros: both
    // are accepted (a boat that makes no way in no wind is still a zero).
    assert_eq!(cell(&imoca.polar, 0.0, 0.0), Some(0.0));
    assert_eq!(cell(&imoca.polar, 0.0, 70.0), Some(0.0));
    assert_eq!(cell(&imoca.polar, 1.0, 0.0), Some(0.0));

    // Swan 78.txt: a tab-led label row (`\ttwa0\tbsp0\tTwaUp\tbspUp…`),
    // skipped, then "6\t0\t0\t45\t7.04\t52\t7.78\t60\t8.39\t75\t9.04\t90\t
    // 9.16\t…" — TWS 6, TWA 90 = 9.16.
    let swan78 = read(&std::fs::read(root().join("polars/Swan 78.txt")).unwrap()).unwrap();
    assert_eq!(cell(&swan78.polar, 90.0, 6.0), Some(9.16));

    // J35.txt: a space-separated label row (`pol  Twa0  Bsp0  UpTwa
    // UpBsp…`), skipped, then "6.3  30.0  0.00  45.1  4.95  53  5.50  60
    // 5.87  70  6.19  80  6.33  90  6.48…" — TWS 6.3, TWA 90 = 6.48.
    let j35 = read(&std::fs::read(root().join("polars/J35.txt")).unwrap()).unwrap();
    assert_eq!(cell(&j35.polar, 90.0, 6.3), Some(6.48));
}
