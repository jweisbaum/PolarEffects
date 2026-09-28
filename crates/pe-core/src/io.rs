//! Reading and writing `.wpsproj` project files (spec.md 4.3).
//!
//! The container rules are VectorEffects' `.veproj` ones: a deflate ZIP of
//! canonical JSON with a plain-text version marker, fixed entry timestamps
//! and an atomic save. JSON over a binary format for diffability and
//! recoverability.
//!
//! The archive holds exactly:
//!
//! - `META-INF/version`: the schema version in plain text, so a newer file is
//!   refused before its document is parsed;
//! - `project.json`: the document, pretty-printed, everything except bulk
//!   track data;
//! - `tracks/<track id>.json`: one per track, its fixes and samples, compact.
//!
//! **Nothing else, ever.** No rendered images, no blend results (invariant 2).
//! The writer cannot produce another entry, and the reader refuses an archive
//! that has one rather than silently dropping it on the next save.

use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::project::{Project, SCHEMA_VERSION};
use crate::track::TrackBulk;

/// The project document inside the archive.
pub const PROJECT_ENTRY: &str = "project.json";
/// A plain-text schema version, readable without parsing the document.
pub const VERSION_ENTRY: &str = "META-INF/version";
/// The directory of per-track bulk entries.
pub const TRACKS_PREFIX: &str = "tracks/";
/// The file extension, without a dot.
pub const EXTENSION: &str = "wpsproj";

/// A function that upgrades a document from one schema version to the next.
pub type Migration = fn(&mut Value) -> Result<()>;

/// The migration chain, keyed by the version each step upgrades *from*
/// (spec.md 4.3). Empty until the first schema change: version 1 is the first
/// this application wrote.
pub const MIGRATIONS: &[(u32, Migration)] = &[];

/// The archive entry holding one track's bulk data.
pub fn track_entry(id: crate::id::TrackId) -> String {
    format!("{TRACKS_PREFIX}{}.json", id.raw())
}

/// Serialises a project document to canonical JSON.
///
/// Canonical means byte-identical for equal documents: struct fields
/// serialise in declaration order, no map in the model is a `HashMap`, and
/// every float goes through [`crate::canonical`].
pub fn to_canonical_json(project: &Project) -> Result<String> {
    Ok(serde_json::to_string_pretty(project)?)
}

/// Parses, migrates and validates a project document, without bulk data.
pub fn from_json(json: &str) -> Result<Project> {
    let mut value: Value = serde_json::from_str(json)?;
    migrate(&mut value)?;
    let mut project: Project = serde_json::from_value(value)?;
    project.schema_version = SCHEMA_VERSION;
    Ok(project)
}

fn migrate(value: &mut Value) -> Result<()> {
    run_migrations(value, MIGRATIONS, SCHEMA_VERSION).map(|_| ())
}

/// Runs a migration chain up to `target`. Split out so tests can drive a
/// synthetic chain.
fn run_migrations(value: &mut Value, migrations: &[(u32, Migration)], target: u32) -> Result<u32> {
    let mut version = value
        .get("schema_version")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| CoreError::Migration {
            from: 0,
            reason: "the document has no schema_version".to_owned(),
        })?;

    if version > target {
        return Err(CoreError::SchemaTooNew {
            found: version,
            supported: target,
        });
    }
    while version < target {
        let Some((_, step)) = migrations.iter().find(|(from, _)| *from == version) else {
            return Err(CoreError::Migration {
                from: version,
                reason: format!("no migration from version {version}"),
            });
        };
        step(value)?;
        version += 1;
        value["schema_version"] = Value::from(version);
    }
    Ok(version)
}

/// Writes a project to `path`, atomically.
///
/// The archive is written to `<name>.wpsproj.tmp` beside it and renamed into
/// place, so an interrupted save cannot leave a truncated file where a valid
/// project used to be.
pub fn save(project: &Project, path: &Path) -> Result<()> {
    write_atomic(path, &to_bytes(project)?)
}

