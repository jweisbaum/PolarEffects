//! The on-disk catalogue: `catalogue.bin` (spec.md 5.1).
//!
//! Layout, all integers little-endian:
//!
//! ```text
//! b"PEORC\0"            magic
//! u16                    format version
//! u32                    header length, bytes
//! header                 postcard `Provenance`, uncompressed
//! body                   LZ4 block (size-prepended) of postcard `Vec<Entry>`
//! ```
//!
//! The provenance header is readable without decompressing anything, so the
//! About box can name the orc-data commit while the catalogue itself stays
//! unloaded until the first search.
//!
//! Every speed and angle is stored as a whole number of hundredths. orc-data
//! gives them to two decimals (its speeds are `round(3600 / allowance, 2)`),
//! so this is exact, and `hundredths as f64 / 100.0` reads back the very same
//! `f64` a JSON parser would have produced for the original text.

use pe_core::orc::{OrcRecord, OrcSize, OrcVpp};
use serde::{Deserialize, Serialize};

use crate::OrcError;

/// The first bytes of every catalogue.
pub const MAGIC: &[u8; 6] = b"PEORC\0";
/// The layout version this build reads and writes.
pub const FORMAT_VERSION: u16 = 1;

/// Where the catalogue came from, shown in About (spec.md 5.1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// The repository the records were read from, `"jieter/orc-data"`.
    pub source: String,
    /// The full commit hash of that checkout.
    pub commit: String,
    /// The commit's date, `YYYY-MM-DD`.
    pub commit_date: String,
    /// The day the catalogue was built, `YYYY-MM-DD` (UTC).
    pub build_date: String,
    /// How many records the catalogue holds.
    pub records: u32,
    /// How many per-boat files the builder read but left out.
    pub dropped: u32,
}

/// One certificate as the catalogue stores it. Converted to the document's
/// [`OrcRecord`] when it is added to a project.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// Sail number as shown, e.g. `"GBR 1124"`; empty when the certificate
    /// has none.
    pub sail_no: String,
    /// Three-letter country code.
    pub country: String,
    /// Boat name.
    pub name: String,
    /// Type or model.
    pub model: Option<String>,
    /// Builder.
    pub builder: Option<String>,
    /// Designer.
    pub designer: Option<String>,
    /// Year built.
    pub year: Option<i32>,
    /// Year of the certificate, when the builder could tell (spec.md 5.1).
    pub certificate_year: Option<i32>,
    /// Size fields: LOA, beam, draft, displacement, main, genoa, spinnaker,
    /// asymmetric spinnaker, crew, in that order. See [`OrcSize`].
    pub size: [Option<f64>; 9],
    /// General purpose handicap, s/NM.
    pub gph: Option<f64>,
    /// Offshore single number, s/NM.
    pub osn: Option<f64>,
    /// The VPP, in hundredths.
    pub vpp: Vpp,
}

/// An ORC VPP table in hundredths of a degree and of a knot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vpp {
    /// True wind angles.
    pub angles: Vec<u16>,
    /// True wind speeds.
    pub speeds: Vec<u16>,
    /// Boat speed per angle (rows) per wind speed (columns).
    pub bsp: Vec<Vec<Option<u16>>>,
    /// Beat angle per wind speed.
    pub beat_angle: Vec<u16>,
    /// Beat VMG per wind speed.
    pub beat_vmg: Vec<u16>,
    /// Run angle per wind speed.
    pub run_angle: Vec<u16>,
    /// Run VMG per wind speed.
    pub run_vmg: Vec<u16>,
}

/// A value given to two decimals as whole hundredths; `None` when it is not
/// exactly that, or out of range.
pub fn hundredths(value: f64) -> Option<u16> {
    let scaled = value * 100.0;
    let rounded = scaled.round();
    if !value.is_finite() || (scaled - rounded).abs() > 1e-6 || !(0.0..=65535.0).contains(&rounded)
    {
        return None;
    }
    // In range and whole, checked just above.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(rounded as u16)
}

/// Hundredths back to the value.
pub fn from_hundredths(value: u16) -> f64 {
    f64::from(value) / 100.0
}

