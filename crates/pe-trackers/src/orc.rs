//! ORC's public data service: every valid certificate of a country, with its
//! speed table (spec.md 5.4).
//!
//! `data.orc.org` answers one JSON document per country, holding that
//! country's certificates for one VPP year. Only the current year is served:
//! any other year answers no certificates. The embedded catalogue comes from
//! the same service by way of jieter/orc-data, so a certificate is read here
//! the way that repository's `format_data` and this project's catalogue
//! builder read it between them, and the same certificate comes out the same
//! from either road.
//!
//! Nothing downloaded is kept: the caller stores the records it is handed.

use pe_core::orc::{OrcRecord, OrcSize, OrcVpp, sail_display};
use serde_json::Value;

use crate::{Fetcher, TrackerError, error::Result};

/// The service's certificate download, ORC family, as JSON.
pub const SERVICE_URL: &str =
    "https://data.orc.org/public/WPub.dll?action=DownRMS&ext=json&Family=1";

/// A country code that is none of the rating offices': the service answers
/// it with no certificates and, as always, the list of countries it serves.
const NO_COUNTRY: &str = "ORC";

/// Anything faster is not a boat speed (as the catalogue builder holds).
const MAX_SPEED_KN: f64 = 60.0;

/// One country's document.
#[derive(Debug, Clone, PartialEq)]
pub struct CountryFile {
    /// The certificates that could be read.
    pub records: Vec<OrcRecord>,
    /// The ones that could not: the certificate's reference (or its place in
    /// the list) and why.
    pub dropped: Vec<(String, String)>,
    /// The country codes the service serves, as this document lists them.
    pub countries: Vec<String>,
    /// The reference number of every certificate on the list, readable or
    /// not: what the country holds valid, for the caller to tell which of
    /// the certificates it kept are no longer among them.
    pub listed: Vec<String>,
}

/// What a scrape brought back.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Harvest {
    /// Every certificate that could be read.
    pub records: Vec<OrcRecord>,
    /// For each country whose list was read whole: every reference on it.
    /// A country that could not be fetched is not here.
    pub listed: std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
}

fn error(at: impl Into<String>, why: impl Into<String>) -> TrackerError {
    TrackerError::Decode {
        what: "ORC certificates",
        at: at.into(),
        why: why.into(),
    }
}

/// The address of a country's certificates for a VPP year.
pub fn country_url(country: &str, year: i32) -> Result<String> {
    if !(2000..=2100).contains(&year) {
        return Err(error("year", format!("{year} is not a certificate year")));
    }
    // The code goes into a query: three characters of the kinds the
    // service's own list uses, nothing that could add a parameter.
    if country.len() != 3
        || !country
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(error(
            "country",
            format!("{country:?} is not a country code"),
        ));
    }
    Ok(format!("{SERVICE_URL}&VPPYear={year}&CountryId={country}"))
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

/// A size or rating: a positive number, else nothing (a boat with no
/// spinnaker has `null` or 0 there).
fn positive(value: &Value) -> Option<f64> {
    value.as_f64().filter(|v| v.is_finite() && *v > 0.0)
}

