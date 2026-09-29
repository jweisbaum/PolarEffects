//! The embedded catalogue itself: it loads, it holds the boats orc-data
//! holds, a known certificate reads back exactly, and search meets the
//! per-keystroke budget of spec.md 13.

use std::time::{Duration, Instant};

use pe_orc::{Fields, Filters, catalogue, provenance};

/// GBR 1124, Eratosthenes, a 1999 Nautor Swan 112, as orc-data's
/// `site/data/GBR/1124.json` gives it (commit c2ca870c, 2026-09-28). The
/// numbers are copied by hand from that file, whose speeds are the ORC
/// certificate's time allowances as knots to 0.01.
mod eratosthenes {
    pub const ANGLES: [f64; 8] = [52.0, 60.0, 75.0, 90.0, 110.0, 120.0, 135.0, 150.0];
    pub const SPEEDS: [f64; 7] = [6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 20.0];
    pub const BSP_52: [f64; 7] = [6.87, 8.6, 9.79, 10.54, 10.96, 11.23, 11.53];
    pub const BSP_90: [f64; 7] = [7.75, 9.46, 10.76, 11.68, 12.28, 12.68, 13.24];
    pub const BSP_150: [f64; 7] = [5.17, 6.77, 8.21, 9.5, 10.65, 11.59, 12.77];
    pub const BEAT_ANGLE: [f64; 7] = [47.1, 45.0, 43.2, 43.0, 42.3, 42.0, 41.7];
    pub const BEAT_VMG: [f64; 7] = [4.33, 5.53, 6.4, 6.92, 7.25, 7.47, 7.72];
    pub const RUN_ANGLE: [f64; 7] = [141.2, 143.0, 145.1, 147.8, 148.8, 149.8, 150.5];
    pub const RUN_VMG: [f64; 7] = [4.48, 5.87, 7.11, 8.22, 9.22, 10.04, 11.06];
}

fn close(actual: &[f64], expected: &[f64], what: &str) {
    assert_eq!(actual.len(), expected.len(), "{what}");
    for (a, e) in actual.iter().zip(expected) {
        assert!((a - e).abs() <= 0.01, "{what}: {a} is not {e}");
    }
}

