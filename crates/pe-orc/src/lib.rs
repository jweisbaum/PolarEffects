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

pub use format::{Entry, Provenance, Scraped};
pub use search::{Fields, Filters, Hits};

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
    /// The certificate's reference number, for the entries that came from a
    /// scrape; the embedded catalogue has none.
    refs: Vec<Option<String>>,
    /// The scraped certificates ORC has withdrawn: places kept so that ids
    /// do not move, but not certificates of the catalogue any more.
    withdrawn: Vec<bool>,
    index: search::Index,
    countries: Vec<String>,
}

/// What makes two entries the same certificate when there is no reference
/// number to say so: the boat, its sail number and the certificate's year,
/// as [`same_certificate`] compares records.
type Identity = (
    String,
    String,
    String,
    Option<String>,
    Option<i32>,
    Option<i32>,
);

fn identity(entry: &Entry) -> Identity {
    (
        entry.country.clone(),
        entry.sail_no.clone(),
        entry.name.clone(),
        entry.model.clone(),
        entry.year,
        entry.certificate_year,
    )
}

/// What a scrape did to the store.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Merged {
    /// Certificates the store did not have.
    pub added: u32,
    /// Certificates it had, read differently now (or back after being
    /// withdrawn).
    pub updated: u32,
    /// Certificates ORC no longer lists for their year.
    pub withdrawn: u32,
}

/// Writes a scrape into the stored certificates, so that the store holds
/// each certificate once and nothing ORC has taken back (spec.md 5.4).
///
/// A certificate is its reference number: an incoming one replaces the
/// stored one with its number, in its place, and a new one goes to the end.
/// Two certificates of one boat in one year are two certificates — a crewed
/// and a double-handed one, say — and both stay.
///
/// `listed` names, for each country whose list was read whole, every
/// reference on it. A stored certificate of `year` from such a country that
/// is not on its list any more was replaced or revoked, and is withdrawn:
/// it keeps its place, so catalogue ids stay good across scrapes, and is no
/// longer searched. A country whose list came back empty withdraws nothing:
/// an answer of nothing is what the service gives when it is unwell, too.
/// Other years' certificates are never touched.
pub fn merge_scraped(
    stored: &mut Vec<Scraped>,
    incoming: Vec<Scraped>,
    listed: &std::collections::BTreeMap<String, std::collections::BTreeSet<String>>,
    year: i32,
) -> Merged {
    use std::collections::HashMap;
    let mut by_ref: HashMap<String, usize> = stored
        .iter()
        .enumerate()
        .map(|(place, held)| (held.ref_no.clone(), place))
        .collect();
    let mut merged = Merged::default();
    for scraped in incoming {
        match by_ref.get(&scraped.ref_no) {
            Some(&place) => {
                if stored[place] != scraped {
                    merged.updated += 1;
                    stored[place] = scraped;
                }
            }
            None => {
                merged.added += 1;
                by_ref.insert(scraped.ref_no.clone(), stored.len());
                stored.push(scraped);
            }
        }
    }
    for held in stored.iter_mut() {
        if held.withdrawn || held.entry.certificate_year != Some(year) {
            continue;
        }
        let gone = listed
            .get(&held.entry.country)
            .is_some_and(|refs| !refs.is_empty() && !refs.contains(&held.ref_no));
        if gone {
            held.withdrawn = true;
            merged.withdrawn += 1;
        }
    }
    merged
}

