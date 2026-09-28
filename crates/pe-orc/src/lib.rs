//! The embedded ORC catalogue.
//!
//! The catalogue is generated at build time from a `jieter/orc-data` checkout
//! by `tools/orc-catalogue-builder` and embedded in the binary, with a search
//! index over it (spec.md 5). It is never fetched at run time (invariant 4),
//! so this crate has no network code.
//!
//! Nothing is decoded until the catalogue is first used: [`catalogue`]
//! decompresses it and builds the index once, and [`provenance`] reads only
//! the small uncompressed header, for the About box.

pub mod fold;
pub mod format;
pub mod search;

use std::sync::OnceLock;

use pe_core::orc::OrcRecord;

pub use format::{Entry, Provenance};
pub use search::{Filters, Hits};

/// The catalogue as built into this binary.
pub static EMBEDDED: &[u8] = include_bytes!("../data/catalogue.bin");

/// Why the catalogue could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OrcError {
    /// The bytes are not a catalogue this build can read.
    #[error("The ORC catalogue is damaged: {0}")]
    Corrupt(String),
}

/// The decoded catalogue and its search index.
#[derive(Debug)]
pub struct Catalogue {
    provenance: Provenance,
    entries: Vec<Entry>,
    index: search::Index,
    countries: Vec<String>,
}

impl Catalogue {
    /// Decodes a catalogue and indexes it.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, OrcError> {
        let (provenance, entries) = format::decode(bytes)?;
        let index = search::Index::new(&entries);
        let mut countries: Vec<String> = entries.iter().map(|e| e.country.clone()).collect();
        countries.sort();
        countries.dedup();
        Ok(Self {
            provenance,
            entries,
            index,
            countries,
        })
    }

    /// Where the records came from.
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// How many records it holds.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether it holds none.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// One record by catalogue id.
    pub fn entry(&self, id: u32) -> Option<&Entry> {
        self.entries.get(id as usize)
    }

    /// Every country with a certificate, sorted.
    pub fn countries(&self) -> &[String] {
        &self.countries
    }

    /// The earliest and latest year built.
    pub fn year_range(&self) -> Option<(i32, i32)> {
        let years = self.entries.iter().filter_map(|e| e.year);
        let min = years.clone().min()?;
        let max = years.max()?;
        Some((min, max))
    }

    /// Searches (spec.md 5.2); see [`search`].
    pub fn search(&self, query: &str, filters: &Filters, limit: usize) -> Hits {
        self.index.search(&self.entries, query, filters, limit)
    }
}

static CATALOGUE: OnceLock<Result<Catalogue, OrcError>> = OnceLock::new();

/// The embedded catalogue, decoded and indexed on first use.
pub fn catalogue() -> Result<&'static Catalogue, OrcError> {
    CATALOGUE
        .get_or_init(|| Catalogue::from_bytes(EMBEDDED))
        .as_ref()
        .map_err(Clone::clone)
}

/// The embedded catalogue's provenance, without decoding its records.
pub fn provenance() -> Result<Provenance, OrcError> {
    format::decode_provenance(EMBEDDED)
}

/// Whether two records are the same certificate (spec.md 5.3: adding one
/// twice asks first). orc-data gives no certificate number, so a certificate
/// is known by its boat, sail number and year.
pub fn same_certificate(a: &OrcRecord, b: &OrcRecord) -> bool {
    if let (Some(x), Some(y)) = (&a.ref_no, &b.ref_no) {
        return x == y;
    }
    a.country == b.country
        && a.sail_no == b.sail_no
        && a.name == b.name
        && a.model == b.model
        && a.year == b.year
        && a.certificate_year == b.certificate_year
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_certificate_ignores_nothing_that_identifies_it() {
        let a = Entry {
            sail_no: "GBR 1124".to_owned(),
            country: "GBR".to_owned(),
            name: "Jiminy".to_owned(),
            certificate_year: Some(2024),
            ..Entry::default()
        }
        .to_record();
        let mut b = a.clone();
        assert!(same_certificate(&a, &b));
        b.certificate_year = Some(2025);
        assert!(!same_certificate(&a, &b));
        let mut c = a.clone();
        c.name = "Sister".to_owned();
        assert!(!same_certificate(&a, &c));
        let mut d = a.clone();
        d.gph = Some(1.0);
        assert!(same_certificate(&a, &d));
        // A certificate number, when both have one, decides alone.
        let (mut e, mut f) = (a.clone(), c.clone());
        e.ref_no = Some("03160002QKB".to_owned());
        f.ref_no = Some("03160002QKB".to_owned());
        assert!(same_certificate(&e, &f));
    }
}
