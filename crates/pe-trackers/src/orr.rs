//! RegattaMan's public ORR valid list, complete certificates and ratings.
//! Embedded public data is decoded without running page scripts or logging in.

mod details;

use std::collections::BTreeMap;

use dom_query::{Document, Selection};
use pe_core::{orc::OrcSize, orr::OrrRecord, polar::PolarGrid};

use crate::{Fetcher, TrackerError, error::Result};

/// The catalogue the user explicitly requested.
pub const CATALOGUE_URL: &str =
    "https://www.regattaman.com/valid_list_ora.php?crule=ORR&sdir=true&ssdir=true&sort=3&ssort=0";
const CERTIFICATE_URL: &str = "https://www.regattaman.com/cert_form.php?sku=";

/// Public boat/certificate identity and every named valid-list column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Certificate {
    /// Validated SKU, safe to append to the fixed certificate URL.
    pub sku: String,
    /// Printed certificate number.
    pub certificate: String,
    /// Certificate year selected in the list.
    pub year: i32,
    /// Boat name.
    pub name: String,
    /// Sail number.
    pub sail_no: String,
    /// Model/type.
    pub model: Option<String>,
    /// Original named columns, including ratings and certificate dates.
    pub fields: BTreeMap<String, String>,
}

fn error(at: impl Into<String>, why: impl Into<String>) -> TrackerError {
    TrackerError::Decode {
        what: "ORR certificate",
        at: at.into(),
        why: why.into(),
    }
}