fn list(values: &[u16]) -> Vec<f64> {
    values.iter().copied().map(from_hundredths).collect()
}

impl Vpp {
    /// The table as the document keeps it.
    pub fn to_document(&self) -> OrcVpp {
        OrcVpp {
            angles: list(&self.angles),
            speeds: list(&self.speeds),
            bsp: self
                .bsp
                .iter()
                .map(|row| row.iter().map(|cell| cell.map(from_hundredths)).collect())
                .collect(),
            beat_angle: list(&self.beat_angle),
            beat_vmg: list(&self.beat_vmg),
            run_angle: list(&self.run_angle),
            run_vmg: list(&self.run_vmg),
        }
    }
}

impl Entry {
    /// The record a project stores when this certificate is added.
    pub fn to_record(&self) -> OrcRecord {
        let [
            loa,
            beam,
            draft,
            displacement_kg,
            main_area,
            genoa_area,
            spinnaker_area,
            asym_spinnaker_area,
            crew_kg,
        ] = self.size;
        OrcRecord {
            ref_no: None,
            sail_no: self.sail_no.clone(),
            country: self.country.clone(),
            name: self.name.clone(),
            model: self.model.clone(),
            builder: self.builder.clone(),
            designer: self.designer.clone(),
            year: self.year,
            certificate_year: self.certificate_year,
            size: OrcSize {
                loa,
                beam,
                draft,
                displacement_kg,
                main_area,
                genoa_area,
                spinnaker_area,
                asym_spinnaker_area,
                crew_kg,
            },
            gph: self.gph,
            osn: self.osn,
            vpp: self.vpp.to_document(),
        }
    }
}

fn encoding(err: impl std::fmt::Display) -> OrcError {
    OrcError::Corrupt(err.to_string())
}

/// Writes a catalogue.
pub fn encode(provenance: &Provenance, entries: &[Entry]) -> Result<Vec<u8>, OrcError> {
    let header = postcard::to_allocvec(provenance).map_err(encoding)?;
    let body = postcard::to_allocvec(entries).map_err(encoding)?;
    let header_len = u32::try_from(header.len()).map_err(encoding)?;
    let mut out = Vec::with_capacity(16 + header.len() + body.len() / 3);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    out.extend_from_slice(&header_len.to_le_bytes());
    out.extend_from_slice(&header);
    out.extend_from_slice(&lz4_flex::block::compress_prepend_size(&body));
    Ok(out)
}

/// Splits a catalogue into its provenance and its compressed body.
fn split(bytes: &[u8]) -> Result<(Provenance, &[u8]), OrcError> {
    let rest = bytes
        .strip_prefix(MAGIC.as_slice())
        .ok_or_else(|| OrcError::Corrupt("this is not a PolarEffects ORC catalogue".to_owned()))?;
    let (version, rest) = rest
        .split_first_chunk::<2>()
        .ok_or_else(|| OrcError::Corrupt("the catalogue ends in its header".to_owned()))?;
    let version = u16::from_le_bytes(*version);
    if version != FORMAT_VERSION {
        return Err(OrcError::Corrupt(format!(
            "the catalogue has layout version {version}; this build reads {FORMAT_VERSION}"
        )));
    }
    let (len, rest) = rest
        .split_first_chunk::<4>()
        .ok_or_else(|| OrcError::Corrupt("the catalogue ends in its header".to_owned()))?;
    let len = usize::try_from(u32::from_le_bytes(*len)).map_err(encoding)?;
    if rest.len() < len {
        return Err(OrcError::Corrupt(
            "the catalogue ends in its header".to_owned(),
        ));
    }
    let (header, body) = rest.split_at(len);
    let provenance = postcard::from_bytes(header).map_err(encoding)?;
    Ok((provenance, body))
}

/// Reads only the provenance header, without decompressing the records.
pub fn decode_provenance(bytes: &[u8]) -> Result<Provenance, OrcError> {
    split(bytes).map(|(provenance, _)| provenance)
}

