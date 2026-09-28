//! Regenerates the frontend's TypeScript bindings from the Rust types.
//!
//! `cargo run -p pe-app --example export_bindings`, or `npm run bindings`.
//!
//! Every type that crosses IPC must be listed here. CI runs this and fails if
//! the working tree changes, so bindings cannot drift from the Rust types.

use std::path::PathBuf;

use pe_app::autosave::RecoveredProject;
use pe_app::commands::AppInfo;
use pe_app::error::AppErrorPayload;
use pe_app::polar_files::{PolarImportFailure, PolarImportResult};
use pe_app::projects::{BoatInput, PolarFileSummary, ProjectSummary, RecentProject, SourceSummary};
use pe_app::settings::{AutosaveMode, ChunkCacheStatus, Settings};
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