fn normalized(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Decode the valid list with its column names, rather than relying on column
/// order. Duplicate links (including repeated table headers) are harmless.
pub fn parse_list(html: &str, year: i32) -> Result<Vec<Certificate>> {
    let document = Document::from(html);
    let mut records = BTreeMap::new();
    for table in document.select("table.restable").iter() {
        let headings: Vec<_> = table
            .select("thead th")
            .iter()
            .map(|h| {
                h.attr("data-dbname")
                    .map(|s| s.to_string())
                    .unwrap_or_default()
            })
            .collect();
        for row in table.select("tr.dataRow").iter() {
            let cells: Vec<_> = row
                .select("td")
                .iter()
                .map(|c| normalized(&c.text()))
                .collect();
            let field = |name: &str| {
                headings
                    .iter()
                    .position(|h| h == name)
                    .and_then(|i| cells.get(i))
                    .cloned()
                    .unwrap_or_default()
            };
            let Some(link) = row.select("a[href*='cert_form.php?sku=']").attr("href") else {
                continue;
            };
            let url = reqwest::Url::parse(CERTIFICATE_URL)
                .and_then(|base| base.join(&link))
                .map_err(|_| error("catalogue link", "invalid certificate address"))?;
            if url.scheme() != "https"
                || url.host_str() != Some("www.regattaman.com")
                || url.path() != "/cert_form.php"
            {
                return Err(error(
                    "catalogue link",
                    "certificate link leaves RegattaMan",
                ));
            }
            let sku = url
                .query_pairs()
                .find(|(key, _)| key == "sku")
                .map(|(_, v)| v.to_string())
                .unwrap_or_default();
            if sku.is_empty()
                || sku.len() > 160
                || !sku.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
            {
                return Err(error("catalogue link", "invalid certificate SKU"));
            }
            let model = field("boat_type");
            records.entry(sku.clone()).or_insert(Certificate {
                sku,
                certificate: field("cert_id"),
                year,
                name: field("boat_name"),
                sail_no: field("sail_num"),
                model: (!model.is_empty()).then_some(model),
                fields: headings
                    .iter()
                    .zip(&cells)
                    .filter(|(name, _)| !name.is_empty())
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect(),
            });
        }
    }
    if records.is_empty() {
        return Err(error("valid list", "no public ORR certificate links found"));
    }
    Ok(records.into_values().collect())
}

fn number(text: &str, max: f64, at: &str) -> Result<f64> {
    let text = text
        .trim()
        .trim_end_matches('°')
        .trim_end_matches("kts")
        .trim();
    let value = text
        .parse::<f64>()
        .map_err(|_| error(at, format!("{text:?} is not a number")))?;
    if !value.is_finite() || value < 0.0 || value > max {
        return Err(error(at, format!("{value} is outside 0–{max}")));
    }
    Ok(value)
}

fn polar_table(table: &Selection<'_>, variant: &str) -> Result<PolarGrid> {
    let tws: Vec<_> = table
        .select("thead .tws")
        .iter()
        .enumerate()
        .map(|(j, c)| number(&c.text(), 70.0, &format!("{variant} wind column {}", j + 1)))
        .collect::<Result<_>>()?;
    if tws.is_empty() || tws.len() > 512 || tws.windows(2).any(|p| p[0] >= p[1]) {
        return Err(error(
            variant,
            "wind speeds must be present, increasing and unique",
        ));
    }
    let mut cells: BTreeMap<u64, Vec<Option<f64>>> = BTreeMap::new();
    let mut optimum = BTreeMap::new();
    for (row_index, row) in table.select("tbody > tr").iter().enumerate() {
        let values: Vec<_> = row
            .select("td")
            .iter()
            .map(|c| normalized(&c.text()))
            .collect();
        let Some(label) = values.first() else {
            continue;
        };
        let label = label.to_ascii_lowercase();
        let angle = label.trim_end_matches('°').parse::<f64>().ok();
        let special = ["up angle", "dn angle", "speed up", "speed dn"]
            .iter()
            .find(|prefix| label.starts_with(**prefix))
            .copied();
        if angle.is_none() && special.is_none() {
            continue;
        }
        let at = format!("{variant} row {} ({label})", row_index + 1);
        if values.len() != tws.len() + 1 {
            return Err(error(at, "boat-speed row does not match the wind columns"));
        }
        let angles = special.is_some_and(|s| s.ends_with("angle"));
        let numbers: Vec<_> = values
            .iter()
            .skip(1)
            .enumerate()
            .map(|(j, v)| {
                number(
                    v,
                    if angles { 180.0 } else { 60.0 },
                    &format!("{at}, column {}", j + 2),
                )
            })
            .collect::<Result<_>>()?;
        if let Some(a) = angle {
            let a = number(&a.to_string(), 180.0, &at)?;
            if cells
                .insert(a.to_bits(), numbers.into_iter().map(Some).collect())
                .is_some()
            {
                return Err(error(at, "angle is listed twice"));
            }
        } else if let Some(kind) = special {
            optimum.insert(kind, numbers);
        }
    }
    if cells.is_empty() {
        return Err(error(variant, "no boat-speed rows found"));
    }
    for (angle_name, speed_name) in [("up angle", "speed up"), ("dn angle", "speed dn")] {
        if let (Some(angles), Some(speeds)) = (optimum.get(angle_name), optimum.get(speed_name)) {
            for (j, (angle, speed)) in angles.iter().zip(speeds).enumerate() {
                cells
                    .entry(angle.to_bits())
                    .or_insert_with(|| vec![None; tws.len()])[j] = Some(*speed);
            }
        }
    }
    let (twa, bsp) = cells
        .into_iter()
        .map(|(a, row)| (f64::from_bits(a), row))
        .unzip();
    let polar = PolarGrid { twa, tws, bsp };
    polar
        .validate()
        .map_err(|e| error(variant, e.to_string()))?;
    Ok(polar)
}

/// Preserve the whole public certificate, while using only the two speed
/// containers for the polar. Time allowances never enter a boat-speed grid.
pub fn parse_certificate(html: &str, certificate: &Certificate) -> Result<Vec<OrrRecord>> {
    let doc = Document::from(html);
    let details = details::parse(&doc, certificate)?;
    let measure = |name: &str| -> Option<f64> {
        // The hidden inputs are metric originals; display spans are converted
        // to feet/pounds by page JavaScript and are deliberately not read.
        let input = doc.select(&format!("input[name='{name}']"));
        let value = input.attr("data-origval").or_else(|| input.attr("value"))?;
        value
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && *n >= 0.0)
    };
    let size = OrcSize {
        loa: measure("LOA-bcd"),
        beam: measure("beam_max-bcd"),
        draft: measure("draft_mt-bcd"),
        displacement_kg: measure("disp_mt-bcd"),
        main_area: measure("area_main-calc"),
        genoa_area: measure("area_jib-calc"),
        spinnaker_area: measure("area_symm-calc"),
        asym_spinnaker_area: measure("area_asym-calc"),
        crew_kg: measure("crew_wt-bcd"),
    };
    let mut records = Vec::new();
    for (selector, variant) in [
        ("#polar_speed table.ORRPolars", "offshore"),
        ("#polar_speed_da table.ORRPolars", "short_course"),
    ] {
        let table = doc.select(selector);
        if table.length() == 0 {
            continue;
        }
        let polar = polar_table(&table, variant)?;
        records.push(OrrRecord {
            sku: certificate.sku.clone(),
            variant: variant.into(),
            certificate: certificate.certificate.clone(),
            year: certificate.year,
            name: certificate.name.clone(),
            sail_no: certificate.sail_no.clone(),
            model: certificate.model.clone(),
            size: size.clone(),
            polar,
            details: Some(Box::new(details.clone())),
        });
    }
    Ok(records)
}

