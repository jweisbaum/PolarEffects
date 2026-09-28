//! The ORC polars section over IPC (spec.md 5).
//!
//! The catalogue is embedded in the binary (`pe_orc`) and decoded on first
//! use; nothing here touches the network (invariant 4). A search returns a
//! light result per boat — the fields the list shows and a thumbnail of
//! three wind speeds — and Add copies the full record into the project as an
//! ORC source, one undo entry, with the next palette colour.

use pe_core::source::Source;
use pe_core::{Command, SourceKind};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Result};
use crate::projects::ProjectSummary;

/// The history label of Add: an English key to translate.
pub const ADD_ORC: &str = "Add ORC polar";
/// The most results one search returns.
pub const MAX_LIMIT: u32 = 200;
/// The wind speeds the thumbnail draws, knots: light, medium and strong. Each
/// is matched to the nearest the certificate has.
const THUMB_TWS: [f64; 3] = [6.0, 12.0, 20.0];

/// What the catalogue is and where it came from (spec.md 5.1), for the
/// section's footer, its filters and About.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "OrcCatalogueInfo.ts")]
pub struct OrcCatalogueInfo {
    /// How many certificates it holds.
    pub records: u32,
    /// The repository it was built from, `"jieter/orc-data"`.
    pub source: String,
    /// The commit it was built from, in full.
    pub commit: String,
    /// That commit's date, `YYYY-MM-DD`.
    pub commit_date: String,
    /// The day it was built, `YYYY-MM-DD`.
    pub build_date: String,
    /// Every country with a certificate, as three-letter codes, sorted.
    pub countries: Vec<String>,
    /// The earliest year built.
    pub year_min: Option<i32>,
    /// The latest year built.
    pub year_max: Option<i32>,
}

/// The search's filters (spec.md 5.2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, TS)]
#[ts(export_to = "OrcFilters.ts")]
pub struct OrcFilters {
    /// Earliest year built, inclusive.
    pub year_min: Option<i32>,
    /// Latest year built, inclusive.
    pub year_max: Option<i32>,
    /// A three-letter country code.
    pub country: Option<String>,
}

/// One wind speed's curve in a result's thumbnail: the points the polar has
/// at that speed, by increasing angle.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "OrcThumbCurve.ts")]
pub struct OrcThumbCurve {
    /// Wind speed, knots.
    pub tws: f64,
    /// True wind angles, degrees.
    pub twa: Vec<f64>,
    /// Boat speeds, knots, one per angle.
    pub bsp: Vec<f64>,
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "OrcHit.ts")]
pub struct OrcHit {
    /// Catalogue id, for [`orc_add`]. Valid for this build only.
    pub id: u32,
    /// Boat name.
    pub name: String,
    /// Sail number as shown; empty when there is none.
    pub sail_no: String,
    /// Three-letter country code.
    pub country: String,
    /// Type or model.
    pub model: Option<String>,
    /// Builder.
    pub builder: Option<String>,
    /// Year built.
    pub year: Option<i32>,
    /// Year of the certificate, when known.
    pub certificate_year: Option<i32>,
    /// Whether the open project already holds this certificate.
    pub in_project: bool,
    /// Light, medium and strong wind curves for the thumbnail.
    pub thumb: Vec<OrcThumbCurve>,
}

/// A search's answer.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "OrcSearchResult.ts")]
pub struct OrcSearchResult {
    /// How many certificates matched in all.
    pub total: u32,
    /// The best of them, best first.
    pub hits: Vec<OrcHit>,
}

fn catalogue() -> Result<&'static pe_orc::Catalogue> {
    pe_orc::catalogue().map_err(|e| AppError::Internal(e.to_string()))
}

/// The catalogue's size, provenance, countries and years.
#[tauri::command]
pub fn orc_catalogue_info() -> Result<OrcCatalogueInfo> {
    info()
}

/// [`orc_catalogue_info`], also for tests.
pub fn info() -> Result<OrcCatalogueInfo> {
    let catalogue = catalogue()?;
    let provenance = catalogue.provenance();
    let years = catalogue.year_range();
    Ok(OrcCatalogueInfo {
        records: u32::try_from(catalogue.len()).unwrap_or(u32::MAX),
        source: provenance.source.clone(),
        commit: provenance.commit.clone(),
        commit_date: provenance.commit_date.clone(),
        build_date: provenance.build_date.clone(),
        countries: catalogue.countries().to_vec(),
        year_min: years.map(|(min, _)| min),
        year_max: years.map(|(_, max)| max),
    })
}

/// The thumbnail curves of a record.
fn thumbnail(record: &pe_core::orc::OrcRecord) -> Vec<OrcThumbCurve> {
    let polar = pe_polar::vpp_to_polar(&record.vpp);
    let mut columns: Vec<usize> = THUMB_TWS
        .iter()
        .filter_map(|want| {
            (0..polar.tws.len()).min_by(|&a, &b| {
                (polar.tws[a] - want)
                    .abs()
                    .total_cmp(&(polar.tws[b] - want).abs())
            })
        })
        .collect();
    columns.dedup();
    columns
        .into_iter()
        .map(|j| {
            let (twa, bsp) = polar
                .twa
                .iter()
                .zip(&polar.bsp)
                .filter_map(|(twa, row)| row[j].map(|bsp| (*twa, bsp)))
                .unzip();
            OrcThumbCurve {
                tws: polar.tws[j],
                twa,
                bsp,
            }
        })
        .collect()
}

