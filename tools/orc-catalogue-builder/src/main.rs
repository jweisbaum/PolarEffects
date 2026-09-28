//! Builds `crates/pe-orc/data/catalogue.bin` from a jieter/orc-data checkout
//! (spec.md 5.1).
//!
//! ```text
//! cargo run -p orc-catalogue-builder --release -- ../orc-data/site/data \
//!     crates/pe-orc/data/catalogue.bin
//! ```
//!
//! The per-boat files are read **from the checkout's `HEAD` commit** through
//! `git`, not from the working tree. orc-data holds file names that differ
//! only in case (`FIN/FIN71.json`, `FIN/Fin71.json`), which a macOS or
//! Windows checkout silently collapses into one; reading the commit sees
//! every file and makes the result independent of local edits. The commit's
//! hash and date are recorded in the catalogue.
//!
//! Every file that is left out is reported on stderr with its reason: the
//! per-boat schema has changed over the years, and a silently shorter
//! catalogue would be a bug nobody sees.
//!
//! # The certificate year
//!
//! The per-boat files do not say which year's certificate they hold. orc-data
//! rewrites a boat's file each year it has a certificate, and the VPP's wind
//! speeds changed over time: 6–20 kn (7 speeds) up to 2023, 6–24 kn in 2024,
//! 4–24 kn from 2025. So the year is read from the axis, and within the
//! 6–20 kn era from the yearly lists (`ALL<year>.json`, Python literals, of
//! which only the sail numbers are read): the latest year up to 2023 whose
//! list holds the boat. From 2025 it is the latest yearly list from 2025 that
//! holds the boat, else the `YEAR` the checkout's Makefile fetches. When none
//! of this identifies a year it is left empty rather than guessed.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use pe_orc::format::{self, Entry, Provenance, Vpp, hundredths};
use serde_json::Value;

/// Anything faster is not a boat speed (pe-polar refuses it in files too).
const MAX_SPEED_KN: f64 = 60.0;

type Failure = Box<dyn std::error::Error>;

