//! The search index over the catalogue (spec.md 5.2).
//!
//! One box searches every field. The query is split into words, and a record
//! matches when **every** word starts some word of its boat name, sail number,
//! country, model, builder, designer, year built or certificate year. Words
//! are folded (case and accents), and the sail number is also indexed with its
//! separators removed, so `GBR1124`, `GBR 1124` and `GBR/1124` all find it.
//!
//! Ranking, best first: the sail number exactly, then the name starting with
//! the query, then the model starting with it, then any other match; within
//! each, newer certificates first, then by name.
//!
//! The index is built once, when the catalogue is first used. A search is a
//! linear scan of one pre-folded string per record, which over the ~18,000
//! records stays well inside the 30 ms per keystroke of spec.md 13.

use crate::fold::{compact, fold, words};
use crate::format::Entry;

/// What the result list is narrowed to, besides the query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filters {
    /// Earliest year built, inclusive.
    pub year_min: Option<i32>,
    /// Latest year built, inclusive.
    pub year_max: Option<i32>,
    /// Only this country's certificates, by three-letter code.
    pub country: Option<String>,
}

impl Filters {
    fn is_empty(&self) -> bool {
        self.year_min.is_none() && self.year_max.is_none() && self.country.is_none()
    }

    fn admits(&self, entry: &Entry) -> bool {
        if let Some(country) = &self.country
            && !entry.country.eq_ignore_ascii_case(country)
        {
            return false;
        }
        if self.year_min.is_some() || self.year_max.is_some() {
            let Some(year) = entry.year else {
                return false;
            };
            if self.year_min.is_some_and(|min| year < min)
                || self.year_max.is_some_and(|max| year > max)
            {
                return false;
            }
        }
        true
    }
}

/// A search's answer: how many records matched, and the best `limit` of them
/// as catalogue ids, best first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Hits {
    /// Every record that matched.
    pub total: usize,
    /// The best of them, in rank order.
    pub ids: Vec<u32>,
}

/// Precomputed search keys of one record.
#[derive(Debug)]
struct Keys {
    /// Every word of every searched field, each preceded by a space, so
    /// `" word"` finds a word start with one substring search.
    hay: String,
    /// The name's words, joined by single spaces.
    name: String,
    /// The model's words, joined by single spaces.
    model: String,
    /// The sail number without separators, country included.
    sail: String,
    /// The sail number without separators or its country prefix.
    sail_number: String,
    /// Place in the order of newer certificates first, then name.
    order: u32,
}

/// Ranking tiers (spec.md 5.2), best first.
const EXACT_SAIL: u8 = 0;
const NAME_PREFIX: u8 = 1;
const MODEL_PREFIX: u8 = 2;
const OTHER: u8 = 3;

/// The search index over a catalogue's entries.
#[derive(Debug)]
pub struct Index {
    keys: Vec<Keys>,
}

fn joined(text: Option<&str>) -> String {
    text.map(|t| words(t).join(" ")).unwrap_or_default()
}

impl Index {
    /// Builds the index. `entries[i]` is catalogue id `i`.
    pub fn new(entries: &[Entry]) -> Self {
        let mut by_age: Vec<usize> = (0..entries.len()).collect();
        let names: Vec<String> = entries.iter().map(|e| fold(&e.name)).collect();
        by_age.sort_by(|&a, &b| {
            entries[b]
                .certificate_year
                .cmp(&entries[a].certificate_year)
                .then_with(|| names[a].cmp(&names[b]))
                .then(a.cmp(&b))
        });
        let mut order = vec![0u32; entries.len()];
        for (rank, &i) in by_age.iter().enumerate() {
            order[i] = u32::try_from(rank).unwrap_or(u32::MAX);
        }

        let keys = entries
            .iter()
            .zip(order)
            .map(|(entry, order)| {
                let sail = compact(&entry.sail_no);
                let country = compact(&entry.country);
                let sail_number = sail
                    .strip_prefix(country.as_str())
                    .filter(|rest| !rest.is_empty())
                    .unwrap_or(&sail)
                    .to_owned();
                let mut hay = String::new();
                let fields = [
                    Some(entry.name.as_str()),
                    Some(entry.sail_no.as_str()),
                    Some(entry.country.as_str()),
                    entry.model.as_deref(),
                    entry.builder.as_deref(),
                    entry.designer.as_deref(),
                ];
                for field in fields.into_iter().flatten() {
                    for word in words(field) {
                        hay.push(' ');
                        hay.push_str(&word);
                    }
                }
                // The name without its punctuation too, so `oneil` finds
                // O'Neil and `xrated` finds X-Rated.
                let name = compact(&entry.name);
                for extra in [&sail, &sail_number, &name] {
                    if !extra.is_empty() {
                        hay.push(' ');
                        hay.push_str(extra);
                    }
                }
                for year in [entry.year, entry.certificate_year].into_iter().flatten() {
                    hay.push(' ');
                    hay.push_str(&year.to_string());
                }
                Keys {
                    hay,
                    name: joined(Some(&entry.name)),
                    model: joined(entry.model.as_deref()),
                    sail,
                    sail_number,
                    order,
                }
            })
            .collect();
        Self { keys }
    }