impl Catalogue {
    /// Decodes a catalogue and indexes it.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, OrcError> {
        Self::with_scraped(bytes, &[])
    }

    /// The catalogue in `bytes` together with scraped certificates, each
    /// listed once (spec.md 5.4).
    ///
    /// A scraped certificate that is the same certificate as an embedded
    /// entry takes that entry's place, the scraped data standing for both;
    /// the others follow the embedded entries in the order given. So an
    /// embedded entry's id is the same with or without a scrape, and a
    /// scraped one's does not change while `scraped` only grows at its end.
    /// A withdrawn certificate keeps a place after the embedded entries and
    /// is found by nothing.
    pub fn with_scraped(bytes: &[u8], scraped: &[Scraped]) -> Result<Self, OrcError> {
        let (provenance, mut entries) = format::decode(bytes)?;
        let mut refs: Vec<Option<String>> = vec![None; entries.len()];
        let mut withdrawn = vec![false; entries.len()];
        if !scraped.is_empty() {
            let mut embedded: std::collections::HashMap<Identity, usize> =
                std::collections::HashMap::with_capacity(entries.len());
            for (id, entry) in entries.iter().enumerate() {
                embedded.entry(identity(entry)).or_insert(id);
            }
            for item in scraped {
                // A withdrawn certificate replaces nothing: the embedded
                // entry it once stood for is in the catalogue again.
                let twin = if item.withdrawn {
                    None
                } else {
                    embedded.remove(&identity(&item.entry))
                };
                match twin {
                    Some(id) => {
                        entries[id] = item.entry.clone();
                        refs[id] = Some(item.ref_no.clone());
                    }
                    None => {
                        entries.push(item.entry.clone());
                        refs.push(Some(item.ref_no.clone()));
                        withdrawn.push(item.withdrawn);
                    }
                }
            }
        }
        let mut index = search::Index::new(&entries);
        for (id, gone) in withdrawn.iter().enumerate() {
            if *gone {
                index.hide(id);
            }
        }
        let mut countries: Vec<String> = entries
            .iter()
            .zip(&withdrawn)
            .filter(|(_, gone)| !**gone)
            .map(|(entry, _)| entry.country.clone())
            .collect();
        countries.sort();
        countries.dedup();
        Ok(Self {
            provenance,
            entries,
            refs,
            withdrawn,
            index,
            countries,
        })
    }

    /// One certificate as the record a project stores, with its reference
    /// number when it was scraped. A withdrawn certificate is none.
    pub fn record(&self, id: u32) -> Option<OrcRecord> {
        let mut record = self.entry(id)?.to_record();
        record.ref_no = self.refs.get(id as usize).cloned().flatten();
        Some(record)
    }

    /// How many certificates it holds: its places less the withdrawn ones.
    pub fn certificates(&self) -> usize {
        self.withdrawn.iter().filter(|gone| !**gone).count()
    }

    /// How many of its certificates came from a scrape.
    pub fn scraped(&self) -> usize {
        self.refs
            .iter()
            .zip(&self.withdrawn)
            .filter(|(reference, gone)| reference.is_some() && !**gone)
            .count()
    }

    /// Where the records came from.
    pub fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// How many places it has: one more than its largest id. A scrape can
    /// leave a withdrawn certificate's place empty; see
    /// [`Self::certificates`] for how many certificates there are.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether it holds none.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// One record by catalogue id; none for a withdrawn certificate's.
    pub fn entry(&self, id: u32) -> Option<&Entry> {
        if self.withdrawn.get(id as usize).copied().unwrap_or(true) {
            return None;
        }
        self.entries.get(id as usize)
    }

    /// Every country with a certificate, sorted.
    pub fn countries(&self) -> &[String] {
        &self.countries
    }

    /// The earliest and latest year built.
    pub fn year_range(&self) -> Option<(i32, i32)> {
        let years = self
            .entries
            .iter()
            .zip(&self.withdrawn)
            .filter(|(_, gone)| !**gone)
            .filter_map(|(e, _)| e.year);
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

    fn boat(sail: &str, name: &str, certificate_year: i32) -> Entry {
        Entry {
            sail_no: sail.to_owned(),
            country: "NOR".to_owned(),
            name: name.to_owned(),
            model: Some("Elan 350".to_owned()),
            year: Some(2009),
            certificate_year: Some(certificate_year),
            vpp: format::Vpp {
                angles: vec![5200],
                speeds: vec![600],
                bsp: vec![vec![Some(537)]],
                beat_angle: vec![4260],
                beat_vmg: vec![354],
                run_angle: vec![14280],
                run_vmg: vec![387],
            },
            ..Entry::default()
        }
    }

    fn scraped(ref_no: &str, entry: Entry) -> Scraped {
        Scraped {
            ref_no: ref_no.to_owned(),
            entry,
            withdrawn: false,
        }
    }

    /// The references a country's list named, as the scraper reports them.
    fn listed(
        refs: &[&str],
    ) -> std::collections::BTreeMap<String, std::collections::BTreeSet<String>> {
        [(
            "NOR".to_owned(),
            refs.iter().map(|r| (*r).to_owned()).collect(),
        )]
        .into()
    }

    fn names(catalogue: &Catalogue) -> Vec<(String, Option<i32>, Option<String>)> {
        (0..catalogue.len())
            .filter_map(|id| catalogue.record(u32::try_from(id).unwrap()))
            .map(|record| (record.name, record.certificate_year, record.ref_no))
            .collect()
    }

    fn counts(added: u32, updated: u32, withdrawn: u32) -> Merged {
        Merged {
            added,
            updated,
            withdrawn,
        }
    }

    #[test]
    fn scraping_again_stores_nothing_twice() {
        let mut stored = Vec::new();
        // Two certificates of one boat in one year (crewed and
        // double-handed, as ORC issues them) are two certificates.
        let mut double_handed = boat("NOR 1", "Momentum", 2026);
        double_handed.size[8] = Some(200.0);
        let first = vec![
            scraped("A1", boat("NOR 1", "Momentum", 2026)),
            scraped("A1DH", double_handed),
            scraped("A2", boat("NOR 2", "Lazy", 2026)),
        ];
        let all = listed(&["A1", "A1DH", "A2"]);
        assert_eq!(
            merge_scraped(&mut stored, first.clone(), &all, 2026),
            counts(3, 0, 0)
        );
        // The same answer again changes nothing at all.
        assert_eq!(
            merge_scraped(&mut stored, first.clone(), &all, 2026),
            counts(0, 0, 0)
        );
        assert_eq!(stored, first);

        // A certificate measured again under its number keeps its place.
        let mut remeasured = boat("NOR 1", "Momentum", 2026);
        remeasured.gph = Some(601.0);
        let mut again = first.clone();
        again[0] = scraped("A1", remeasured.clone());
        assert_eq!(
            merge_scraped(&mut stored, again.clone(), &all, 2026),
            counts(0, 1, 0)
        );
        assert_eq!(stored, again);
    }

    #[test]
    fn a_certificate_orc_no_longer_lists_is_withdrawn_and_keeps_its_place() {
        let mut stored = Vec::new();
        let first = vec![
            scraped("A1", boat("NOR 1", "Momentum", 2026)),
            scraped("A2", boat("NOR 2", "Lazy", 2026)),
            scraped("OLD", boat("NOR 3", "Elder", 2025)),
        ];
        merge_scraped(&mut stored, first, &listed(&["A1", "A2", "OLD"]), 2026);

        // Lazy's certificate was issued again under a new number: the old
        // one is gone from the list. Last year's certificate is not this
        // year's list's business.
        let next = vec![
            scraped("A1", boat("NOR 1", "Momentum", 2026)),
            scraped("B7", boat("NOR 2", "Lazy", 2026)),
        ];
        assert_eq!(
            merge_scraped(&mut stored, next.clone(), &listed(&["A1", "B7"]), 2026),
            counts(1, 0, 1)
        );
        assert_eq!(
            stored
                .iter()
                .map(|s| (s.ref_no.as_str(), s.withdrawn))
                .collect::<Vec<_>>(),
            vec![("A1", false), ("A2", true), ("OLD", false), ("B7", false)]
        );
        // And again: nothing more to do.
        assert_eq!(
            merge_scraped(&mut stored, next, &listed(&["A1", "B7"]), 2026),
            counts(0, 0, 0)
        );

        // A country that did not answer, or answered nothing, takes nothing
        // back; nor does a list that names a certificate we could not read.
        let before = stored.clone();
        let none = std::collections::BTreeMap::new();
        assert_eq!(
            merge_scraped(&mut stored, vec![], &none, 2026),
            counts(0, 0, 0)
        );
        assert_eq!(
            merge_scraped(&mut stored, vec![], &listed(&[]), 2026),
            counts(0, 0, 0)
        );
        assert_eq!(
            merge_scraped(
                &mut stored,
                vec![scraped("A1", boat("NOR 1", "Momentum", 2026))],
                &listed(&["A1", "B7"]),
                2026
            ),
            counts(0, 0, 0)
        );
        assert_eq!(stored, before);

        // A withdrawn certificate that is listed again is back, in its place.
        assert_eq!(
            merge_scraped(
                &mut stored,
                vec![scraped("A2", boat("NOR 2", "Lazy", 2026))],
                &listed(&["A1", "A2", "B7"]),
                2026
            ),
            counts(0, 1, 0)
        );
        assert!(!stored[1].withdrawn);
    }

    #[test]
    fn a_withdrawn_certificate_is_in_no_search_and_moves_no_id() {
        let bytes = format::encode(&Provenance::default(), &[]).unwrap();
        let mut gone = scraped("A2", boat("NOR 2", "Lazy", 2026));
        gone.withdrawn = true;
        let store = vec![
            scraped("A1", boat("NOR 1", "Momentum", 2026)),
            gone,
            scraped("B7", boat("NOR 2", "Lazy", 2026)),
        ];
        let catalogue = Catalogue::with_scraped(&bytes, &store).unwrap();
        assert_eq!(catalogue.len(), 3, "three places");
        assert_eq!(catalogue.certificates(), 2);
        assert_eq!(catalogue.scraped(), 2);
        assert_eq!(catalogue.record(1), None);
        assert!(catalogue.entry(1).is_none());
        assert_eq!(catalogue.record(2).unwrap().ref_no.as_deref(), Some("B7"));
        let lazy = catalogue.search("lazy", &Filters::default(), 10);
        assert_eq!((lazy.total, lazy.ids), (1, vec![2]));
        assert_eq!(
            names(&catalogue),
            vec![
                ("Momentum".to_owned(), Some(2026), Some("A1".to_owned())),
                ("Lazy".to_owned(), Some(2026), Some("B7".to_owned())),
            ]
        );
    }

    #[test]
    fn a_scraped_certificate_and_its_embedded_twin_are_listed_once() {
        let embedded = [
            boat("NOR 1", "Momentum", 2025),
            boat("NOR 2", "Lazy", 2026),
            boat("NOR 3", "Third", 2026),
        ];
        let bytes = format::encode(
            &Provenance {
                records: 3,
                ..Provenance::default()
            },
            &embedded,
        )
        .unwrap();
        let mut newer = boat("NOR 2", "Lazy", 2026);
        newer.gph = Some(650.0);
        let scrape = vec![
            // Another year's certificate of an embedded boat: both stay.
            scraped("A1", boat("NOR 1", "Momentum", 2026)),
            // The embedded certificate itself: one entry, the scraped data.
            scraped("A2", newer),
            // A boat the embedded catalogue does not have.
            scraped("A4", boat("NOR 4", "Fourth", 2026)),
        ];
        let catalogue = Catalogue::with_scraped(&bytes, &scrape).unwrap();
        assert_eq!(
            names(&catalogue),
            vec![
                ("Momentum".to_owned(), Some(2025), None),
                ("Lazy".to_owned(), Some(2026), Some("A2".to_owned())),
                ("Third".to_owned(), Some(2026), None),
                ("Momentum".to_owned(), Some(2026), Some("A1".to_owned())),
                ("Fourth".to_owned(), Some(2026), Some("A4".to_owned())),
            ]
        );
        assert_eq!(catalogue.record(1).unwrap().gph, Some(650.0));
        assert_eq!(catalogue.scraped(), 3);
        // Found once by search, and the embedded ids are where they were.
        let lazy = catalogue.search("lazy", &Filters::default(), 10);
        assert_eq!(lazy.ids, vec![1]);
        let plain = Catalogue::from_bytes(&bytes).unwrap();
        assert_eq!(plain.scraped(), 0);
        assert_eq!(plain.record(2).unwrap().name, "Third");
        assert_eq!(catalogue.record(2).unwrap().name, "Third");
        assert_eq!(catalogue.record(9), None);
    }

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