fn to_hundredths(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// A list of numbers, one per wind speed or angle, each inside `range`.
fn numbers(
    allowances: &Value,
    name: &str,
    range: std::ops::RangeInclusive<f64>,
) -> std::result::Result<Vec<f64>, String> {
    let list = allowances[name]
        .as_array()
        .ok_or_else(|| format!("no {name} list"))?;
    list.iter()
        .map(|item| {
            let number = item
                .as_f64()
                .filter(|n| n.is_finite())
                .ok_or_else(|| format!("{name}: {item} is not a number"))?;
            let number = to_hundredths(number);
            if range.contains(&number) {
                Ok(number)
            } else {
                Err(format!("{name}: {number} is out of range"))
            }
        })
        .collect()
}

/// A time allowance, seconds per nautical mile, as a speed in knots to two
/// decimals: what a certificate's table holds, turned the way it is sailed.
fn speed(allowance: &Value, what: &str) -> std::result::Result<f64, String> {
    let seconds = allowance
        .as_f64()
        .filter(|s| s.is_finite() && *s > 0.0)
        .ok_or_else(|| format!("{what}: {allowance} is not a time allowance"))?;
    let knots = to_hundredths(3600.0 / seconds);
    if knots > MAX_SPEED_KN {
        return Err(format!("{what}: {knots} kn is not a boat speed"));
    }
    Ok(knots)
}

fn speeds(allowances: &Value, name: &str, count: usize) -> std::result::Result<Vec<f64>, String> {
    let list = allowances[name]
        .as_array()
        .ok_or_else(|| format!("no {name} list"))?;
    if list.len() != count {
        return Err(format!(
            "{name} has {} values for {count} wind speeds",
            list.len()
        ));
    }
    list.iter().map(|item| speed(item, name)).collect()
}

fn increasing(values: &[f64], name: &str) -> std::result::Result<(), String> {
    if values.is_empty() {
        return Err(format!("{name} is empty"));
    }
    if values.windows(2).all(|pair| pair[0] < pair[1]) {
        Ok(())
    } else {
        Err(format!("{name} is not increasing"))
    }
}

/// The key of an angle's row of allowances: `R52`, `R110`.
fn row_key(angle: f64) -> String {
    if angle.fract() == 0.0 {
        format!("R{angle:.0}")
    } else {
        format!("R{angle}")
    }
}

/// One certificate of a country's list as a record, or why it cannot be one.
fn record(value: &Value, country: &str, year: i32) -> std::result::Result<OrcRecord, String> {
    let ref_no = text(&value["RefNo"]).ok_or("it has no reference number")?;
    let allowances = &value["Allowances"];
    if !allowances.is_object() {
        return Err("it has no allowances".to_owned());
    }
    let angles = numbers(allowances, "WindAngles", 0.01..=180.0)?;
    let wind = numbers(allowances, "WindSpeeds", 0.01..=MAX_SPEED_KN)?;
    increasing(&angles, "WindAngles")?;
    increasing(&wind, "WindSpeeds")?;
    let per_speed = |name: &str, range: std::ops::RangeInclusive<f64>| {
        let list = numbers(allowances, name, range)?;
        if list.len() == wind.len() {
            Ok(list)
        } else {
            Err(format!(
                "{name} has {} values for {} wind speeds",
                list.len(),
                wind.len()
            ))
        }
    };
    let bsp = angles
        .iter()
        .map(|angle| {
            speeds(allowances, &row_key(*angle), wind.len())
                .map(|row| row.into_iter().map(Some).collect())
        })
        .collect::<std::result::Result<Vec<Vec<Option<f64>>>, String>>()?;
    let vpp = OrcVpp {
        beat_angle: per_speed("BeatAngle", 0.01..=89.99)?,
        beat_vmg: speeds(allowances, "Beat", wind.len())?,
        run_angle: per_speed("GybeAngle", 90.01..=180.0)?,
        run_vmg: speeds(allowances, "Run", wind.len())?,
        angles,
        speeds: wind,
        bsp,
    };
    // The number with its separators removed, as orc-data keys its files,
    // then shown the one way the catalogue shows a sail number.
    let sail: String = value["SailNo"]
        .as_str()
        .unwrap_or_default()
        .chars()
        .filter(|c| !matches!(c, ' ' | '-' | '/'))
        .collect();
    let rounded = |key: &str| positive(&value[key]).map(to_hundredths);
    Ok(OrcRecord {
        ref_no: Some(ref_no),
        sail_no: sail_display(&sail, country),
        country: country.to_owned(),
        name: text(&value["YachtName"]).unwrap_or_default(),
        model: text(&value["Class"]),
        builder: text(&value["Builder"]),
        designer: text(&value["Designer"]),
        year: value["Age_Year"]
            .as_i64()
            .filter(|year| (1800..=9999).contains(year))
            .and_then(|year| i32::try_from(year).ok()),
        certificate_year: Some(year),
        size: OrcSize {
            loa: positive(&value["LOA"]),
            beam: rounded("MB"),
            draft: rounded("Draft"),
            displacement_kg: positive(&value["Dspl_Measurement"]),
            main_area: positive(&value["Area_Main"]),
            genoa_area: positive(&value["Area_Jib"]),
            spinnaker_area: positive(&value["Area_Sym"]),
            asym_spinnaker_area: positive(&value["Area_Asym"])
                .or_else(|| positive(&value["Area_ASym"])),
            crew_kg: positive(&value["CrewWT"]),
        },
        gph: positive(&value["GPH"]),
        osn: positive(&value["OSN"]),
        vpp,
    })
}

/// Reads one country's document. A certificate that cannot be read is
/// reported in `dropped` and the rest are kept; a document that is not the
/// service's at all is an error naming where it stops making sense.
pub fn parse_country(bytes: &[u8], country: &str, year: i32) -> Result<CountryFile> {
    // The service sends a byte-order mark, which JSON does not allow.
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let document: Value = serde_json::from_slice(bytes).map_err(|e| {
        error(
            format!("{country} line {} column {}", e.line(), e.column()),
            e.to_string(),
        )
    })?;
    let list = document["rms"]
        .as_array()
        .ok_or_else(|| error(country, "the document has no list of certificates"))?;
    let mut records = Vec::with_capacity(list.len());
    let mut dropped = Vec::new();
    let listed = list
        .iter()
        .filter_map(|value| text(&value["RefNo"]))
        .collect();
    for (place, value) in list.iter().enumerate() {
        match record(value, country, year) {
            Ok(record) => records.push(record),
            Err(why) => dropped.push((
                text(&value["RefNo"]).unwrap_or_else(|| format!("{country} #{}", place + 1)),
                why,
            )),
        }
    }
    let countries = document["Countries"]
        .as_array()
        .map(|countries| {
            countries
                .iter()
                .filter_map(|entry| text(&entry["CountryId"]))
                .collect()
        })
        .unwrap_or_default();
    Ok(CountryFile {
        records,
        dropped,
        countries,
        listed,
    })
}

/// Downloads every country's valid certificates for `year`, one country
/// after another to keep the load on the public service low.
///
/// `progress` hears the countries done, the countries there are and the
/// certificates read so far; `failure` hears each certificate left out and
/// each country that could not be fetched. A country that fails does not
/// stop the others. The caller stores the result only once it is whole.
pub fn scrape(
    fetcher: &Fetcher,
    year: i32,
    progress: &mut dyn FnMut(usize, usize, usize),
    failure: &mut dyn FnMut(&str, &str),
) -> Result<Harvest> {
    let listing = fetcher.get(&country_url(NO_COUNTRY, year)?, &mut |_, _| {})?;
    let countries: Vec<String> = parse_country(&listing, NO_COUNTRY, year)?
        .countries
        .into_iter()
        .filter(|country| country != NO_COUNTRY && country_url(country, year).is_ok())
        .collect();
    if countries.is_empty() {
        return Err(error("country list", "the service lists no countries"));
    }
    let mut harvest = Harvest::default();
    progress(0, countries.len(), 0);
    for (done, country) in countries.iter().enumerate() {
        fetcher.check()?;
        let fetched = fetcher
            .get(&country_url(country, year)?, &mut |_, _| {})
            .and_then(|bytes| parse_country(&bytes, country, year));
        match fetched {
            Ok(file) => {
                for (certificate, why) in &file.dropped {
                    failure(certificate, why);
                }
                harvest.records.extend(file.records);
                harvest
                    .listed
                    .insert(country.clone(), file.listed.into_iter().collect());
            }
            Err(TrackerError::Cancelled) => return Err(TrackerError::Cancelled),
            Err(err) => failure(country, &err.to_string()),
        }
        progress(done + 1, countries.len(), harvest.records.len());
    }
    fetcher.check()?;
    Ok(harvest)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recorded answer of the service (Norway, 14 October 2026), cut to
    /// three certificates and two of its scoring options.
    const NORWAY: &[u8] = include_bytes!("../tests/fixtures/orc/NOR2026.json");

    #[test]
    fn a_country_document_becomes_records_with_speeds_from_allowances() {
        let file = parse_country(NORWAY, "NOR", 2026).unwrap();
        assert_eq!(file.dropped, vec![]);
        assert_eq!(file.records.len(), 3);
        assert_eq!(file.countries.len(), 34);
        assert_eq!(file.listed, ["03440004L2V", "03440004L2W", "03440004L3M"]);
        assert!(file.countries.contains(&"ITA".to_owned()));

        let boat = &file.records[0];
        assert_eq!(boat.ref_no.as_deref(), Some("03440004L2V"));
        assert_eq!(boat.sail_no, "NOR 14438");
        assert_eq!(boat.country, "NOR");
        assert_eq!(boat.name, "Momentum");
        assert_eq!(boat.model.as_deref(), Some("Elan 350"));
        assert_eq!(boat.builder.as_deref(), Some("Elan Marine"));
        assert_eq!(boat.designer.as_deref(), Some("RobHumpreys"));
        assert_eq!(boat.year, Some(2009));
        assert_eq!(boat.certificate_year, Some(2026));
        assert_eq!(boat.gph, Some(605.7));
        assert_eq!(boat.osn, Some(589.4));
        // Beam and draft to two decimals, as orc-data keeps them (2.318 m).
        assert_eq!(boat.size.loa, Some(10.588));
        assert_eq!(boat.size.beam, Some(3.54));
        assert_eq!(boat.size.draft, Some(2.32));
        assert_eq!(boat.size.displacement_kg, Some(4928.0));
        assert_eq!(boat.size.main_area, Some(38.37));
        assert_eq!(boat.size.genoa_area, Some(31.94));
        assert_eq!(boat.size.spinnaker_area, None);
        assert_eq!(boat.size.asym_spinnaker_area, Some(113.86));
        assert_eq!(boat.size.crew_kg, Some(510.0));

        let vpp = &boat.vpp;
        assert_eq!(
            vpp.speeds,
            vec![4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 20.0, 24.0]
        );
        assert_eq!(
            vpp.angles,
            vec![52.0, 60.0, 75.0, 90.0, 110.0, 120.0, 135.0, 150.0]
        );
        // By hand, from the certificate's seconds per mile: 3600 / 922.2 =
        // 3.9037, 3600 / 670.2 = 5.3715 (52° in 4 and 6 kn); 3600 / 323.1 =
        // 11.142 (150° in 24 kn).
        assert_eq!(vpp.bsp[0][0], Some(3.9));
        assert_eq!(vpp.bsp[0][1], Some(5.37));
        assert_eq!(vpp.bsp[7][8], Some(11.14));
        // 3600 / 1447.3 = 2.4874 upwind, 3600 / 1348.4 = 2.6698 downwind.
        assert_eq!(vpp.beat_vmg[0], 2.49);
        assert_eq!(vpp.run_vmg[0], 2.67);
        assert_eq!(vpp.beat_angle[..2], [46.0, 42.6]);
        assert_eq!(vpp.run_angle[..2], [139.2, 142.8]);
        assert!(vpp.bsp.iter().all(|row| row.len() == 9));

        // A boat with a symmetric spinnaker has its area.
        assert_eq!(file.records[1].name, "Lazy");
        assert_eq!(file.records[1].size.spinnaker_area, Some(76.02));
    }

    #[test]
    fn a_certificate_that_cannot_be_read_is_left_out_and_named() {
        let text = String::from_utf8_lossy(NORWAY.strip_prefix(b"\xef\xbb\xbf").unwrap());
        let mut document: Value = serde_json::from_str(&text).unwrap();
        // The first has a table with a row short, the second no reference,
        // the third an allowance of zero: none can be a polar.
        document["rms"][0]["Allowances"]["R90"]
            .as_array_mut()
            .unwrap()
            .pop();
        document["rms"][1]["RefNo"] = Value::Null;
        document["rms"][2]["Allowances"]["Beat"][0] = serde_json::json!(0);
        let bytes = serde_json::to_vec(&document).unwrap();
        let file = parse_country(&bytes, "NOR", 2026).unwrap();
        assert_eq!(file.records, vec![]);
        assert_eq!(file.dropped.len(), 3);
        // Still on the list, readable or not: they are not taken for gone.
        assert_eq!(file.listed, ["03440004L2V", "03440004L3M"]);
        assert_eq!(file.dropped[0].0, "03440004L2V");
        assert!(file.dropped[0].1.contains("R90"), "{:?}", file.dropped[0]);
        assert_eq!(file.dropped[1].0, "NOR #2");
        assert!(file.dropped[1].1.contains("reference"));
        assert!(file.dropped[2].1.contains("Beat"), "{:?}", file.dropped[2]);
    }

    #[test]
    fn a_document_that_is_not_the_services_is_an_error_naming_the_place() {
        let err = parse_country(b"{\"rms\": [\n  {\"RefNo\": ", "NOR", 2026).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("line 2"), "{message}");
        assert!(message.contains("column"), "{message}");
        assert!(parse_country(b"", "NOR", 2026).is_err());
        assert!(parse_country(b"<html>busy</html>", "NOR", 2026).is_err());
        let err = parse_country(b"{\"other\": 1}", "NOR", 2026).unwrap_err();
        assert!(err.to_string().contains("no list of certificates"));
        // No certificates is an answer, not an error: any year but the
        // current one is answered so.
        let empty = parse_country(b"{\"rms\": [], \"Countries\": []}", "NOR", 2025).unwrap();
        assert_eq!(empty.records, vec![]);
    }

    #[test]
    fn only_a_country_code_and_a_year_reach_the_address() {
        assert_eq!(
            country_url("NOR", 2026).unwrap(),
            "https://data.orc.org/public/WPub.dll?action=DownRMS&ext=json&Family=1&VPPYear=2026&CountryId=NOR"
        );
        assert!(country_url("SY_", 2026).is_ok());
        for bad in ["", "NO", "NORW", "N&x", "nor", "N R", "A=1"] {
            assert!(country_url(bad, 2026).is_err(), "{bad:?}");
        }
        assert!(country_url("NOR", 1999).is_err());
        assert!(crate::net::allowed_host("data.orc.org"));
    }
}