    /// The best `limit` records for `query` within `filters`. An empty query
    /// with no filter finds nothing; with a filter, it lists what the filter
    /// admits, newest certificates first.
    pub fn search(&self, entries: &[Entry], query: &str, filters: &Filters, limit: usize) -> Hits {
        let query_words = words(query);
        if query_words.is_empty() && filters.is_empty() {
            return Hits::default();
        }
        let needles: Vec<String> = query_words.iter().map(|w| format!(" {w}")).collect();
        let phrase = query_words.join(" ");
        let whole = compact(query);

        let mut ranked: Vec<(u8, u32, u32)> = Vec::new();
        for (id, (keys, entry)) in self.keys.iter().zip(entries).enumerate() {
            if !needles
                .iter()
                .all(|needle| keys.hay.contains(needle.as_str()))
            {
                continue;
            }
            if !filters.admits(entry) {
                continue;
            }
            let tier = if !whole.is_empty() && (keys.sail == whole || keys.sail_number == whole) {
                EXACT_SAIL
            } else if !phrase.is_empty() && keys.name.starts_with(&phrase) {
                NAME_PREFIX
            } else if !phrase.is_empty() && keys.model.starts_with(&phrase) {
                MODEL_PREFIX
            } else {
                OTHER
            };
            ranked.push((tier, keys.order, u32::try_from(id).unwrap_or(u32::MAX)));
        }
        let total = ranked.len();
        if ranked.len() > limit {
            ranked.select_nth_unstable(limit);
            ranked.truncate(limit);
        }
        ranked.sort_unstable();
        Hits {
            total,
            ids: ranked.into_iter().map(|(_, _, id)| id).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boat(sail: &str, country: &str, name: &str, model: &str, year: i32, cert: i32) -> Entry {
        Entry {
            sail_no: sail.to_owned(),
            country: country.to_owned(),
            name: name.to_owned(),
            model: Some(model.to_owned()),
            builder: Some("Carroll Marine".to_owned()),
            year: Some(year),
            certificate_year: Some(cert),
            ..Entry::default()
        }
    }

    fn fleet() -> Vec<Entry> {
        vec![
            boat("GBR 1124", "GBR", "Jiminy", "Farr 40", 1998, 2022), // 0
            boat("USA 40", "USA", "Farrago", "J/109", 2004, 2024),    // 1
            boat("ITA 7", "ITA", "Fröken", "Farr 40", 2001, 2023),    // 2
            boat("GER 4011", "GER", "Farr Out", "Farr 40", 2000, 2025), // 3
            boat("GBR 11240", "GBR", "Other", "First 40.7", 2023, 2024), // 4
        ]
    }

    fn ids(index: &Index, entries: &[Entry], query: &str, filters: &Filters) -> Vec<u32> {
        index.search(entries, query, filters, 50).ids
    }

    #[test]
    fn every_word_must_start_a_word_of_some_field() {
        let entries = fleet();
        let index = Index::new(&entries);
        let none = Filters::default();
        // "farr 40 2023": Farr 40s with a 2023 certificate or built in 2023.
        assert_eq!(ids(&index, &entries, "farr 40 2023", &none), vec![2]);
        // Words match across fields, in any order.
        assert_eq!(ids(&index, &entries, "2001 fröken ita", &none), vec![2]);
        // A word inside another word does not match: "arr" is not a word start.
        assert!(ids(&index, &entries, "arr", &none).is_empty());
        assert!(ids(&index, &entries, "   ", &none).is_empty());
    }

    #[test]
    fn case_accents_and_sail_separators_do_not_matter() {
        let entries = fleet();
        let index = Index::new(&entries);
        let none = Filters::default();
        assert_eq!(ids(&index, &entries, "FROKEN", &none), vec![2]);
        assert_eq!(ids(&index, &entries, "fro\u{308}ken", &none), vec![2]);
        for query in ["GBR1124", "GBR 1124", "GBR/1124", "gbr-1124"] {
            assert_eq!(ids(&index, &entries, query, &none)[0], 0, "{query}");
        }
    }

    #[test]
    fn a_name_is_found_without_its_punctuation() {
        let mut entries = fleet();
        entries.push(boat("TUR 1", "TUR", "O'Neil İstanbul", "X-35", 2010, 2024)); // 5
        let index = Index::new(&entries);
        let none = Filters::default();
        assert_eq!(ids(&index, &entries, "oneil", &none), vec![5]);
        assert_eq!(ids(&index, &entries, "o'neil", &none), vec![5]);
        assert_eq!(ids(&index, &entries, "istanbul", &none), vec![5]);
        assert_eq!(ids(&index, &entries, "İSTANBUL", &none), vec![5]);
    }

    #[test]
    fn ranking_follows_the_spec() {
        let entries = fleet();
        let index = Index::new(&entries);
        let none = Filters::default();
        // "gbr 1124" also prefixes GBR 11240; the exact sail number wins.
        assert_eq!(ids(&index, &entries, "gbr 1124", &none), vec![0, 4]);
        // "40": the sail number USA 40 exactly, then everything else with a
        // word starting "40", newest certificate first.
        assert_eq!(ids(&index, &entries, "40", &none), vec![1, 3, 4, 2, 0]);
        // "farr": name prefixes (Farr Out, Farrago), then model prefixes, each
        // newest first.
        assert_eq!(ids(&index, &entries, "farr", &none), vec![3, 1, 2, 0]);
    }

    #[test]
    fn filters_narrow_by_year_built_and_country() {
        let entries = fleet();
        let index = Index::new(&entries);
        let gbr = Filters {
            country: Some("gbr".to_owned()),
            ..Filters::default()
        };
        assert_eq!(ids(&index, &entries, "", &gbr), vec![4, 0]);
        let years = Filters {
            year_min: Some(2000),
            year_max: Some(2004),
            ..Filters::default()
        };
        assert_eq!(ids(&index, &entries, "farr", &years), vec![3, 1, 2]);
        let hits = index.search(&entries, "farr", &years, 1);
        assert_eq!((hits.total, hits.ids), (3, vec![3]));
    }
}