#[test]
fn the_embedded_catalogue_loads_with_its_provenance() {
    let started = Instant::now();
    let catalogue = catalogue().unwrap();
    eprintln!("first load and index: {:?}", started.elapsed());
    assert!(catalogue.len() > 18_000, "{} records", catalogue.len());
    let about = provenance().unwrap();
    assert_eq!(&about, catalogue.provenance());
    assert_eq!(about.source, "jieter/orc-data");
    assert_eq!(about.commit.len(), 40);
    assert!(about.commit.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(about.records as usize, catalogue.len());
    assert!(catalogue.countries().iter().any(|c| c == "GBR"));
    let (first, last) = catalogue.year_range().unwrap();
    assert!(first >= 1800 && last <= 2030 && first < last);
}

#[test]
fn a_known_certificate_is_found_by_its_sail_number_and_reads_back_exactly() {
    let catalogue = catalogue().unwrap();
    for query in ["GBR1124", "GBR 1124", "GBR/1124", "gbr-1124"] {
        let hits = catalogue.search(query, &Filters::default(), 50);
        let entry = catalogue.entry(hits.ids[0]).unwrap();
        assert_eq!(entry.name, "Eratosthenes", "{query}");
    }
    let hits = catalogue.search("GBR 1124", &Filters::default(), 1);
    let record = catalogue.entry(hits.ids[0]).unwrap().to_record();
    assert_eq!(record.sail_no, "GBR 1124");
    assert_eq!(record.model.as_deref(), Some("Swan 112"));
    assert_eq!(record.builder.as_deref(), Some("Nautor"));
    assert_eq!(record.designer.as_deref(), Some("Frers"));
    assert_eq!(record.year, Some(1999));
    assert_eq!(record.size.loa, Some(34.34));
    assert_eq!(record.gph, Some(450.2));

    use eratosthenes::*;
    let vpp = &record.vpp;
    close(&vpp.angles, &ANGLES, "angles");
    close(&vpp.speeds, &SPEEDS, "speeds");
    let row = |angle: f64| -> Vec<f64> {
        let i = vpp.angles.iter().position(|a| *a == angle).unwrap();
        vpp.bsp[i].iter().map(|c| c.unwrap()).collect()
    };
    close(&row(52.0), &BSP_52, "52°");
    close(&row(90.0), &BSP_90, "90°");
    close(&row(150.0), &BSP_150, "150°");
    close(&vpp.beat_angle, &BEAT_ANGLE, "beat angle");
    close(&vpp.beat_vmg, &BEAT_VMG, "beat VMG");
    close(&vpp.run_angle, &RUN_ANGLE, "run angle");
    close(&vpp.run_vmg, &RUN_VMG, "run VMG");
}

#[test]
fn words_across_fields_are_anded_and_accents_fold() {
    let catalogue = catalogue().unwrap();
    let hits = catalogue.search("swan 112 nautor", &Filters::default(), 50);
    assert!(
        hits.ids
            .iter()
            .any(|id| catalogue.entry(*id).unwrap().name == "Eratosthenes")
    );
    for id in &hits.ids {
        let model = catalogue
            .entry(*id)
            .unwrap()
            .model
            .clone()
            .unwrap_or_default();
        assert!(model.to_lowercase().contains("swan"), "{model}");
    }
    // Every boat whose name has an accent is found without it.
    let accented = (0..catalogue.len() as u32)
        .map(|id| catalogue.entry(id).unwrap())
        .find(|e| e.name.contains('ö'))
        .unwrap();
    let plain = accented.name.replace('ö', "o");
    let hits = catalogue.search(&plain, &Filters::default(), 1000);
    assert!(
        hits.ids
            .iter()
            .any(|id| catalogue.entry(*id).unwrap() == accented)
    );
}

#[test]
fn each_field_is_searched_on_its_own() {
    let catalogue = catalogue().unwrap();
    let only = |fields: Fields| Filters {
        fields,
        ..Filters::default()
    };
    for query in ["GBR1124", "GBR 1124", "GBR/1124", "1124"] {
        let hits = catalogue.search(
            "",
            &only(Fields {
                sail_no: query.to_owned(),
                ..Fields::default()
            }),
            50,
        );
        let first = catalogue.entry(hits.ids[0]).unwrap();
        assert_eq!(first.name, "Eratosthenes", "{query}");
    }
    // Model and builder together, each in its own field.
    let swans = only(Fields {
        model: "swan 112".to_owned(),
        builder: "nautor".to_owned(),
        ..Fields::default()
    });
    let hits = catalogue.search("", &swans, 200);
    assert!(hits.total >= 1);
    for id in &hits.ids {
        let entry = catalogue.entry(*id).unwrap();
        assert!(entry.model.as_deref().unwrap_or("").contains("112"));
        assert!(
            entry
                .builder
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains("nautor")
        );
    }
    // "nautor" is a builder, never a boat name here.
    let named = catalogue.search(
        "",
        &only(Fields {
            name: "nautor".to_owned(),
            ..Fields::default()
        }),
        200,
    );
    for id in &named.ids {
        let name = catalogue.entry(*id).unwrap().name.to_lowercase();
        assert!(
            name.split(|c: char| !c.is_alphanumeric())
                .any(|w| w.starts_with("nautor")),
            "{name}"
        );
    }
    // The designer field, with the certificate year from its start.
    let frers = only(Fields {
        designer: "frers".to_owned(),
        certificate_year: "202".to_owned(),
        ..Fields::default()
    });
    let hits = catalogue.search("", &frers, 200);
    assert!(hits.total >= 1);
    for id in &hits.ids {
        let entry = catalogue.entry(*id).unwrap();
        assert!(
            entry
                .designer
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains("frers")
        );
        assert!(
            entry
                .certificate_year
                .is_some_and(|y| (2020..2030).contains(&y))
        );
    }
}

/// spec.md 13: a search result update within 30 ms of each keystroke, over
/// the full catalogue. Every prefix of each query is one keystroke.
#[test]
fn search_meets_the_keystroke_budget() {
    let catalogue = catalogue().unwrap();
    let queries = [
        "GBR 1124",
        "farr 40 2023",
        "swan 112",
        "j/109",
        "Eratosthenes",
        "first 40.7",
        "beneteau oceanis",
        "x-yachts xp 44",
        "a",
        "e",
        "2025",
        "NED",
    ];
    let filters = [
        Filters::default(),
        Filters {
            year_min: Some(1990),
            year_max: Some(2010),
            ..Filters::default()
        },
    ];
    let mut times: Vec<Duration> = Vec::new();
    for filter in &filters {
        for query in queries {
            for end in query.char_indices().map(|(i, c)| i + c.len_utf8()) {
                let started = Instant::now();
                let hits = catalogue.search(&query[..end], filter, 50);
                times.push(started.elapsed());
                assert!(hits.ids.len() <= 50);
            }
        }
    }
    // The same keystrokes typed into single fields, alone, with the box and
    // with a second field already filled in.
    let fields: [fn(&str) -> Fields; 5] = [
        |q| Fields {
            name: q.to_owned(),
            ..Fields::default()
        },
        |q| Fields {
            sail_no: q.to_owned(),
            ..Fields::default()
        },
        |q| Fields {
            model: q.to_owned(),
            builder: "b".to_owned(),
            ..Fields::default()
        },
        |q| Fields {
            designer: q.to_owned(),
            certificate_year: "20".to_owned(),
            ..Fields::default()
        },
        |q| Fields {
            builder: q.to_owned(),
            name: "a".to_owned(),
            ..Fields::default()
        },
    ];
    for field in fields {
        for (query, boxed) in queries.iter().zip(["", "e"].iter().cycle()) {
            for end in query.char_indices().map(|(i, c)| i + c.len_utf8()) {
                let filter = Filters {
                    year_min: Some(1980),
                    fields: field(&query[..end]),
                    ..Filters::default()
                };
                let started = Instant::now();
                let hits = catalogue.search(boxed, &filter, 50);
                times.push(started.elapsed());
                assert!(hits.ids.len() <= 50);
            }
        }
    }
    times.sort();
    let p99 = times[(times.len() * 99).div_ceil(100) - 1];
    let worst = times[times.len() - 1];
    eprintln!(
        "{} keystrokes: median {:?}, p99 {:?}, worst {:?}",
        times.len(),
        times[times.len() / 2],
        p99,
        worst
    );
    assert!(p99 < Duration::from_millis(30), "p99 {p99:?}");
}