fn main() {
    if let Err(err) = run() {
        eprintln!("orc-catalogue-builder: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Failure> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [data_dir, out] = args.as_slice() else {
        return Err("usage: orc-catalogue-builder <orc-data>/site/data <catalogue.bin>".into());
    };
    let data_dir = PathBuf::from(data_dir);
    let git = Git::at(&data_dir)?;
    let prefix = git.text(&["rev-parse", "--show-prefix"])?;
    let commit = git.text(&["rev-parse", "HEAD"])?;
    let commit_date = git.text(&["log", "-1", "--format=%cs", "HEAD"])?;

    // The yearly lists and the Makefile's year, from the top of the checkout.
    let top = git.tree("")?;
    let mut lists = YearLists::default();
    let mut list_blobs = Vec::new();
    for (path, sha) in &top {
        if let Some(year) = path
            .strip_prefix("ALL")
            .and_then(|rest| rest.strip_suffix(".json"))
            .and_then(|year| year.parse::<i32>().ok())
        {
            list_blobs.push((year, sha.clone()));
        }
    }
    let makefile = top.iter().find(|(path, _)| path == "Makefile");
    let mut wanted: Vec<String> = list_blobs.iter().map(|(_, sha)| sha.clone()).collect();
    wanted.extend(makefile.map(|(_, sha)| sha.clone()));
    let blobs = git.blobs(&wanted)?;
    for ((year, _), bytes) in list_blobs.iter().zip(&blobs) {
        lists.add(*year, &String::from_utf8_lossy(bytes));
    }
    let make_year = makefile
        .and_then(|_| blobs.last())
        .and_then(|bytes| makefile_year(&String::from_utf8_lossy(bytes)));

    // The per-boat files: <prefix><CC>/<sail>.json.
    let files: Vec<(String, String)> = git
        .tree(&prefix)?
        .into_iter()
        .filter(|(path, _)| {
            let rest = path.strip_prefix(prefix.as_str()).unwrap_or(path);
            rest.ends_with(".json") && rest.split('/').count() == 2
        })
        .collect();
    let contents = git.blobs(&files.iter().map(|(_, sha)| sha.clone()).collect::<Vec<_>>())?;

    let mut kept: Vec<(String, Entry)> = Vec::new();
    let mut dropped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for ((path, _), bytes) in files.iter().zip(&contents) {
        let parsed = serde_json::from_slice::<Value>(bytes)
            .map_err(|e| Drop::new("not JSON", e.to_string()))
            .and_then(|value| entry(&value, &lists, make_year));
        match parsed {
            Ok(entry) => kept.push((path.clone(), entry)),
            Err(drop) => dropped
                .entry(drop.reason.to_owned())
                .or_default()
                .push(format!("{path}: {}", drop.detail)),
        }
    }

    // A stable order, whatever order git listed the files in.
    kept.sort_by(|(pa, a), (pb, b)| {
        (&a.country, &a.sail_no, &a.name, pa).cmp(&(&b.country, &b.sail_no, &b.name, pb))
    });
    let mut entries: Vec<Entry> = Vec::with_capacity(kept.len());
    let mut last_path = String::new();
    for (path, entry) in kept {
        if entries.last() == Some(&entry) {
            dropped
                .entry("identical to another file".to_owned())
                .or_default()
                .push(format!("{path}: same record as {last_path}"));
            continue;
        }
        last_path = path;
        entries.push(entry);
    }

    let dropped_count: usize = dropped.values().map(Vec::len).sum();
    let provenance = Provenance {
        source: "jieter/orc-data".to_owned(),
        commit: commit.clone(),
        commit_date,
        build_date: build_date()?,
        records: u32::try_from(entries.len())?,
        dropped: u32::try_from(dropped_count)?,
    };
    let bytes = format::encode(&provenance, &entries)?;
    // Refuse to write something the app could not read back.
    let (_, check) = format::decode(&bytes)?;
    if check != entries {
        return Err("the encoded catalogue does not read back as written".into());
    }
    std::fs::write(out, &bytes)?;

    let mut years: BTreeMap<Option<i32>, usize> = BTreeMap::new();
    for entry in &entries {
        *years.entry(entry.certificate_year).or_default() += 1;
    }
    eprintln!(
        "orc-data {commit} ({}): {} files read, {} kept, {} dropped",
        provenance.commit_date,
        files.len(),
        entries.len(),
        dropped_count
    );
    for (year, count) in &years {
        match year {
            Some(year) => eprintln!("  certificate year {year}: {count}"),
            None => eprintln!("  certificate year unknown: {count}"),
        }
    }
    for (reason, paths) in &dropped {
        eprintln!("dropped, {reason} ({}):", paths.len());
        for path in paths {
            eprintln!("  {path}");
        }
    }
    eprintln!("wrote {out}: {} bytes", bytes.len());
    Ok(())
}

/// A file left out of the catalogue, and why.
#[derive(Debug, PartialEq, Eq)]
struct Drop {
    /// The kind of problem, to group the report by.
    reason: &'static str,
    /// What exactly was wrong.
    detail: String,
}

impl Drop {
    fn new(reason: &'static str, detail: impl Into<String>) -> Self {
        Self {
            reason,
            detail: detail.into(),
        }
    }
}

/// Which boats each yearly list holds, by orc-data sail number.
#[derive(Debug, Default)]
struct YearLists {
    years: BTreeMap<i32, BTreeSet<String>>,
}

impl YearLists {
    /// Reads the sail numbers out of one `ALL<year>.json`. The file is a
    /// Python literal, not JSON; only `'sailnumber': '…'` is needed.
    fn add(&mut self, year: i32, text: &str) {
        let set = self.years.entry(year).or_default();
        for quote in ['\'', '"'] {
            let key = format!("{quote}sailnumber{quote}: ");
            let mut rest = text;
            while let Some(at) = rest.find(&key) {
                rest = &rest[at + key.len()..];
                let mut chars = rest.chars();
                let Some(open) = chars.next().filter(|c| *c == '\'' || *c == '"') else {
                    continue;
                };
                let value = chars.as_str();
                if let Some(end) = value.find(open) {
                    set.insert(value[..end].to_owned());
                    rest = &value[end..];
                }
            }
        }
    }

    /// The latest year in `years` whose list holds `sail`.
    fn latest(&self, sail: &str, years: impl std::ops::RangeBounds<i32>) -> Option<i32> {
        self.years
            .range(years)
            .rev()
            .find(|(_, set)| set.contains(sail))
            .map(|(year, _)| *year)
    }
}

/// The `YEAR = 2025` line of orc-data's Makefile.
fn makefile_year(text: &str) -> Option<i32> {
    text.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "YEAR")
            .then(|| value.trim().parse().ok())
            .flatten()
    })
}

