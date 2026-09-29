//! A GRIB file written message by message, atomically (spec.md 7.8).
//!
//! Messages go to `<name>.tmp` beside the target as they are made, never
//! gathered in memory; [`GribFile::commit`] syncs it and renames it into
//! place, so an export that fails or is cancelled part way never leaves a
//! truncated `.grib2` where the user asked for one, nor replaces a file
//! already there. Dropping an uncommitted file removes the temporary.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::writer::{MessageSpec, message};

/// An export being written.
#[derive(Debug)]
pub struct GribFile {
    target: PathBuf,
    temp: PathBuf,
    out: Option<BufWriter<File>>,
    bytes: u64,
    messages: u32,
}

/// What a committed file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Written {
    /// Its size.
    pub bytes: u64,
    /// Its messages.
    pub messages: u32,
}

/// `<name>.tmp` beside `path`.
pub fn temp_path_for(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    path.with_file_name(name)
}

impl GribFile {
    /// Starts writing `target` (through its temporary).
    ///
    /// # Errors
    /// The temporary cannot be created.
    pub fn create(target: &Path) -> Result<Self> {
        let temp = temp_path_for(target);
        let file = File::create(&temp)?;
        Ok(Self {
            target: target.to_path_buf(),
            temp,
            out: Some(BufWriter::with_capacity(1 << 20, file)),
            bytes: 0,
            messages: 0,
        })
    }

    /// Appends one message.
    ///
    /// # Errors
    /// As [`message`], or the write failing.
    pub fn write(&mut self, spec: &MessageSpec, values: &[f32]) -> Result<()> {
        let bytes = message(spec, values)?;
        if let Some(out) = self.out.as_mut() {
            out.write_all(&bytes)?;
        }
        self.bytes += bytes.len() as u64;
        self.messages += 1;
        Ok(())
    }

    /// Bytes written so far.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Flushes, syncs and renames the file into place.
    ///
    /// # Errors
    /// Any of those failing; the temporary is then removed and a file
    /// already at the target is left as it was.
    pub fn commit(mut self) -> Result<Written> {
        let done = (|| -> Result<()> {
            if let Some(out) = self.out.take() {
                let file = out.into_inner().map_err(|e| e.into_error())?;
                file.sync_all()?;
            }
            std::fs::rename(&self.temp, &self.target)?;
            Ok(())
        })();
        match done {
            Ok(()) => Ok(Written {
                bytes: self.bytes,
                messages: self.messages,
            }),
            Err(err) => {
                let _ = std::fs::remove_file(&self.temp);
                Err(err)
            }
        }
    }
}

impl Drop for GribFile {
    fn drop(&mut self) {
        // Committed: `out` was taken and the temporary renamed away.
        if self.out.take().is_some() {
            let _ = std::fs::remove_file(&self.temp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::writer::{GridSpec, Parameter, ReferenceTime};

    fn spec() -> MessageSpec {
        MessageSpec {
            parameter: Parameter::WindU,
            grid: GridSpec::global(10_000_000),
            reference_time: ReferenceTime::from_epoch(0).unwrap(),
            forecast_hour: 0,
            centre: 255,
            bits: 16,
        }
    }

    fn dir(label: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pe-grib-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_committed_file_is_the_messages_in_order() {
        let d = dir("commit");
        let target = d.join("out.grib2");
        let values = vec![1.0f32; spec().grid.point_count() as usize];
        let mut file = GribFile::create(&target).unwrap();
        file.write(&spec(), &values).unwrap();
        file.write(
            &MessageSpec {
                forecast_hour: 1,
                ..spec()
            },
            &values,
        )
        .unwrap();
        assert!(!target.exists(), "nothing at the target before the commit");
        let written = file.commit().unwrap();
        let bytes = std::fs::read(&target).unwrap();
        let one = message(&spec(), &values).unwrap();
        assert_eq!(written.messages, 2);
        assert_eq!(written.bytes, bytes.len() as u64);
        assert_eq!(&bytes[..one.len()], &one[..]);
        assert!(!temp_path_for(&target).exists());
        let _ = std::fs::remove_dir_all(d);
    }

    /// A cancelled export leaves the file that was there, and no temporary.
    #[test]
    fn a_dropped_file_leaves_the_old_one_and_no_temporary() {
        let d = dir("drop");
        let target = d.join("out.grib2");
        std::fs::write(&target, b"before").unwrap();
        let values = vec![1.0f32; spec().grid.point_count() as usize];
        let mut file = GribFile::create(&target).unwrap();
        file.write(&spec(), &values).unwrap();
        drop(file);
        assert_eq!(std::fs::read(&target).unwrap(), b"before");
        assert!(!temp_path_for(&target).exists());
        let _ = std::fs::remove_dir_all(d);
    }
}