/// Searches the catalogue (spec.md 5.2).
#[tauri::command]
pub fn orc_search(
    state: tauri::State<'_, AppState>,
    query: String,
    filters: OrcFilters,
    limit: u32,
) -> Result<OrcSearchResult> {
    search(&state, &query, filters, limit)
}

/// [`orc_search`] without a Tauri handle.
pub fn search(
    state: &AppState,
    query: &str,
    filters: OrcFilters,
    limit: u32,
) -> Result<OrcSearchResult> {
    let catalogue = catalogue()?;
    let filters = pe_orc::Filters {
        year_min: filters.year_min,
        year_max: filters.year_max,
        country: filters.country.filter(|c| !c.trim().is_empty()),
    };
    let hits = catalogue.search(query, &filters, limit.clamp(1, MAX_LIMIT) as usize);
    let records: Vec<(u32, pe_core::orc::OrcRecord)> = hits
        .ids
        .iter()
        .filter_map(|id| catalogue.entry(*id).map(|e| (*id, e.to_record())))
        .collect();
    // Which are already in the project; no project open means none are.
    let present: Vec<bool> = state.with_session(|session| {
        Ok(match session.require_open() {
            Ok(open) => records
                .iter()
                .map(|(_, record)| holds(&open.project, record))
                .collect(),
            Err(_) => vec![false; records.len()],
        })
    })?;
    Ok(OrcSearchResult {
        total: u32::try_from(hits.total).unwrap_or(u32::MAX),
        hits: records
            .into_iter()
            .zip(present)
            .map(|((id, record), in_project)| OrcHit {
                id,
                thumb: thumbnail(&record),
                name: record.name,
                sail_no: record.sail_no,
                country: record.country,
                model: record.model,
                builder: record.builder,
                year: record.year,
                certificate_year: record.certificate_year,
                in_project,
            })
            .collect(),
    })
}

/// Whether `project` already has `record` as a source.
fn holds(project: &pe_core::Project, record: &pe_core::orc::OrcRecord) -> bool {
    project.sources.iter().any(|source| match &source.kind {
        SourceKind::Orc { record: held } => pe_orc::same_certificate(held, record),
        _ => false,
    })
}

/// A source's label for a certificate: its name, else its model, else its
/// sail number.
fn label(record: &pe_core::orc::OrcRecord) -> String {
    [
        record.name.as_str(),
        record.model.as_deref().unwrap_or(""),
        record.sail_no.as_str(),
    ]
    .into_iter()
    .find(|text| !text.trim().is_empty())
    .unwrap_or("ORC")
    .trim()
    .to_owned()
}

/// Adds a certificate to the project as an ORC source (spec.md 5.3). A
/// certificate the project already holds is refused with kind
/// `"orc-duplicate"` unless `allow_duplicate`: the frontend asks, then calls
/// again.
#[tauri::command]
pub fn orc_add(
    state: tauri::State<'_, AppState>,
    id: u32,
    allow_duplicate: bool,
) -> Result<ProjectSummary> {
    add(&state, id, allow_duplicate)
}

/// [`orc_add`] without a Tauri handle.
pub fn add(state: &AppState, id: u32, allow_duplicate: bool) -> Result<ProjectSummary> {
    state.with_session(|session| session.require_open().map(|_| ()))?;
    let record = catalogue()?
        .entry(id)
        .ok_or_else(|| AppError::BadOption {
            field: "ORC record",
            value: id.to_string(),
        })?
        .to_record();
    state.with_session(|session| {
        let open = session.require_open()?;
        if !allow_duplicate && holds(&open.project, &record) {
            return Err(AppError::DuplicateCertificate {
                name: label(&record),
            });
        }
        let colour = open.project.next_palette_colour();
        let source_id = open.project.allocate_source_id();
        let source = Source::new(
            source_id,
            label(&record),
            colour,
            SourceKind::Orc {
                record: Box::new(record),
            },
        );
        let index = open.project.sources.len();
        open.apply(Command::Batch {
            label: ADD_ORC.to_owned(),
            commands: vec![Command::AddSource {
                index,
                source: Box::new(source),
            }],
        })?;
        Ok(ProjectSummary::of(open))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_thumbnail_has_light_medium_and_strong_wind() {
        let catalogue = catalogue().unwrap();
        let hits = catalogue.search("GBR 1124", &pe_orc::Filters::default(), 1);
        let record = catalogue.entry(hits.ids[0]).unwrap().to_record();
        let thumb = thumbnail(&record);
        assert_eq!(
            thumb.iter().map(|c| c.tws).collect::<Vec<_>>(),
            vec![6.0, 12.0, 20.0]
        );
        // Eight ORC angles plus that wind speed's beat and run angles.
        let light = &thumb[0];
        assert_eq!(light.twa.len(), 10);
        assert_eq!(light.twa[0], 47.1);
        assert!(light.twa.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn a_label_falls_back_from_name_to_model_to_sail() {
        let mut record = pe_core::orc::OrcRecord {
            name: " ".to_owned(),
            model: Some("Swan 112".to_owned()),
            sail_no: "GBR 1124".to_owned(),
            ..Default::default()
        };
        assert_eq!(label(&record), "Swan 112");
        record.model = None;
        assert_eq!(label(&record), "GBR 1124");
    }
}
