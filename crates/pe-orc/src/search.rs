//! The search index over the catalogue (spec.md 5.2).
//!
//! One box searches every field. The query is split into words, and a record
//! matches when **every** word starts some word of its boat name, sail number,
//! country, model, builder, designer, year built or certificate year. Words
//! are folded (case and accents), and the sail number is also indexed with its
//! separators removed, so `GBR1124`, `GBR 1124` and `GBR/1124` all find it.
//!
//! Besides that box, each field can be searched on its own ([`Fields`]):
//! every word of a field query must start a word of **that** field, with the
//! same folding and compact forms, and every field query must match.
//!
//! Ranking, best first: a field query equal to its whole field, then the sail
//! number exactly, then the name starting with the query, then the model
//! starting with it, then any other match; within each, newer certificates
//! first, then by name.
//!
//! The index is built once, when the catalogue is first used. A search is a
//! linear scan of one pre-folded string per record, which over the ~18,000
//! records stays well inside the 30 ms per keystroke of spec.md 13.

use crate::fold::{compact, fold, words};
use crate::format::Entry;

/// Queries on one field each (spec.md 5.2): the "Search by field" boxes.
/// Empty means no condition on that field.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields {
    /// Boat name.
    pub name: String,
    /// Sail number, with or without its country.
    pub sail_no: String,
    /// Type or model.
    pub model: String,
    /// Builder.
    pub builder: String,
    /// Designer.
    pub designer: String,
    /// Certificate year, matched from its start: `202` finds 2020 to 2029.
    pub certificate_year: String,
}

/// The per-field search keys, in this order.
const NAME: usize = 0;
const SAIL: usize = 1;
const MODEL: usize = 2;
const BUILDER: usize = 3;
const DESIGNER: usize = 4;
const CERTIFICATE_YEAR: usize = 5;
const FIELDS: usize = 6;

impl Fields {
    fn queries(&self) -> [&str; FIELDS] {
        let mut out = [""; FIELDS];
        out[NAME] = &self.name;
        out[SAIL] = &self.sail_no;
        out[MODEL] = &self.model;
        out[BUILDER] = &self.builder;
        out[DESIGNER] = &self.designer;
        out[CERTIFICATE_YEAR] = &self.certificate_year;
        out
    }
}

/// One field query, prepared once per search.
struct FieldQuery {
    field: usize,
    /// `" word"` for each word, as for the all-fields box.
    needles: Vec<String>,
    /// The query without separators, for the exact-field tier.
    whole: String,
}

/// What the result list is narrowed to, besides the query.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filters {
    /// Earliest year built, inclusive.
    pub year_min: Option<i32>,
    /// Latest year built, inclusive.
    pub year_max: Option<i32>,
    /// Only this country's certificates, by three-letter code.
    pub country: Option<String>,
    /// Queries on single fields.
    pub fields: Fields,
    /// Inclusive numeric minima in OrcSize order; SI units (metres, kg, m²).
    pub size_min: [Option<f64>; 9],
    /// Inclusive maxima in the same order; missing measured values never pass.
    pub size_max: [Option<f64>; 9],
}

impl Filters {
    /// Whether nothing narrows the list, ignoring the field queries (which
    /// the search prepares, and counts only when they hold a word).
    fn is_empty(&self) -> bool {
        self.year_min.is_none()
            && self.year_max.is_none()
            && self.country.is_none()
            && self
                .size_min
                .iter()
                .chain(&self.size_max)
                .all(Option::is_none)
    }

