//! Regenerates the frontend's TypeScript bindings from the Rust types.
//!
//! `cargo run -p pe-app --example export_bindings`, or `npm run bindings`.
//!
//! Every type that crosses IPC must be listed here. CI runs this and fails if
//! the working tree changes, so bindings cannot drift from the Rust types.

use std::path::PathBuf;

use pe_app::autosave::RecoveredProject;
use pe_app::commands::AppInfo;
use pe_app::env::{EnvEstimate, EnvJobTrack, EnvJobsStatus};
use pe_app::error::AppErrorPayload;
use pe_app::orc::{OrcCatalogueInfo, OrcFilters, OrcHit, OrcSearchResult, OrcThumbCurve};
use pe_app::polar_edit::{EditOp, EditSurface, PolarCell};
use pe_app::polar_files::{PolarImportFailure, PolarImportResult};
use pe_app::polar_plot::{PolarCurve, PolarCurvePoint, PolarPlotResult};
use pe_app::polar3d::PolarNodeRef;
use pe_app::projects::{
    BoatInput, OrcSourceSummary, PolarFileSummary, ProjectSummary, RecentProject, SourceSummary,
};
use pe_app::settings::{AutosaveMode, ChunkCacheStatus, Settings};
use pe_app::trackers::{TrackerBoatRow, TrackerEventView, TrackerProgress};
use pe_app::tracks::{
    CsvMappingInput, CsvPreview, SampleDetails, TrackBoatPreview, TrackFileInspection,
    TrackFileRequest, TrackFilters, TrackImportFailure, TrackImportLine, TrackImportResult,
    TrackSummary,
};
use ts_rs::{Config, TS};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Resolved against the manifest rather than the cwd, so the command works
    // from anywhere in the workspace.
    let out_dir: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/generated");
    std::fs::create_dir_all(&out_dir)?;

    // Ids, revisions, epoch seconds and byte counts are all 64-bit but never
    // approach 2^53, and bigint is painful to work with in React —
    // comparisons, JSON, template strings all need care. Plain numbers keep
    // the frontend simple.
    let cfg = Config::new()
        .with_out_dir(&out_dir)
        .with_large_int("number");
    AppInfo::export_all(&cfg)?;
    AppErrorPayload::export_all(&cfg)?;
    ProjectSummary::export_all(&cfg)?;
    SourceSummary::export_all(&cfg)?;
    PolarFileSummary::export_all(&cfg)?;
    PolarImportResult::export_all(&cfg)?;
    PolarImportFailure::export_all(&cfg)?;
    PolarCurvePoint::export_all(&cfg)?;
    PolarCurve::export_all(&cfg)?;
    PolarCell::export_all(&cfg)?;
    EditSurface::export_all(&cfg)?;
    EditOp::export_all(&cfg)?;
    PolarPlotResult::export_all(&cfg)?;
    PolarNodeRef::export_all(&cfg)?;
    TrackSummary::export_all(&cfg)?;
    TrackerBoatRow::export_all(&cfg)?;
    TrackerEventView::export_all(&cfg)?;
    TrackerProgress::export_all(&cfg)?;
    EnvEstimate::export_all(&cfg)?;
    EnvJobTrack::export_all(&cfg)?;
    EnvJobsStatus::export_all(&cfg)?;
    TrackFilters::export_all(&cfg)?;
    CsvMappingInput::export_all(&cfg)?;
    CsvPreview::export_all(&cfg)?;
    TrackBoatPreview::export_all(&cfg)?;
    TrackFileInspection::export_all(&cfg)?;
    TrackFileRequest::export_all(&cfg)?;
    TrackImportFailure::export_all(&cfg)?;
    TrackImportLine::export_all(&cfg)?;
    TrackImportResult::export_all(&cfg)?;
    SampleDetails::export_all(&cfg)?;
    OrcSourceSummary::export_all(&cfg)?;
    OrcCatalogueInfo::export_all(&cfg)?;
    OrcFilters::export_all(&cfg)?;
    OrcHit::export_all(&cfg)?;
    OrcThumbCurve::export_all(&cfg)?;
    OrcSearchResult::export_all(&cfg)?;
    RecentProject::export_all(&cfg)?;
    BoatInput::export_all(&cfg)?;
    RecoveredProject::export_all(&cfg)?;
    AutosaveMode::export_all(&cfg)?;
    Settings::export_all(&cfg)?;
    ChunkCacheStatus::export_all(&cfg)?;

    // Keep generated files deterministic and free of ts-rs's trailing spaces.
    for entry in std::fs::read_dir(&out_dir)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "ts") {
            let source = std::fs::read_to_string(&path)?;
            let clean = source
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n");
            std::fs::write(path, format!("{clean}\n"))?;
        }
    }
    println!("bindings written to {}", out_dir.display());
    Ok(())
}