/// The certificate year of a file, from its wind-speed axis (in hundredths)
/// and the yearly lists. See the module documentation.
fn certificate_year(
    sail: &str,
    speeds: &[u16],
    lists: &YearLists,
    make_year: Option<i32>,
) -> Option<i32> {
    match (speeds.first(), speeds.last(), speeds.len()) {
        (Some(400), Some(2400), _) => lists
            .latest(sail, 2025..)
            .or(make_year.filter(|year| *year >= 2025)),
        (Some(600), Some(2400), _) => Some(2024),
        (Some(600), Some(2000), 7) => lists.latest(sail, ..=2023),
        _ => None,
    }
}

/// The sail number as shown: `"GBR/GBR1124"` is `"GBR 1124"`, and orc-data's
/// stand-in for a missing number (`"GBR/_3"`) is empty.
fn sail_display(sailnumber: &str, country: &str) -> String {
    let raw = sailnumber
        .split_once('/')
        .map_or(sailnumber, |(_, sail)| sail)
        .trim();
    if raw.is_empty() || raw.starts_with('_') {
        return String::new();
    }
    let rest = raw
        .get(..country.len())
        .filter(|head| !country.is_empty() && head.eq_ignore_ascii_case(country))
        .and_then(|_| raw.get(country.len()..))
        .map(|rest| rest.trim_start_matches([' ', '-', '/']))
        .filter(|rest| !rest.is_empty())
        .unwrap_or(raw);
    if country.is_empty() {
        rest.to_owned()
    } else {
        format!("{country} {rest}")
    }
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// A size or rating: a positive number, else nothing (orc-data writes 0 for
/// "no spinnaker").
fn positive(value: &Value) -> Option<f64> {
    value.as_f64().filter(|v| v.is_finite() && *v > 0.0)
}

/// A list of numbers given to two decimals, in `range`.
fn hundredths_list(
    value: &Value,
    name: &str,
    range: std::ops::RangeInclusive<f64>,
) -> Result<Vec<u16>, Drop> {
    let list = value
        .as_array()
        .ok_or_else(|| Drop::new("VPP field missing", format!("no {name} list")))?;
    list.iter()
        .map(|item| {
            let number = item
                .as_f64()
                .ok_or_else(|| Drop::new("VPP value not a number", format!("{name}: {item}")))?;
            if !range.contains(&number) {
                return Err(Drop::new(
                    "VPP value out of range",
                    format!("{name}: {number} is outside {range:?}"),
                ));
            }
            hundredths(number)
                .ok_or_else(|| Drop::new("VPP value finer than 0.01", format!("{name}: {number}")))
        })
        .collect()
}

fn increasing(values: &[u16], name: &str) -> Result<(), Drop> {
    if values.is_empty() || values.windows(2).any(|w| w[0] >= w[1]) {
        return Err(Drop::new(
            "VPP axis not increasing",
            format!("{name} is empty or not strictly increasing"),
        ));
    }
    Ok(())
}

/// The key orc-data uses for an angle's row: `"52"`, or `"52.5"`.
fn angle_key(hundredths: u16) -> String {
    if hundredths.is_multiple_of(100) {
        (hundredths / 100).to_string()
    } else {
        format!("{}", format::from_hundredths(hundredths))
    }
}

/// One per-boat file as a catalogue entry.
fn entry(value: &Value, lists: &YearLists, make_year: Option<i32>) -> Result<Entry, Drop> {
    let sailnumber = value["sailnumber"]
        .as_str()
        .ok_or_else(|| Drop::new("no sail number", "sailnumber is missing"))?;
    let country = text(&value["country"])
        .ok_or_else(|| Drop::new("no country", "country is missing"))?
        .to_uppercase();
    let boat = &value["boat"];
    let sizes = &boat["sizes"];
    let vpp = &value["vpp"];
    if !vpp.is_object() {
        return Err(Drop::new("no VPP", "vpp is missing"));
    }

    let angles = hundredths_list(&vpp["angles"], "angles", 0.01..=180.0)?;
    let speeds = hundredths_list(&vpp["speeds"], "speeds", 0.01..=MAX_SPEED_KN)?;
    increasing(&angles, "angles")?;
    increasing(&speeds, "speeds")?;
    let per_speed = |name: &str, range: std::ops::RangeInclusive<f64>| {
        let list = hundredths_list(&vpp[name], name, range)?;
        if list.len() == speeds.len() {
            Ok(list)
        } else {
            Err(Drop::new(
                "VPP shape mismatch",
                format!(
                    "{name} has {} values for {} wind speeds",
                    list.len(),
                    speeds.len()
                ),
            ))
        }
    };
    let mut bsp = Vec::with_capacity(angles.len());
    for &angle in &angles {
        let key = angle_key(angle);
        let row = vpp[key.as_str()].as_array().ok_or_else(|| {
            Drop::new(
                "VPP shape mismatch",
                format!("no speeds for the angle {key}"),
            )
        })?;
        if row.len() != speeds.len() {
            return Err(Drop::new(
                "VPP shape mismatch",
                format!(
                    "the angle {key} has {} speeds for {} wind speeds",
                    row.len(),
                    speeds.len()
                ),
            ));
        }
        let cells = row
            .iter()
            .map(|cell| {
                if cell.is_null() {
                    return Ok(None);
                }
                let speed = cell.as_f64().ok_or_else(|| {
                    Drop::new("VPP value not a number", format!("angle {key}: {cell}"))
                })?;
                if !(0.0..=MAX_SPEED_KN).contains(&speed) {
                    return Err(Drop::new(
                        "boat speed above 60 kn or negative",
                        format!("angle {key}: {speed} kn"),
                    ));
                }
                hundredths(speed).map(Some).ok_or_else(|| {
                    Drop::new("VPP value finer than 0.01", format!("angle {key}: {speed}"))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        bsp.push(cells);
    }
    if bsp.iter().flatten().all(Option::is_none) {
        return Err(Drop::new("empty VPP", "no boat speed at all"));
    }
    let vpp = Vpp {
        beat_angle: per_speed("beat_angle", 0.01..=89.99)?,
        beat_vmg: per_speed("beat_vmg", 0.01..=MAX_SPEED_KN)?,
        run_angle: per_speed("run_angle", 90.01..=180.0)?,
        run_vmg: per_speed("run_vmg", 0.01..=MAX_SPEED_KN)?,
        angles,
        speeds,
        bsp,
    };

    let size = |key: &str| positive(&sizes[key]);
    Ok(Entry {
        sail_no: sail_display(sailnumber, &country),
        certificate_year: certificate_year(sailnumber, &vpp.speeds, lists, make_year),
        name: text(&value["name"]).unwrap_or_default(),
        model: text(&boat["type"]),
        builder: text(&boat["builder"]),
        designer: text(&boat["designer"]),
        year: boat["year"]
            .as_i64()
            .filter(|year| (1800..=9999).contains(year))
            .and_then(|year| i32::try_from(year).ok()),
        size: [
            size("loa"),
            size("beam"),
            size("draft"),
            size("displacement"),
            size("main"),
            size("genoa"),
            size("spinnaker"),
            size("spinnaker_asym"),
            size("crew"),
        ],
        gph: positive(&value["rating"]["gph"]),
        osn: positive(&value["rating"]["osn"]),
        vpp,
        country,
    })
}

/// Today (UTC) as `YYYY-MM-DD`, or the day of `SOURCE_DATE_EPOCH` when it is
/// set, so a rebuild can be made byte-identical.
fn build_date() -> Result<String, Failure> {
    let seconds = match std::env::var("SOURCE_DATE_EPOCH") {
        Ok(text) => text.trim().parse::<i64>()?,
        Err(_) => i64::try_from(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs(),
        )?,
    };
    Ok(civil_date(seconds.div_euclid(86_400)))
}

/// The proleptic Gregorian date of a day number since 1970-01-01 (Howard
/// Hinnant's `civil_from_days`).
fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// A git checkout, read through the `git` command.
#[derive(Debug)]
struct Git {
    dir: PathBuf,
}

impl Git {
    fn at(dir: &Path) -> Result<Self, Failure> {
        let git = Self {
            dir: dir.to_path_buf(),
        };
        git.text(&["rev-parse", "--git-dir"]).map_err(|e| {
            format!(
                "{} is not inside a git checkout of orc-data ({e})",
                dir.display()
            )
        })?;
        Ok(git)
    }

    fn output(&self, args: &[&str]) -> Result<Vec<u8>, Failure> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(args)
            .output()?;
        if !out.status.success() {
            return Err(format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            )
            .into());
        }
        Ok(out.stdout)
    }

    fn text(&self, args: &[&str]) -> Result<String, Failure> {
        Ok(String::from_utf8(self.output(args)?)?.trim().to_owned())
    }

    /// The blobs under `prefix` (relative to the top of the checkout) at
    /// `HEAD`, as `(path, sha)`, recursively.
    fn tree(&self, prefix: &str) -> Result<Vec<(String, String)>, Failure> {
        let top = self.text(&["rev-parse", "--show-toplevel"])?;
        let out = Command::new("git")
            .arg("-C")
            .arg(&top)
            .args(["ls-tree", "-r", "-z", "HEAD", "--"])
            .arg(if prefix.is_empty() { "." } else { prefix })
            .output()?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).into_owned().into());
        }
        let mut blobs = Vec::new();
        for record in out.stdout.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let record = String::from_utf8_lossy(record);
            let Some((meta, path)) = record.split_once('\t') else {
                continue;
            };
            let mut meta = meta.split(' ');
            if let (Some(_mode), Some("blob"), Some(sha)) = (meta.next(), meta.next(), meta.next())
            {
                blobs.push((path.to_owned(), sha.to_owned()));
            }
        }
        Ok(blobs)
    }

    /// The contents of `shas`, in order, through one `git cat-file --batch`.
    fn blobs(&self, shas: &[String]) -> Result<Vec<Vec<u8>>, Failure> {
        let mut child = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;
        let mut stdin = child.stdin.take().ok_or("git cat-file has no stdin")?;
        let input: String = shas.iter().map(|sha| format!("{sha}\n")).collect();
        // Written from a thread: git answers while it reads, and a full
        // stdout pipe would otherwise stop both sides.
        let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));
        let mut out = Vec::new();
        child
            .stdout
            .take()
            .ok_or("git cat-file has no stdout")?
            .read_to_end(&mut out)?;
        writer
            .join()
            .map_err(|_| "the git writer thread panicked")??;
        if !child.wait()?.success() {
            return Err("git cat-file failed".into());
        }

        let mut blobs = Vec::with_capacity(shas.len());
        let mut rest = out.as_slice();
        for sha in shas {
            let end = rest
                .iter()
                .position(|b| *b == b'\n')
                .ok_or("git cat-file output ends early")?;
            let header = String::from_utf8_lossy(&rest[..end]).into_owned();
            let size: usize = match header.split(' ').collect::<Vec<_>>().as_slice() {
                [_, "blob", size] => size.parse()?,
                _ => return Err(format!("git cat-file could not read {sha}: {header}").into()),
            };
            let body = rest
                .get(end + 1..end + 1 + size)
                .ok_or("git cat-file output ends early")?;
            blobs.push(body.to_vec());
            rest = rest.get(end + 2 + size..).unwrap_or_default();
        }
        Ok(blobs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A per-boat file as orc-data writes it (abridged from GBR/_1.json).
    fn file() -> Value {
        serde_json::json!({
            "sailnumber": "GBR/GBR1124",
            "country": "GBR",
            "name": " Jiminy ",
            "rating": {"gph": 461.2, "osn": 445.6},
            "boat": {
                "builder": "BALTIC YACHTS", "type": "BALTIC 87", "designer": "",
                "year": 1996,
                "sizes": {"loa": 26.5, "beam": 6.23, "draft": 4.72, "displacement": 58005.0,
                          "genoa": 148.42, "main": 187.26, "spinnaker": 0.0,
                          "spinnaker_asym": 437.69, "crew": 1000.0, "wetted_surface": 122.32}
            },
            "vpp": {
                "angles": [52, 90],
                "speeds": [6, 8, 10, 12, 14, 16, 20],
                "52": [7.07, 8.57, 9.58, 10.18, 10.5, 10.7, 10.93],
                "90": [7.59, 9.07, 10.11, 10.78, 11.2, 11.52, 12.02],
                "beat_angle": [43.6, 41.8, 40, 39, 38.4, 38.1, 37.2],
                "beat_vmg": [4.6, 5.71, 6.5, 7.0, 7.3, 7.5, 7.75],
                "run_angle": [140.8, 143, 146, 148.7, 149.6, 150.3, 170],
                "run_vmg": [4.41, 5.72, 6.87, 7.88, 8.72, 9.32, 10.19]
            }
        })
    }

    fn lists() -> YearLists {
        let mut lists = YearLists::default();
        lists.add(2021, "[{'sailnumber': 'GBR/GBR1124', 'name': 'x'}]");
        lists.add(
            2022,
            "[{'sailnumber': \"GBR/O'NEIL\"}, {'sailnumber': 'GBR/GBR1124'}]",
        );
        lists.add(2023, "[{'sailnumber': 'GBR/9'}]");
        lists.add(2025, "[{'sailnumber': 'GBR/9'}]");
        lists
    }

    #[test]
    fn a_file_becomes_an_entry() {
        let entry = entry(&file(), &lists(), Some(2025)).unwrap();
        assert_eq!(entry.sail_no, "GBR 1124");
        assert_eq!(entry.name, "Jiminy");
        assert_eq!(entry.model.as_deref(), Some("BALTIC 87"));
        assert_eq!(entry.designer, None);
        assert_eq!(entry.year, Some(1996));
        assert_eq!(entry.certificate_year, Some(2022));
        assert_eq!(entry.size[0], Some(26.5));
        assert_eq!(entry.size[6], None, "a spinnaker of 0 is no spinnaker");
        assert_eq!(entry.gph, Some(461.2));
        assert_eq!(entry.vpp.angles, vec![5200, 9000]);
        assert_eq!(entry.vpp.bsp[0][3], Some(1018));
        assert_eq!(entry.vpp.beat_angle[0], 4360);
        assert_eq!(entry.vpp.run_angle[6], 17000);
    }

    #[test]
    fn broken_files_are_dropped_with_a_reason() {
        let lists = lists();
        let reason = |edit: &dyn Fn(&mut Value)| {
            let mut value = file();
            edit(&mut value);
            entry(&value, &lists, None).unwrap_err().reason
        };
        assert_eq!(
            reason(&|v| v["vpp"]["52"] = serde_json::json!([1.0])),
            "VPP shape mismatch"
        );
        assert_eq!(
            reason(&|v| v["vpp"]["90"][0] = serde_json::json!(155.84)),
            "boat speed above 60 kn or negative"
        );
        assert_eq!(
            reason(&|v| v["vpp"]["90"][0] = serde_json::json!(7.591)),
            "VPP value finer than 0.01"
        );
        assert_eq!(
            reason(&|v| v["vpp"]["90"][0] = serde_json::json!("fast")),
            "VPP value not a number"
        );
        assert_eq!(
            reason(&|v| v["vpp"]["speeds"] = serde_json::json!([8, 6, 10, 12, 14, 16, 20])),
            "VPP axis not increasing"
        );
        assert_eq!(
            reason(&|v| {
                v["vpp"].as_object_mut().unwrap().remove("beat_vmg");
            }),
            "VPP field missing"
        );
        assert_eq!(
            reason(&|v| {
                v.as_object_mut().unwrap().remove("vpp");
            }),
            "no VPP"
        );
        assert_eq!(
            reason(&|v| {
                v.as_object_mut().unwrap().remove("country");
            }),
            "no country"
        );
        assert_eq!(
            reason(&|v| v["vpp"]["run_angle"][0] = serde_json::json!(80)),
            "VPP value out of range"
        );
    }

    #[test]
    fn the_certificate_year_follows_the_axis_and_the_lists() {
        let lists = lists();
        let seven = [600, 800, 1000, 1200, 1400, 1600, 2000];
        let eight = [600, 800, 1000, 1200, 1400, 1600, 2000, 2400];
        let nine = [400, 600, 800, 1000, 1200, 1400, 1600, 2000, 2400];
        assert_eq!(
            certificate_year("GBR/GBR1124", &seven, &lists, Some(2025)),
            Some(2022)
        );
        assert_eq!(
            certificate_year("GBR/9", &seven, &lists, Some(2025)),
            Some(2023)
        );
        assert_eq!(
            certificate_year("GBR/unknown", &seven, &lists, Some(2025)),
            None
        );
        assert_eq!(
            certificate_year("GBR/unknown", &eight, &lists, None),
            Some(2024)
        );
        assert_eq!(
            certificate_year("GBR/unknown", &nine, &lists, Some(2025)),
            Some(2025)
        );
        assert_eq!(certificate_year("GBR/unknown", &nine, &lists, None), None);
        assert_eq!(certificate_year("GBR/9", &nine, &lists, None), Some(2025));
        assert_eq!(
            certificate_year("GBR/9", &[500, 900], &lists, Some(2025)),
            None
        );
        assert!(lists.years[&2022].contains("GBR/O'NEIL"));
    }

    #[test]
    fn sail_numbers_are_shown_once_with_their_country() {
        assert_eq!(sail_display("GBR/GBR1124", "GBR"), "GBR 1124");
        assert_eq!(sail_display("GBR/1124", "GBR"), "GBR 1124");
        assert_eq!(sail_display("AUS/Sm35", "AUS"), "AUS Sm35");
        assert_eq!(sail_display("FIN/Fin71", "FIN"), "FIN 71");
        assert_eq!(sail_display("GBR/_1", "GBR"), "");
        assert_eq!(sail_display("GBR/GBR", "GBR"), "GBR GBR");
    }

    #[test]
    fn the_makefile_year_and_the_build_date_read_correctly() {
        assert_eq!(makefile_year("URL = x\nYEAR = 2025\n"), Some(2025));
        assert_eq!(makefile_year("VPPYEAR = 1\n"), None);
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(20_454), "2026-01-01");
        assert_eq!(civil_date(11_016), "2000-02-29");
        assert_eq!(civil_date(-1), "1969-12-31");
    }
}