/// Download one requested year, sequentially to keep pressure on the public
/// service low. A failure names its certificate; successful records remain
/// usable. The caller writes a deduplicated snapshot only after completion.
pub fn scrape(
    fetcher: &Fetcher,
    year: i32,
    progress: &mut dyn FnMut(usize, usize),
    failure: &mut dyn FnMut(&str, &str),
) -> Result<Vec<OrrRecord>> {
    if !(2018..=2100).contains(&year) {
        return Err(error("year", "ORR certificate year must be 2018–2100"));
    }
    let bytes = fetcher.get(&format!("{CATALOGUE_URL}&yr={year}"), &mut |_, _| {})?;
    let certificates = parse_list(&String::from_utf8_lossy(&bytes), year)?;
    let mut records = BTreeMap::new();
    progress(0, certificates.len());
    for (i, certificate) in certificates.iter().enumerate() {
        fetcher.check()?;
        let result = fetcher
            .get(
                &format!("{CERTIFICATE_URL}{}", certificate.sku),
                &mut |_, _| {},
            )
            .and_then(|bytes| parse_certificate(&String::from_utf8_lossy(&bytes), certificate));
        match result {
            Ok(found) if found.is_empty() => {
                failure(&certificate.certificate, "no public boat-speed table")
            }
            Ok(found) => {
                for record in found {
                    records.insert(record.key(), record);
                }
            }
            Err(TrackerError::Cancelled) => return Err(TrackerError::Cancelled),
            Err(err) => failure(&certificate.certificate, &err.to_string()),
        }
        progress(i + 1, certificates.len());
    }
    fetcher.check()?;
    Ok(records.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    const LIST: &str = include_str!("../tests/fixtures/orr/list.html");
    const CERT: &str = include_str!("../tests/fixtures/orr/phoenix.html");

    #[test]
    fn recorded_list_and_certificate_keep_boat_speeds_variants_and_metric_measurements() {
        let listing = parse_list(LIST, 2026).unwrap();
        assert_eq!(listing.len(), 1);
        assert_eq!(listing[0].name, "PHOENIX");
        assert_eq!(listing[0].model.as_deref(), Some("J/120 CF"));
        let records = parse_certificate(CERT, &listing[0]).unwrap();
        assert_eq!(records.len(), 2);
        let offshore = &records[0];
        assert_eq!(offshore.key(), "h-472-2026-20930-17256-0-17:offshore");
        assert_eq!(
            offshore.polar.tws,
            [4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 20.0, 24.0]
        );
        let row = offshore.polar.twa.iter().position(|a| *a == 52.0).unwrap();
        assert_eq!(
            offshore.polar.bsp[row],
            [3.99, 5.63, 6.8, 7.44, 7.76, 7.95, 8.08, 8.23, 8.31].map(Some)
        );
        let beat = offshore.polar.twa.iter().position(|a| *a == 47.9).unwrap();
        assert_eq!(offshore.polar.bsp[beat][0], Some(3.71));
        assert_eq!(offshore.size.loa, Some(12.244));
        assert_eq!(offshore.size.displacement_kg, Some(6840.0));
        assert_eq!(offshore.size.asym_spinnaker_area, Some(161.29));
        let short = &records[1];
        assert_eq!(short.variant, "short_course");
        let row = short.polar.twa.iter().position(|a| *a == 52.0).unwrap();
        assert_eq!(short.polar.bsp[row][0], Some(3.90));
    }

    #[test]
    fn repeat_links_deduplicate_and_bad_addresses_and_data_are_refused() {
        assert_eq!(parse_list(&format!("{LIST}{LIST}"), 2026).unwrap().len(), 1);
        assert!(
            parse_list(
                &LIST.replace(
                    "cert_form.php?sku=",
                    "https://example.invalid/cert_form.php?sku="
                ),
                2026
            )
            .is_err()
        );
        assert!(parse_list("<p>Unavailable</p>", 2026).is_err());
        let certificate = &parse_list(LIST, 2026).unwrap()[0];
        let bad = CERT.replace(">3.99</td>", ">bogus</td>");
        let error = parse_certificate(&bad, certificate)
            .unwrap_err()
            .to_string();
        assert!(error.contains("offshore row") && error.contains("column 2"));
        // A time-allowance table with the shared ORRPolars class is not a speed table.
        let allowance = "<div id='polar_ta'><table class='ORRPolars'><tr><td>52</td><td>999</td></tr></table></div>";
        assert!(parse_certificate(allowance, certificate).is_err());
    }

    #[test]
    fn full_certificate_preserves_ratings_measurements_and_original_allowances() {
        let certificate = &parse_list(LIST, 2026).unwrap()[0];
        let records = parse_certificate(CERT, certificate).unwrap();
        let details = records[0].details.as_ref().unwrap();
        assert_eq!(details.fields.len(), 257);
        assert_eq!(records[0].details, records[1].details);
        assert_eq!(details.list_fields["IR_sp"], "579.2");
        assert_eq!(details.list_fields["IR_ns"], "613.2");
        assert_eq!(details.list_fields["date_exp"], "2026-12-31");
        assert_eq!(details.fields["owner"].value, "Adams, Barrett");
        assert_eq!(details.fields["builder-bt"].value, "TPI");
        assert_eq!(details.fields["P-bcd"].value, "14.174");
        assert_eq!(details.fields["P-bcd"].quantity, "length");
        assert_eq!(details.fields["MSW-main"].value, "22.000");
        assert_eq!(details.fields["VCGD_mt-calc"].value, "-0.025");
        assert_eq!(details.fields["stab_index-bcd"].value, "121.400");
        assert_eq!(details.fields["downwind_SAD-bcd"].value, "52.470");
        assert_eq!(details.fields["bow_mid_x-hull"].value, "4.126");
        assert_eq!(details.fields["gph"].value, "577.5 SpM");
        assert_eq!(details.fields["cert_comments-ce"].value, "");
        assert_eq!(records[0].build_year(), Some(1994));
        assert_eq!(details.ratings.len(), 5);
        assert_eq!(details.ratings.values().map(Vec::len).sum::<usize>(), 61);
        let custom = &details.ratings["custom_ratings"];
        let chicago = custom
            .iter()
            .find(|r| r.course == "Chicago Mac Upwind")
            .unwrap();
        assert_eq!(chicago.spin.as_deref(), Some("0.9222"));
        assert_eq!(chicago.nonspin, None);
        assert_eq!(
            details.ratings["gph_ratings"][0].spin.as_deref(),
            Some("577.5")
        );
        let pcs = details.ratings["PCS_ratings"]
            .iter()
            .find(|r| r.course == "Newport to Bermuda" && r.wind == "6kt")
            .unwrap();
        assert_eq!(pcs.spin.as_deref(), Some("881.0"));
        assert_eq!(pcs.rtype, "PCS");
        assert_eq!(details.tables.len(), 4);
        assert_eq!(details.tables["polar_time"].unit, "s/nmi");
        let allowance = details.tables["polar_time"]
            .rows
            .iter()
            .find(|r| r[0].starts_with("VMG Up"))
            .unwrap();
        assert_eq!(allowance[1], "1,446.8");
        assert!(
            details.tables["polar_time"]
                .notes
                .iter()
                .any(|n| n.contains("gybing"))
        );
        assert!(!details.fields.contains_key("ipadr"));
        assert!(!details.fields.contains_key("userUID"));
        let with_comment = CERT.replace(
            "class=\"inputtextarea\">\n</span>",
            "class=\"inputtextarea\">Certificate note</span>",
        );
        assert_eq!(
            parse_certificate(&with_comment, certificate).unwrap()[0].field("cert_comments-ce"),
            Some("Certificate note")
        );
    }

    #[test]
    fn absent_or_malformed_ratings_cannot_silently_make_a_partial_scrape() {
        let certificate = &parse_list(LIST, 2026).unwrap()[0];
        assert!(
            parse_certificate(
                &CERT.replace("data-ratingjson", "data-missing"),
                certificate
            )
            .is_err()
        );
        assert!(
            parse_certificate(
                &CERT.replacen("data-ratingjson=\"", "data-ratingjson=\"broken", 1),
                certificate
            )
            .is_err()
        );
        let with_nonspin =
            CERT.replacen("&quot;nonspin&quot;:null", "&quot;nonspin&quot;:1.2345", 1);
        let records = parse_certificate(&with_nonspin, certificate).unwrap();
        assert_eq!(
            records[0].details.as_ref().unwrap().ratings["custom_ratings"][0]
                .nonspin
                .as_deref(),
            Some("1.2345")
        );
        let formatted = CERT.replacen("&quot;577.5&quot;", "&quot;1,234.50&quot;", 1);
        let records = parse_certificate(&formatted, certificate).unwrap();
        assert_eq!(
            records[0].details.as_ref().unwrap().ratings["gph_ratings"][0]
                .spin
                .as_deref(),
            Some("1,234.50")
        );
    }
}
