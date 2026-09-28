#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The settings commands (spec.md 3.4): each change is validated, saved, and
//! survives a restart; a refused change or a failed write changes nothing.

mod common;

use common::TempRoot;
use pe_app::error::AppError;
use pe_app::settings::{
    self, ChunkCacheSettings, DistanceUnit, MapProjection, NetworkSettings, Settings, SpeedUnit,
    Units, WaveHeightUnit,
};

#[test]
fn every_setting_is_saved_and_read_back_after_a_restart() {
    let root = TempRoot::new("settings-roundtrip");
    let app = root.state();
    settings::language_set(&app, "fr".to_owned()).unwrap();
    settings::theme_set(&app, "paper".to_owned()).unwrap();
    let units = Units {
        speed: SpeedUnit::Ms,
        wave_height: WaveHeightUnit::Ft,
        distance: DistanceUnit::Km,
    };
    settings::units_set(&app, units).unwrap();
    settings::autosave_mode_set(&app, settings::AutosaveMode::Off).unwrap();
    let cache_root = root.0.join("big-disk");
    settings::chunk_cache_set(
        &app,
        ChunkCacheSettings {
            location: cache_root.to_string_lossy().into_owned(),
            size_limit_gb: 50,
        },
    )
    .unwrap();
    settings::network_set(
        &app,
        NetworkSettings {
            concurrency: 4,
            timeout_s: 120,
        },
    )
    .unwrap();
    let last = settings::projection_set(&app, MapProjection::Orthographic).unwrap();

    let restarted = root.state();
    let read = settings::current(&restarted).unwrap();
    assert_eq!(read, last);
    assert_eq!(read.language, "fr");
    assert_eq!(read.theme, "paper");
    assert_eq!(read.units, units);
    assert_eq!(read.chunk_cache.size_limit_gb, 50);
    assert_eq!(read.network.concurrency, 4);
    assert_eq!(read.projection, MapProjection::Orthographic);
}

#[test]
fn a_refused_value_changes_nothing() {
    let root = TempRoot::new("settings-refused");
    let app = root.state();
    let before = settings::current(&app).unwrap();
    for result in [
        settings::language_set(&app, "tlh".to_owned()),
        settings::theme_set(&app, "neon".to_owned()),
        settings::chunk_cache_set(
            &app,
            ChunkCacheSettings {
                location: String::new(),
                size_limit_gb: 0,
            },
        ),
        settings::chunk_cache_set(
            &app,
            ChunkCacheSettings {
                location: "relative/folder".to_owned(),
                size_limit_gb: 20,
            },
        ),
        settings::network_set(
            &app,
            NetworkSettings {
                concurrency: 0,
                timeout_s: 60,
            },
        ),
        settings::network_set(
            &app,
            NetworkSettings {
                concurrency: 8,
                timeout_s: 1,
            },
        ),
    ] {
        assert!(
            matches!(result, Err(AppError::BadOption { .. })),
            "{result:?}"
        );
    }
    assert_eq!(settings::current(&app).unwrap(), before);
}

#[test]
fn a_failed_write_puts_the_previous_value_back() {
    let root = TempRoot::new("settings-write");
    let app = root.state();
    // A folder where the file should be: every write fails.
    std::fs::create_dir_all(app.paths.settings_file()).unwrap();
    assert!(settings::language_set(&app, "de".to_owned()).is_err());
    assert_eq!(settings::current(&app).unwrap().language, "en");
}

#[test]
fn the_chunk_cache_reports_its_size_and_clears_only_its_own_folder() {
    let root = TempRoot::new("settings-cache");
    let app = root.state();
    let chosen = root.0.join("chosen");
    std::fs::create_dir_all(&chosen).unwrap();
    std::fs::write(chosen.join("keep-me.txt"), b"user data").unwrap();
    settings::chunk_cache_set(
        &app,
        ChunkCacheSettings {
            location: chosen.to_string_lossy().into_owned(),
            size_limit_gb: 20,
        },
    )
    .unwrap();

    let empty = settings::cache_status(&app).unwrap();
    assert_eq!(empty.bytes, 0);
    assert!(empty.path.ends_with("chunks"));

    let chunks = chosen.join("chunks").join("era5").join("0.0.0");
    std::fs::create_dir_all(chunks.parent().unwrap()).unwrap();
    std::fs::write(&chunks, vec![0u8; 1234]).unwrap();
    assert_eq!(settings::cache_status(&app).unwrap().bytes, 1234);

    let cleared = settings::cache_clear(&app).unwrap();
    assert_eq!(cleared.bytes, 0);
    assert!(
        chosen.join("keep-me.txt").is_file(),
        "only chunks/ is removed"
    );
    assert_eq!(Settings::default().chunk_cache.size_limit_gb, 20);
}