/// Writes `bytes` to `path` atomically: to `<name>.tmp` beside it, synced,
/// then renamed into place. A crash mid-write leaves the old file intact.
/// Used for project files, the settings file and autosave manifests.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let temp = temp_path_for(path);
    let written = std::fs::File::create(&temp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    if let Err(err) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(err.into());
    }
    // Rename is atomic within a filesystem; if it fails, the original file is
    // still intact and only the temporary is left behind, then removed.
    if let Err(err) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(err.into());
    }
    Ok(())
}

fn temp_path_for(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

/// The archive bytes for a project. Everything is validated and serialised
/// before a single byte reaches the disk.
pub fn to_bytes(project: &Project) -> Result<Vec<u8>> {
    use zip::CompressionMethod;
    use zip::write::SimpleFileOptions;

    project.validate()?;
    let json = to_canonical_json(project)?;
    // Validation above saw the in-memory values; the file holds them rounded
    // (`canonical`). Two axis values closer than the canonical precision are
    // distinct in memory and equal on disk, and that file would never open
    // again. So the document is also validated as it will be read back, and
    // an unsaveable one is refused here, before anything is written.
    from_json(&json)?.validate_document()?;

    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    // A fixed timestamp keeps the archive byte-reproducible: with the clock
    // baked in, two saves of an unchanged project would differ.
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default())
        .unix_permissions(0o644);

    zip.start_file(VERSION_ENTRY, options).map_err(zip_err)?;
    zip.write_all(SCHEMA_VERSION.to_string().as_bytes())?;
    zip.start_file(PROJECT_ENTRY, options).map_err(zip_err)?;
    zip.write_all(json.as_bytes())?;

    // In source order: the document's order, not a hash map's.
    for source in &project.sources {
        if let Some(track) = source.track() {
            zip.start_file(track_entry(track.id), options)
                .map_err(zip_err)?;
            let bulk = serde_json::to_vec(&track.bulk())?;
            zip.write_all(&bulk)?;
        }
    }
    Ok(zip.finish().map_err(zip_err)?.into_inner())
}

/// Reads a project from `path`.
pub fn load(path: &Path) -> Result<Project> {
    from_bytes(&std::fs::read(path)?)
}

/// Reads a project from archive bytes.
pub fn from_bytes(bytes: &[u8]) -> Result<Project> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(zip_err)?;

    // The version first: a newer file is refused with both versions named,
    // before its document is parsed at all.
    let version = read_version(&mut archive)?;
    if version > SCHEMA_VERSION {
        return Err(CoreError::SchemaTooNew {
            found: version,
            supported: SCHEMA_VERSION,
        });
    }

    let json = read_entry(&mut archive, PROJECT_ENTRY)?;
    let mut project = from_json(&json)?;

    let mut expected: BTreeSet<String> = [VERSION_ENTRY, PROJECT_ENTRY]
        .into_iter()
        .map(str::to_owned)
        .collect();
    for source in &mut project.sources {
        let Some(track) = source.track_mut() else {
            continue;
        };
        let name = track_entry(track.id);
        let bulk: TrackBulk = serde_json::from_str(&read_entry(&mut archive, &name)?)?;
        track.fixes = bulk.fixes;
        track.samples = bulk.samples;
        expected.insert(name);
    }
    if let Some(unexpected) = archive
        .file_names()
        .find(|name| !expected.contains(*name) && !is_os_artefact(name))
    {
        return Err(CoreError::Archive(format!(
            "it holds {unexpected:?}, which is not part of a project"
        )));
    }

    project.validate()?;
    Ok(project)
}

/// Whether an archive entry is litter an operating system adds when a user
/// re-zips or browses the file: macOS `__MACOSX/` resource forks and
/// `.DS_Store`, Windows `Thumbs.db`. Ignored on load (and never written back)
/// rather than refused, because they carry nothing of the project.
fn is_os_artefact(name: &str) -> bool {
    let file = name.rsplit('/').next().unwrap_or(name);
    name.starts_with("__MACOSX/") || file == ".DS_Store" || file == "Thumbs.db"
}

/// Reads only the schema version, without parsing the document.
pub fn peek_version(path: &Path) -> Result<u32> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(zip_err)?;
    read_version(&mut archive)
}