    fn admits(&self, entry: &Entry) -> bool {
        for (k, value) in entry.size.iter().enumerate() {
            let (min, max) = (self.size_min[k], self.size_max[k]);
            if (min.is_some() || max.is_some())
                && value.is_none_or(|v| {
                    !v.is_finite() || min.is_some_and(|m| v < m) || max.is_some_and(|m| v > m)
                })
            {
                return false;
            }
        }
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
    /// Per field (`NAME`…`CERTIFICATE_YEAR`): its words, each preceded by a
    /// space, plus its compact form where it has one.
    fields: [String; FIELDS],
    /// Per field: the whole field without separators, for the exact tier.
    wholes: [String; FIELDS],
    /// Place in the order of newer certificates first, then name.
    order: u32,
}

/// Ranking tiers (spec.md 5.2), best first.
const EXACT_FIELD: u8 = 0;
const EXACT_SAIL: u8 = 1;
const NAME_PREFIX: u8 = 2;
const MODEL_PREFIX: u8 = 3;
const OTHER: u8 = 4;

/// The search index over a catalogue's entries.
#[derive(Debug)]
pub struct Index {
    keys: Vec<Keys>,
    /// The ids no search finds: certificates that keep a place in the
    /// catalogue but are no longer in it (see [`Self::hide`]).
    hidden: Vec<bool>,
}

fn joined(text: Option<&str>) -> String {
    text.map(|t| words(t).join(" ")).unwrap_or_default()
}

/// `text`'s words, each preceded by a space, then each of `extra` that is
/// not empty and not already one of them.
fn haystack(text: &str, extra: &[&str]) -> String {
    let mut hay = String::new();
    let own = words(text);
    for word in &own {
        hay.push(' ');
        hay.push_str(word);
    }
    for more in extra {
        if !more.is_empty() && !own.iter().any(|w| w == more) {
            hay.push(' ');
            hay.push_str(more);
        }
    }
    hay
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

        let keys: Vec<Keys> = entries
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
                let model = entry.model.as_deref().unwrap_or("");
                let builder = entry.builder.as_deref().unwrap_or("");
                let designer = entry.designer.as_deref().unwrap_or("");
                let certificate = entry
                    .certificate_year
                    .map(|y| y.to_string())
                    .unwrap_or_default();
                let (model_c, builder_c, designer_c) =
                    (compact(model), compact(builder), compact(designer));
                let fields = [
                    haystack(&entry.name, &[&name]),
                    haystack(&entry.sail_no, &[&sail, &sail_number]),
                    haystack(model, &[&model_c]),
                    haystack(builder, &[&builder_c]),
                    haystack(designer, &[&designer_c]),
                    haystack(&certificate, &[]),
                ];
                let wholes = [
                    name,
                    sail.clone(),
                    model_c,
                    builder_c,
                    designer_c,
                    certificate,
                ];
                Keys {
                    hay,
                    name: joined(Some(&entry.name)),
                    model: joined(entry.model.as_deref()),
                    sail,
                    sail_number,
                    fields,
                    wholes,
                    order,
                }
            })
            .collect();
        let hidden = vec![false; keys.len()];
        Self { keys, hidden }
    }

    /// Takes an id out of every search. Its place stays, so the ids after
    /// it are unchanged.
    pub fn hide(&mut self, id: usize) {
        if let Some(hidden) = self.hidden.get_mut(id) {
            *hidden = true;
        }
    }

    /// The best `limit` records for `query` within `filters`. An empty query
    /// with no filter or field query finds nothing; with one, it lists what
    /// they admit, newest certificates first.
    pub fn search(&self, entries: &[Entry], query: &str, filters: &Filters, limit: usize) -> Hits {
        let query_words = words(query);
        let field_queries: Vec<FieldQuery> = filters
            .fields
            .queries()
            .into_iter()
            .enumerate()
            .filter_map(|(field, text)| {
                let needles: Vec<String> = words(text).iter().map(|w| format!(" {w}")).collect();
                (!needles.is_empty()).then(|| FieldQuery {
                    field,
                    needles,
                    whole: compact(text),
                })
            })
            .collect();
        if query_words.is_empty() && field_queries.is_empty() && filters.is_empty() {
            return Hits::default();
        }
        let needles: Vec<String> = query_words.iter().map(|w| format!(" {w}")).collect();
        let phrase = query_words.join(" ");
        let whole = compact(query);

        let mut ranked: Vec<(u8, u32, u32)> = Vec::new();
        for (id, (keys, entry)) in self.keys.iter().zip(entries).enumerate() {
            if self.hidden[id] {
                continue;
            }
            if !needles
                .iter()
                .all(|needle| keys.hay.contains(needle.as_str()))
            {
                continue;
            }
            if !field_queries.iter().all(|q| {
                let hay = &keys.fields[q.field];
                q.needles.iter().all(|needle| hay.contains(needle.as_str()))
            }) {
                continue;
            }
            if !filters.admits(entry) {
                continue;
            }
            let exact_field = field_queries.iter().any(|q| {
                keys.wholes[q.field] == q.whole || (q.field == SAIL && keys.sail_number == q.whole)
            });
            let tier = if exact_field {
                EXACT_FIELD
            } else if !whole.is_empty() && (keys.sail == whole || keys.sail_number == whole) {
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
    fn measurement_only_queries_are_inclusive_and_missing_values_do_not_pass() {
        let mut entries = fleet();
        entries[0].size[0] = Some(12.0);
        entries[1].size[0] = Some(12.0);
        entries[2].size[0] = Some(11.9);
        entries[0].size[3] = Some(7000.0);
        entries[1].size[3] = Some(8000.0);
        let index = Index::new(&entries);
        let mut filters = Filters::default();
        filters.size_min[0] = Some(12.0);
        filters.size_max[0] = Some(12.0);
        let found = ids(&index, &entries, "", &filters);
        assert_eq!(found.len(), 2);
        assert!(found.contains(&0) && found.contains(&1));
        filters.size_max[3] = Some(7000.0);
        assert_eq!(ids(&index, &entries, "", &filters), vec![0]);
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

    fn by_field(fields: Fields) -> Filters {
        Filters {
            fields,
            ..Filters::default()
        }
    }

    /// The fleet, with designers and builders that share words with other
    /// fields, so a match in the wrong field would show.
    fn designed() -> Vec<Entry> {
        let mut entries = fleet();
        entries[0].designer = Some("Bruce Farr".to_owned());
        entries[1].designer = Some("Rod Johnstone".to_owned());
        entries[1].builder = Some("J/Boats".to_owned());
        entries[2].designer = Some("Farr Yacht Design".to_owned());
        entries[3].designer = None;
        entries[4].designer = Some("Berret-Racoupeau".to_owned());
        entries[4].builder = Some("Bénéteau".to_owned());
        entries.push(boat("TUR 1", "TUR", "O'Neil İstanbul", "X-35", 2010, 2024)); // 5
        entries
    }

    #[test]
    fn a_field_query_matches_word_starts_of_that_field_only() {
        let entries = designed();
        let index = Index::new(&entries);
        let name = |q: &str| {
            by_field(Fields {
                name: q.to_owned(),
                ..Fields::default()
            })
        };
        // "farr" starts the names Farrago and Farr Out; the Farr 40 models and
        // Bruce Farr the designer do not count in the name field.
        assert_eq!(ids(&index, &entries, "", &name("farr")), vec![3, 1]);
        // Word starts only: "arr" is inside a word.
        assert!(ids(&index, &entries, "", &name("arr")).is_empty());
        // Tokens are ANDed within the field.
        assert_eq!(ids(&index, &entries, "", &name("out farr")), vec![3]);
        assert!(ids(&index, &entries, "", &name("farr jiminy")).is_empty());
        let designer = |q: &str| {
            by_field(Fields {
                designer: q.to_owned(),
                ..Fields::default()
            })
        };
        assert_eq!(ids(&index, &entries, "", &designer("farr")), vec![2, 0]);
        assert_eq!(ids(&index, &entries, "", &designer("jOhN")), vec![1]);
        let model = |q: &str| {
            by_field(Fields {
                model: q.to_owned(),
                ..Fields::default()
            })
        };
        assert_eq!(ids(&index, &entries, "", &model("farr 40")), vec![3, 2, 0]);
        assert_eq!(ids(&index, &entries, "", &model("j/109")), vec![1]);
        assert_eq!(ids(&index, &entries, "", &model("j109")), vec![1]);
        assert!(ids(&index, &entries, "", &model("jiminy")).is_empty());
        let builder = |q: &str| {
            by_field(Fields {
                builder: q.to_owned(),
                ..Fields::default()
            })
        };
        assert_eq!(ids(&index, &entries, "", &builder("j")), vec![1]);
        // Punctuation-only queries are no condition, and so find nothing alone.
        assert!(ids(&index, &entries, "", &builder(" / ")).is_empty());
    }

    #[test]
    fn a_field_query_folds_case_and_accents_and_accepts_compact_forms() {
        let entries = designed();
        let index = Index::new(&entries);
        let builder = |q: &str| {
            by_field(Fields {
                builder: q.to_owned(),
                ..Fields::default()
            })
        };
        for query in ["beneteau", "BENETEAU", "Bénéteau", "be\u{301}ne\u{301}teau"] {
            assert_eq!(
                ids(&index, &entries, "", &builder(query)),
                vec![4],
                "{query}"
            );
        }
        let name = |q: &str| {
            by_field(Fields {
                name: q.to_owned(),
                ..Fields::default()
            })
        };
        for query in [
            "oneil",
            "o'neil",
            "O NEIL",
            "istanbul",
            "İSTANBUL",
            "fröken",
            "froken",
        ] {
            assert_eq!(ids(&index, &entries, "", &name(query)).len(), 1, "{query}");
        }
        let sail = |q: &str| {
            by_field(Fields {
                sail_no: q.to_owned(),
                ..Fields::default()
            })
        };
        for query in ["GBR1124", "GBR 1124", "GBR/1124", "gbr-1124", "1124"] {
            // GBR 11240 starts with it too, but the exact number comes first.
            assert_eq!(
                ids(&index, &entries, "", &sail(query)),
                vec![0, 4],
                "{query}"
            );
        }
        // The country alone in the sail field is a word of the sail number.
        assert_eq!(ids(&index, &entries, "", &sail("ita")), vec![2]);
        // Not a word of any other field: "farr" is no sail number.
        assert!(ids(&index, &entries, "", &sail("farr")).is_empty());
        let designer = |q: &str| {
            by_field(Fields {
                designer: q.to_owned(),
                ..Fields::default()
            })
        };
        assert_eq!(
            ids(&index, &entries, "", &designer("berretracoupeau")),
            vec![4]
        );
    }

    #[test]
    fn every_field_query_the_box_and_the_filters_must_all_match() {
        let entries = designed();
        let index = Index::new(&entries);
        let farr_models = Fields {
            model: "farr".to_owned(),
            ..Fields::default()
        };
        // Model Farr and designer Farr: Jiminy and Fröken, not Farr Out.
        let both = by_field(Fields {
            designer: "farr".to_owned(),
            ..farr_models.clone()
        });
        assert_eq!(ids(&index, &entries, "", &both), vec![2, 0]);
        // ... and the all-fields box as well.
        assert_eq!(ids(&index, &entries, "ita", &both), vec![2]);
        assert!(ids(&index, &entries, "usa", &both).is_empty());
        // ... and the year and country filters.
        let built = Filters {
            year_min: Some(2000),
            fields: both.fields.clone(),
            ..Filters::default()
        };
        assert_eq!(ids(&index, &entries, "", &built), vec![2]);
        let country = Filters {
            country: Some("GBR".to_owned()),
            fields: both.fields.clone(),
            ..Filters::default()
        };
        assert_eq!(ids(&index, &entries, "", &country), vec![0]);
    }

    #[test]
    fn year_built_ranges_and_the_certificate_year_field() {
        let entries = designed();
        let index = Index::new(&entries);
        let built = |min: Option<i32>, max: Option<i32>| Filters {
            year_min: min,
            year_max: max,
            ..Filters::default()
        };
        assert_eq!(
            ids(&index, &entries, "", &built(Some(2001), Some(2001))),
            vec![2]
        );
        assert_eq!(
            ids(&index, &entries, "", &built(Some(2004), None)),
            vec![1, 5, 4]
        );
        assert_eq!(ids(&index, &entries, "", &built(None, Some(1999))), vec![0]);
        assert!(ids(&index, &entries, "", &built(Some(2005), Some(2004))).is_empty());
        let cert = |q: &str| {
            by_field(Fields {
                certificate_year: q.to_owned(),
                ..Fields::default()
            })
        };
        assert_eq!(ids(&index, &entries, "", &cert("2024")), vec![1, 5, 4]);
        // From its start, as a word: "202" is 2020 to 2029, "024" nothing.
        assert_eq!(ids(&index, &entries, "", &cert("202")).len(), 6);
        assert!(ids(&index, &entries, "", &cert("024")).is_empty());
        // The certificate year is not the year built: none was built in 2024.
        assert!(ids(&index, &entries, "", &built(Some(2024), Some(2024))).is_empty());
        let with_built = Filters {
            year_min: Some(2005),
            ..cert("2024")
        };
        assert_eq!(ids(&index, &entries, "", &with_built), vec![5, 4]);
    }

    #[test]
    fn an_exact_field_match_ranks_first() {
        let mut entries = designed();
        // A boat called exactly Farr, with the oldest certificate of all.
        entries.push(boat("NZL 1", "NZL", "Farr", "Farr 40", 1990, 2010)); // 6
        entries.push(boat("AUS 2", "AUS", "Zed", "Farr 40 OD", 2020, 2026)); // 7
        let index = Index::new(&entries);
        let name = by_field(Fields {
            name: "farr".to_owned(),
            ..Fields::default()
        });
        assert_eq!(ids(&index, &entries, "", &name), vec![6, 3, 1]);
        // Model "farr 40" equals four models exactly; they come before the
        // prefix-only "Farr 40 OD", although its certificate is the newest.
        let model = by_field(Fields {
            model: "farr 40".to_owned(),
            ..Fields::default()
        });
        assert_eq!(ids(&index, &entries, "", &model), vec![3, 2, 0, 6, 7]);
        // Below the exact field, the box's own tiers still apply: with "40"
        // in the box, USA 40's exact sail number comes before Farr Out.
        assert_eq!(ids(&index, &entries, "40", &name), vec![6, 1, 3]);
    }
}
