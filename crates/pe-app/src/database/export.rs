//! Complete PostgreSQL logical export through PostgreSQL's versioned dump utility.
//! Scraping and search never invoke a process. pg_dump is used only for full-fidelity
//! backups (PostGIS, sequences, views, functions, constraints and all user tables).
use super::DatabaseSettings;
use crate::error::{AppError, Context, Result};
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::Duration,
};
fn executable(settings: &DatabaseSettings) -> Result<PathBuf> {
    if !settings.pg_dump.trim().is_empty() {
        return Ok(PathBuf::from(settings.pg_dump.trim()));
    }
    let name = if cfg!(windows) {
        "pg_dump.exe"
    } else {
        "pg_dump"
    };
    let mut roots: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for base in [
        "/usr/local/opt",
        "/opt/homebrew/opt",
        "/Applications/Postgres.app/Contents/Versions",
        "/Library/PostgreSQL",
    ] {
        if let Ok(entries) = std::fs::read_dir(base) {
            let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            entries.sort();
            entries.reverse();
            roots.extend(entries.into_iter().map(|e| e.join("bin")));
        }
    }
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(variable)
            && let Ok(entries) = std::fs::read_dir(PathBuf::from(root).join("PostgreSQL"))
        {
            let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
            entries.sort();
            entries.reverse();
            roots.extend(entries.into_iter().map(|e| e.join("bin")));
        }
    }
    roots.into_iter().map(|r|r.join(name)).find(|p|p.is_file()).ok_or_else(||AppError::Internal("Install PostgreSQL client tools or set the pg_dump executable in Settings to export the entire database".into()))
}
pub(super) fn run(settings: &DatabaseSettings, path: &Path, cancel: &AtomicBool) -> Result<()> {
    if !path.is_absolute() {
        return Err(AppError::Internal(
            "Choose an absolute export filename".into(),
        ));
    }
    let exe = executable(settings)?;
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Internal("Invalid export path".into()))?;
    let temp = parent.join(format!(".polarexplorer-{}.sql", uuid::Uuid::new_v4()));
    let errors = parent.join(format!(".polarexplorer-{}.log", uuid::Uuid::new_v4()));
    let result = (|| {
        let output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .doing("create", temp.display())?;
        let stderr = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&errors)
            .doing("create", "export diagnostic file")?;
        let mut command = Command::new(exe);
        command
            .args([
                "--format=plain",
                "--no-owner",
                "--no-privileges",
                "--no-password",
                "--host",
                &settings.host,
                "--port",
                &settings.port.to_string(),
                "--username",
                &settings.user,
                "--dbname",
                &settings.name,
            ])
            .env("PGPASSWORD", &settings.password)
            .env("PGCONNECT_TIMEOUT", "10")
            .env(
                "PGSSLMODE",
                if settings.tls {
                    "verify-full"
                } else {
                    "disable"
                },
            )
            .stdin(Stdio::null())
            .stdout(output)
            .stderr(stderr);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn().doing("start", "pg_dump")?;
        let status = loop {
            if super::check(cancel).is_err() {
                let _ = child.kill();
                let _ = child.wait();
                return super::check(cancel);
            }
            if let Some(status) = child.try_wait().doing("wait for", "database export")? {
                break status;
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        if !status.success() {
            let detail = std::fs::read_to_string(&errors).unwrap_or_default();
            return Err(AppError::Internal(format!(
                "Database export failed: {}",
                detail.replace(
                    &settings.password,
                    if settings.password.is_empty() {
                        ""
                    } else {
                        "[redacted]"
                    }
                )
            )));
        }
        super::check(cancel)?;
        std::fs::rename(&temp, path).doing("publish database export to", path.display())?;
        Ok(())
    })();
    let _ = std::fs::remove_file(&temp);
    let _ = std::fs::remove_file(&errors);
    result
}