fn read_version<R: Read + std::io::Seek>(archive: &mut zip::ZipArchive<R>) -> Result<u32> {
    let text = read_entry(archive, VERSION_ENTRY)?;
    text.trim()
        .parse()
        .map_err(|_| CoreError::Archive(format!("its version marker {text:?} is not a number")))
}

fn read_entry<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<String> {
    let mut text = String::new();
    archive
        .by_name(name)
        .map_err(|_| CoreError::Archive(format!("it has no {name}")))?
        .read_to_string(&mut text)?;
    Ok(text)
}

fn zip_err(err: zip::result::ZipError) -> CoreError {
    CoreError::Archive(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;
    use proptest::prelude::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "pe-io-{}-{label}-{}",
                std::process::id(),
                crate::id::ProjectId::fresh().raw()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn entry_names(bytes: &[u8]) -> Vec<String> {
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        archive.file_names().map(str::to_owned).collect()
    }

    /// Rebuilds an archive with its entries changed by `edit`.
    fn rewrite(bytes: &[u8], edit: impl Fn(&mut Vec<(String, Vec<u8>)>)) -> Vec<u8> {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut entries = Vec::new();
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).unwrap();
            let mut data = Vec::new();
            file.read_to_end(&mut data).unwrap();
            entries.push((file.name().to_owned(), data));
        }
        edit(&mut entries);
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in entries {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&data).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    #[test]
    fn a_project_round_trips_through_a_file() {
        let dir = TempDir::new("round-trip");
        let path = dir.path("a.wpsproj");
        let project = fixtures::project();
        save(&project, &path).unwrap();
        let loaded = load(&path).unwrap();
        assert_eq!(loaded, project);
        assert_eq!(loaded.sources[2].track().unwrap().samples.len(), 3);
    }

    /// The M1 acceptance criterion, for any generated project: save, load,
    /// save again, and the bytes are identical.
    #[test]
    fn save_load_save_is_byte_identical_for_the_fixture() {
        let first = to_bytes(&fixtures::project()).unwrap();
        let second = to_bytes(&from_bytes(&first).unwrap()).unwrap();
        assert_eq!(first, second);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(128))]

        #[test]
        fn any_project_survives_save_load_save_byte_identically(project in fixtures::arb_project()) {
            let first = to_bytes(&project).unwrap();
            let loaded = from_bytes(&first).unwrap();
            let second = to_bytes(&loaded).unwrap();
            prop_assert_eq!(&first, &second);
            // And the loaded document is a fixed point.
            prop_assert_eq!(&from_bytes(&second).unwrap(), &loaded);
        }
    }

    #[test]
    fn saving_twice_gives_identical_files() {
        let dir = TempDir::new("twice");
        let (a, b) = (dir.path("a.wpsproj"), dir.path("b.wpsproj"));
        let project = fixtures::project();
        save(&project, &a).unwrap();
        save(&project, &b).unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), std::fs::read(&b).unwrap());
    }

    /// Spec 4.3: exactly the version marker, the document and one entry per
    /// track. A rendered image or a blend result would show up here.
    #[test]
    fn the_archive_holds_no_unexpected_entries() {
        let project = fixtures::project();
        let track = project.sources[2].track().unwrap().id;
        let names = entry_names(&to_bytes(&project).unwrap());
        assert_eq!(
            names,
            vec![
                VERSION_ENTRY.to_owned(),
                PROJECT_ENTRY.to_owned(),
                track_entry(track)
            ]
        );
    }

    #[test]
    fn an_archive_with_an_unexpected_entry_is_refused() {
        let bytes = to_bytes(&fixtures::project()).unwrap();
        let tampered = rewrite(&bytes, |entries| {
            entries.push(("blend.png".to_owned(), vec![0x89, b'P', b'N', b'G']));
        });
        let err = from_bytes(&tampered).unwrap_err();
        assert!(
            matches!(&err, CoreError::Archive(m) if m.contains("blend.png")),
            "{err}"
        );

        // A bulk entry for a track the document does not have is also foreign.
        let stray = rewrite(&bytes, |entries| {
            entries.push(("tracks/999.json".to_owned(), b"{}".to_vec()));
        });
        assert!(matches!(from_bytes(&stray), Err(CoreError::Archive(_))));
    }

    #[test]
    fn operating_system_artefacts_are_ignored_on_load() {
        let project = fixtures::project();
        let bytes = to_bytes(&project).unwrap();
        let littered = rewrite(&bytes, |entries| {
            for name in [
                "__MACOSX/._project.json",
                "__MACOSX/tracks/._3.json",
                ".DS_Store",
                "tracks/.DS_Store",
                "Thumbs.db",
            ] {
                entries.push((name.to_owned(), b"junk".to_vec()));
            }
        });
        let loaded = from_bytes(&littered).unwrap();
        assert_eq!(loaded, project);
        // And they are not carried into the next save.
        assert_eq!(to_bytes(&loaded).unwrap(), bytes);

        // Anything merely resembling one is still foreign.
        let lookalike = rewrite(&bytes, |entries| {
            entries.push(("MACOSX/x".to_owned(), vec![]));
        });
        assert!(matches!(from_bytes(&lookalike), Err(CoreError::Archive(_))));
    }

    /// Controller ruling on M1: values distinct in memory but equal once
    /// rounded would write a file that can never open. Refused at save time.
    #[test]
    fn a_document_that_rounds_into_an_invalid_one_is_not_written() {
        let dir = TempDir::new("rounds-invalid");
        let path = dir.path("p.wpsproj");

        let mut polar_axis = fixtures::project();
        if let crate::source::SourceKind::PolarFile { polar, .. } = &mut polar_axis.sources[1].kind
        {
            polar.twa[1] = polar.twa[2] - 1e-12;
        }
        polar_axis.validate().unwrap();
        let err = save(&polar_axis, &path).unwrap_err();
        assert!(err.to_string().contains("strictly increasing"), "{err}");

        let mut grid = fixtures::project();
        grid.grid.tws[1] = grid.grid.tws[0] + 1e-9;
        grid.validate().unwrap();
        assert!(save(&grid, &path).is_err());

        assert!(!path.exists(), "nothing was written");
        assert!(!dir.path("p.wpsproj.tmp").exists());
    }

    #[test]
    fn write_atomic_replaces_whole_files_and_leaves_no_temporary() {
        let dir = TempDir::new("write-atomic");
        let path = dir.path("settings.json");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert!(!dir.path("settings.json.tmp").exists());
        assert!(write_atomic(&dir.path("missing/dir/x"), b"x").is_err());
    }

    #[test]
    fn a_missing_track_entry_is_refused() {
        let bytes = to_bytes(&fixtures::project()).unwrap();
        let cut = rewrite(&bytes, |entries| {
            entries.retain(|(name, _)| !name.starts_with(TRACKS_PREFIX));
        });
        let err = from_bytes(&cut).unwrap_err();
        assert!(err.to_string().contains("tracks/"), "{err}");
    }

    #[test]
    fn project_json_holds_no_bulk_track_data() {
        let json = to_canonical_json(&fixtures::project()).unwrap();
        assert!(!json.contains("\"fixes\""), "{json}");
        assert!(!json.contains("\"samples\""));
        assert!(json.contains("\"env_meta\""));
    }

    /// Acceptance: a newer file is refused with a message naming both
    /// versions, and before the document is parsed (here, it is not even
    /// JSON).
    #[test]
    fn a_newer_schema_is_refused_naming_both_versions() {
        let bytes = to_bytes(&fixtures::project()).unwrap();
        let newer = SCHEMA_VERSION + 1;
        let future = rewrite(&bytes, |entries| {
            for (name, data) in entries.iter_mut() {
                if name == VERSION_ENTRY {
                    *data = newer.to_string().into_bytes();
                }
                if name == PROJECT_ENTRY {
                    *data = b"{ something from the future".to_vec();
                }
            }
        });
        let err = from_bytes(&future).unwrap_err();
        assert!(
            matches!(err, CoreError::SchemaTooNew { found, supported } if found == newer && supported == SCHEMA_VERSION)
        );
        let message = err.to_string();
        assert!(message.contains(&format!("version {newer}")), "{message}");
        assert!(
            message.contains(&format!("up to version {SCHEMA_VERSION}")),
            "{message}"
        );
    }

    /// The document's own version is checked too, for a marker that lies.
    #[test]
    fn a_newer_document_behind_an_old_marker_is_refused() {
        let mut value = serde_json::to_value(fixtures::project()).unwrap();
        value["schema_version"] = Value::from(SCHEMA_VERSION + 5);
        let err = from_json(&value.to_string()).unwrap_err();
        assert!(matches!(err, CoreError::SchemaTooNew { .. }));
    }

    #[test]
    fn the_version_marker_can_be_read_without_parsing() {
        let dir = TempDir::new("peek");
        let path = dir.path("p.wpsproj");
        save(&fixtures::project(), &path).unwrap();
        assert_eq!(peek_version(&path).unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn a_document_without_a_version_is_refused() {
        let mut value = serde_json::to_value(fixtures::project()).unwrap();
        value.as_object_mut().unwrap().remove("schema_version");
        assert!(matches!(
            from_json(&value.to_string()),
            Err(CoreError::Migration { from: 0, .. })
        ));
    }

    #[test]
    fn the_migration_runner_walks_the_chain() {
        fn one_to_two(value: &mut Value) -> Result<()> {
            value["added_in_2"] = Value::from(true);
            Ok(())
        }
        fn two_to_three(value: &mut Value) -> Result<()> {
            let had = value["added_in_2"].as_bool().unwrap_or(false);
            value["seen_2"] = Value::from(had);
            Ok(())
        }
        let chain: &[(u32, Migration)] = &[(2, two_to_three), (1, one_to_two)];
        let mut value = serde_json::json!({ "schema_version": 1 });
        assert_eq!(run_migrations(&mut value, chain, 3).unwrap(), 3);
        assert_eq!(value["schema_version"], 3);
        assert_eq!(value["seen_2"], true);

        let mut gap = serde_json::json!({ "schema_version": 1 });
        assert!(matches!(
            run_migrations(&mut gap, &[(2, two_to_three)], 3),
            Err(CoreError::Migration { from: 1, .. })
        ));
    }

    #[test]
    #[allow(
        clippy::reversed_empty_ranges,
        reason = "empty while SCHEMA_VERSION is 1; the check matters from the first bump"
    )]
    fn the_shipped_chain_reaches_the_current_version_from_every_older_one() {
        for from in 1..SCHEMA_VERSION {
            assert!(
                MIGRATIONS.iter().any(|(v, _)| *v == from),
                "no migration from {from}"
            );
        }
    }

    #[test]
    fn a_file_that_is_not_an_archive_is_refused() {
        assert!(matches!(
            from_bytes(b"not a project"),
            Err(CoreError::Archive(_))
        ));
    }

    #[test]
    fn saving_leaves_no_temporary_file_and_replaces_the_old_one() {
        let dir = TempDir::new("atomic");
        let path = dir.path("p.wpsproj");
        std::fs::write(&path, b"old").unwrap();
        save(&fixtures::project(), &path).unwrap();
        assert!(!dir.path("p.wpsproj.tmp").exists());
        assert!(load(&path).is_ok());
    }

    /// An invalid document is refused before anything touches the disk, so
    /// the file that was there survives.
    #[test]
    fn an_invalid_project_is_not_written() {
        let dir = TempDir::new("invalid");
        let path = dir.path("p.wpsproj");
        save(&fixtures::project(), &path).unwrap();
        let before = std::fs::read(&path).unwrap();

        let mut bad = fixtures::project();
        bad.sources[0].weight = f64::NAN;
        assert!(save(&bad, &path).is_err());
        let mut bad = fixtures::project();
        if let Some(track) = bad.sources[2].track_mut() {
            track.samples[0].tws = Some(f64::INFINITY);
        }
        assert!(save(&bad, &path).is_err());

        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!dir.path("p.wpsproj.tmp").exists());
    }
}