/// Reads a whole catalogue.
pub fn decode(bytes: &[u8]) -> Result<(Provenance, Vec<Entry>), OrcError> {
    let (provenance, body) = split(bytes)?;
    let body = lz4_flex::block::decompress_size_prepended(body).map_err(encoding)?;
    let entries: Vec<Entry> = postcard::from_bytes(&body).map_err(encoding)?;
    if entries.len() != provenance.records as usize {
        return Err(OrcError::Corrupt(format!(
            "the catalogue says it holds {} records but holds {}",
            provenance.records,
            entries.len()
        )));
    }
    Ok((provenance, entries))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> Entry {
        Entry {
            sail_no: "GBR 1124".to_owned(),
            country: "GBR".to_owned(),
            name: "Jiminy".to_owned(),
            model: Some("Farr 40".to_owned()),
            year: Some(1998),
            certificate_year: Some(2024),
            size: [Some(12.41), None, None, None, None, None, None, None, None],
            vpp: Vpp {
                angles: vec![5200, 9000],
                speeds: vec![600, 1200],
                bsp: vec![vec![Some(707), Some(857)], vec![Some(759), None]],
                beat_angle: vec![4360, 4180],
                beat_vmg: vec![460, 571],
                run_angle: vec![14080, 17000],
                run_vmg: vec![441, 572],
            },
            ..Entry::default()
        }
    }

    #[test]
    fn hundredths_are_exact_or_refused() {
        assert_eq!(hundredths(7.07), Some(707));
        assert_eq!(hundredths(43.6), Some(4360));
        assert_eq!(hundredths(0.0), Some(0));
        assert_eq!(hundredths(7.071), None);
        assert_eq!(hundredths(-1.0), None);
        assert_eq!(hundredths(f64::NAN), None);
        assert_eq!(hundredths(700.0), None);
        // Reads back as the value a JSON parser gives for the same text.
        for text in ["7.07", "10.18", "43.6", "0.01", "155.84", "24"] {
            let value: f64 = text.parse().unwrap();
            assert_eq!(from_hundredths(hundredths(value).unwrap()), value, "{text}");
        }
    }

    #[test]
    fn a_catalogue_round_trips_and_its_header_reads_alone() {
        let provenance = Provenance {
            source: "jieter/orc-data".to_owned(),
            commit: "c2ca870c".to_owned(),
            commit_date: "2026-09-28".to_owned(),
            build_date: "2026-09-28".to_owned(),
            records: 1,
            dropped: 2,
        };
        let bytes = encode(&provenance, &[entry()]).unwrap();
        assert_eq!(decode_provenance(&bytes).unwrap(), provenance);
        let (read, entries) = decode(&bytes).unwrap();
        assert_eq!(read, provenance);
        assert_eq!(entries, vec![entry()]);
    }

    #[test]
    fn a_damaged_catalogue_is_an_error_not_a_panic() {
        let provenance = Provenance {
            records: 1,
            ..Provenance::default()
        };
        let bytes = encode(&provenance, &[entry()]).unwrap();
        for cut in 0..bytes.len() {
            let _ = decode(&bytes[..cut]);
        }
        assert!(decode(b"").is_err());
        assert!(decode(b"PEORC\0\x02\0").is_err());
        let mut wrong_count = encode(&Provenance::default(), &[entry()]).unwrap();
        assert!(decode(&wrong_count).is_err());
        wrong_count[0] = b'X';
        assert!(decode_provenance(&wrong_count).is_err());
    }

    #[test]
    fn an_entry_becomes_the_document_record() {
        let record = entry().to_record();
        assert_eq!(record.sail_no, "GBR 1124");
        assert_eq!(record.certificate_year, Some(2024));
        assert_eq!(record.size.loa, Some(12.41));
        assert_eq!(record.vpp.angles, vec![52.0, 90.0]);
        assert_eq!(record.vpp.bsp[0], vec![Some(7.07), Some(8.57)]);
        assert_eq!(record.vpp.bsp[1][1], None);
        assert_eq!(record.vpp.beat_angle, vec![43.6, 41.8]);
        assert_eq!(record.vpp.run_vmg, vec![4.41, 5.72]);
    }
}
