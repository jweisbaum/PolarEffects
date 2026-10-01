//! Conservative model matching. Names are descriptive, never evidence of identity.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default)]
pub struct Profile {
    pub models: BTreeSet<String>,
    builders: BTreeSet<String>,
    lengths: Vec<f64>,
    mmsi: BTreeSet<String>,
    sails: BTreeSet<String>,
}

fn compact(s: &str) -> String {
    pe_orc::fold::compact(s)
}

// These are families/rating rules rather than production models. In particular,
// two IMOCA 60s or Class 40s may be entirely different designs.
fn generic(s: &str) -> bool {
    matches!(
        s,
        "unknown"
            | "other"
            | "na"
            | "none"
            | "custom"
            | "prototype"
            | "monohull"
            | "multihull"
            | "sailboat"
            | "sailingyacht"
            | "yacht"
            | "boat"
            | "cruiser"
            | "racer"
            | "catamaran"
            | "trimaran"
            | "onedesign"
            | "open40"
            | "open60"
            | "class40"
            | "imoca"
            | "imoca60"
            | "tp52"
            | "irc"
            | "orc"
            | "orr"
            | "phrf"
    ) || [
        "irc", "orc", "orr", "phrf", "division", "class", "mono", "multi",
    ]
    .iter()
    .any(|prefix| s.starts_with(prefix))
}

fn maker(s: &str) -> String {
    let mut value = compact(s);
    for suffix in ["yachts", "yacht", "boats", "boat", "sa", "ltd"] {
        if value.ends_with(suffix) && value.len() > suffix.len() + 2 {
            value.truncate(value.len() - suffix.len());
        }
    }
    match value.as_str() {
        "j" | "jboats" => "jboats".into(),
        "nautor" | "nautorswan" => "nautor".into(),
        _ => value,
    }
}
fn known_maker(s: &str) -> bool {
    matches!(
        s,
        "beneteau"
            | "jeanneau"
            | "dufour"
            | "hanse"
            | "bavaria"
            | "jboats"
            | "nautor"
            | "x"
            | "x-yachts"
            | "dehler"
            | "elan"
            | "farr"
            | "hylas"
            | "oyster"
            | "sunseeker"
            | "catalina"
            | "melges"
            | "jpk"
    )
}

pub fn model_key(value: &str) -> Option<String> {
    // Preserve decimal model designations, but ignore separators and accents.
    let folded = pe_orc::fold::fold(value);
    let chars: Vec<_> = folded.chars().collect();
    let key: String = chars
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            (c.is_alphanumeric()
                || (*c == '.'
                    && i > 0
                    && chars[i - 1].is_ascii_digit()
                    && chars.get(i + 1).is_some_and(char::is_ascii_digit)))
            .then_some(*c)
        })
        .collect();
    let key = key
        .strip_prefix("beneteau")
        .filter(|rest| rest.starts_with("first") || rest.starts_with("oceanis"))
        .or_else(|| {
            key.strip_prefix("jeanneau")
                .filter(|rest| rest.starts_with("sunfast") || rest.starts_with("sunodyssey"))
        })
        .or_else(|| {
            key.strip_prefix("nautor")
                .filter(|rest| rest.starts_with("swan"))
        })
        .unwrap_or(&key)
        .to_owned();
    if key.len() < 3
        || !key.chars().any(char::is_alphabetic)
        || generic(&key)
        || known_maker(&maker(&key))
        || (!key.chars().any(|c| c.is_ascii_digit())
            && !matches!(
                key.as_str(),
                "laser"
                    | "optimist"
                    | "dragon"
                    | "star"
                    | "soling"
                    | "yngling"
                    | "contender"
                    | "flyingdutchman"
                    | "folkboat"
            ))
    {
        None
    } else {
        Some(key)
    }
}

impl Profile {
    pub fn from_details(details: &BTreeMap<String, String>) -> Self {
        let mut result = Self::default();
        let units = details
            .iter()
            .find(|(key, _)| {
                matches!(
                    compact(key).as_str(),
                    "units" | "lengthunit" | "lengthunits"
                )
            })
            .map(|(_, v)| compact(v))
            .unwrap_or_default();
        for (key, value) in details {
            let key = compact(key.rsplit('.').next().unwrap_or(key));
            if value.trim().is_empty() {
                continue;
            }
            let builder = ["builder", "make", "manufacturer", "shipyard"]
                .iter()
                .any(|word| {
                    key == *word || key == format!("boat{word}") || key == format!("vessel{word}")
                });
            let model = ["model", "class", "type", "design"].iter().any(|word| {
                key == *word || key == format!("boat{word}") || key == format!("vessel{word}")
            });
            if model || builder {
                for brand in ["beneteau", "jeanneau", "nautor"] {
                    if compact(value).starts_with(brand) {
                        result.builders.insert(brand.into());
                    }
                }
                if let Some(candidate) = model_key(value) {
                    // A builder label can contain the model; a maker alone cannot.
                    if model || candidate.chars().any(|c| c.is_ascii_digit()) {
                        result.models.insert(candidate);
                    } else {
                        result.builders.insert(maker(value));
                    }
                } else if builder && !generic(&compact(value)) {
                    result.builders.insert(maker(value));
                }
            }
            if key.contains("mmsi") {
                let value = compact(value);
                if value.len() == 9
                    && value.bytes().all(|c| c.is_ascii_digit())
                    && !value.bytes().all(|c| c == b'0')
                {
                    result.mmsi.insert(value);
                }
            }
            if matches!(key.as_str(), "sail" | "sailno" | "sailnumber") {
                let sail = compact(value);
                if sail.len() >= 4
                    && sail.chars().any(|c| c.is_ascii_digit())
                    && sail.chars().any(char::is_alphabetic)
                {
                    result.sails.insert(sail);
                }
            }
            if matches!(
                key.as_str(),
                "loa"
                    | "loam"
                    | "loameters"
                    | "length"
                    | "lengthm"
                    | "lengthmeters"
                    | "lengthoverall"
                    | "lengthft"
                    | "lengthfeet"
            ) {
                let raw = value.trim().replace(',', ".");
                let number: String = raw
                    .chars()
                    .take_while(|c| c.is_ascii_digit() || *c == '.')
                    .collect();
                if let Ok(mut n) = number.parse::<f64>() {
                    let lower = raw.to_lowercase();
                    let feet = key.ends_with("ft")
                        || key.ends_with("feet")
                        || lower.contains("ft")
                        || lower.contains("feet")
                        || lower.contains('\'')
                        || matches!(units.as_str(), "ft" | "feet" | "imperial");
                    let metres = key != "length"
                        || lower.ends_with('m')
                        || matches!(units.as_str(), "m" | "meters" | "metres" | "metric");
                    if feet {
                        n *= 0.3048;
                    }
                    // Unlabelled 'length' is ambiguous across trackers; retain it
                    // in metadata, but do not invent units for a rejection.
                    if (feet || metres) && (1.0..200.0).contains(&n) {
                        result.lengths.push(n);
                    }
                }
            }
        }
        result
    }
    fn compatible_specs(&self, other: &Self) -> bool {
        self.builders.len() <= 1
            && other.builders.len() <= 1
            && (self.builders.is_empty()
                || other.builders.is_empty()
                || !self.builders.is_disjoint(&other.builders))
            && self.lengths.iter().all(|a| {
                other
                    .lengths
                    .iter()
                    .all(|b| (a - b).abs() <= 0.6_f64.max(a.min(*b) * 0.05))
            })
    }
    pub fn same_model(&self, other: &Self) -> bool {
        self.models.len() == 1 && self.models == other.models && self.compatible_specs(other)
    }
    /// Identity can supply missing model data; neither names nor a bare sail
    /// number can establish identity. Different MMSIs are fine for sister ships.
    pub fn same_vessel(&self, other: &Self) -> bool {
        self.compatible_specs(other)
            && self.models.len() <= 1
            && other.models.len() <= 1
            && (self.models.is_empty() || other.models.is_empty() || self.models == other.models)
            && self.mmsi.len() <= 1
            && other.mmsi.len() <= 1
            && (self.mmsi.is_empty() || other.mmsi.is_empty() || self.mmsi == other.mmsi)
            && (!self.mmsi.is_disjoint(&other.mmsi)
                || (!self.sails.is_disjoint(&other.sails)
                    && !self.builders.is_empty()
                    && !other.builders.is_empty()
                    && !self.lengths.is_empty()
                    && !other.lengths.is_empty()))
    }
    pub fn resolve_model(&self, candidates: &[Profile]) -> Self {
        if !self.models.is_empty() {
            return self.clone();
        }
        let mut models = BTreeSet::new();
        for candidate in candidates
            .iter()
            .filter(|p| !p.models.is_empty() && self.same_vessel(p))
        {
            models.extend(candidate.models.iter().cloned());
        }
        let mut resolved = self.clone();
        if models.len() == 1 {
            resolved.models = models;
        }
        resolved
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(fields: &[(&str, &str)]) -> Profile {
        Profile::from_details(
            &fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        )
    }
    #[test]
    fn ambiguous_models_and_empty_sail_identifiers_cannot_resolve_identity() {
        let ambiguous = p(&[("model", "J109"), ("class", "J111")]);
        assert!(!ambiguous.same_model(&ambiguous));
        let unknown = p(&[("sail", "-"), ("builder", "Beneteau"), ("loa", "12")]);
        let other = p(&[
            ("sail", "-"),
            ("builder", "Beneteau"),
            ("loa", "12"),
            ("model", "First40.7"),
        ]);
        assert!(unknown.resolve_model(&[other]).models.is_empty());
        assert!(
            !p(&[("model", "Beneteau First40.7")])
                .same_model(&p(&[("model", "First40.7"), ("builder", "Dufour")]))
        );
    }
    #[test]
    fn names_are_never_matching_evidence() {
        let a = p(&[("name", "Whirlwind"), ("teamName", "Sunshine")]);
        let b = p(&[("name", "Whirlwind"), ("model", "J/109")]);
        assert!(!a.same_model(&a));
        assert!(!a.same_model(&b));
        assert!(!a.same_vessel(&b));
        assert!(a.resolve_model(&[b]).models.is_empty());
    }
    #[test]
    fn exact_boats_need_identity_and_cannot_override_conflicting_specs() {
        let a = p(&[("model", "J109"), ("mmsi", "123456789")]);
        assert!(a.same_vessel(&p(&[("mmsi", "123456789")])));
        assert!(!a.same_vessel(&p(&[("model", "J109"), ("mmsi", "987654321")])));
        assert!(!a.same_vessel(&p(&[("model", "J111"), ("mmsi", "123456789")])));
        assert!(!a.same_vessel(&p(&[("model", "J109")])));
        let sail = p(&[("sail", "GBR1234"), ("builder", "J Boats"), ("loa", "10.9")]);
        assert!(sail.same_vessel(&sail));
        assert!(!sail.same_vessel(&p(&[("sail", "GBR1234"), ("builder", "J Boats")])));
        assert!(!sail.same_vessel(&p(&[
            ("sail", "GBR1234"),
            ("builder", "J Boats"),
            ("loa", "15")
        ])));
    }
    #[test]
    fn synonyms_and_sister_ships_match_without_identical_identity() {
        let a = p(&[
            ("model", "Bénéteau First 40.7"),
            ("mmsi", "123456789"),
            ("name", "Alpha"),
        ]);
        let b = p(&[
            ("builder", "First40.7"),
            ("mmsi", "987654321"),
            ("teamName", "Beta"),
        ]);
        assert!(a.same_model(&b));
        assert!(p(&[("class", "J/109")]).same_model(&p(&[("type", "J 109")])));
    }
    #[test]
    fn broad_classes_and_builders_do_not_identify_a_model() {
        for value in [
            "IRC 2",
            "Class 40",
            "IMOCA 60",
            "TP52",
            "Monohull",
            "Beneteau",
            "Oceanis",
            "Super Zero",
            "Racer/Cruiser",
        ] {
            let a = p(&[("class", value)]);
            assert!(!a.same_model(&a), "{value}");
        }
    }
    #[test]
    fn conflicting_model_numbers_builders_and_lengths_reject() {
        for (a, b) in [
            ("First40", "First40.7"),
            ("J109", "J111"),
            ("J109", "J109E"),
        ] {
            assert!(!p(&[("model", a)]).same_model(&p(&[("model", b)])));
        }
        assert!(
            !p(&[("model", "Custom 40"), ("builder", "Acme")])
                .same_model(&p(&[("model", "Custom40"), ("builder", "Other Yard")]))
        );
        assert!(
            !p(&[("model", "J109"), ("loa", "10.75")])
                .same_model(&p(&[("model", "J109"), ("loa", "14")]))
        );
        assert!(
            p(&[("model", "J109"), ("loa", "10.75")])
                .same_model(&p(&[("model", "J109"), ("length", "35.25 ft")]))
        );
    }
    #[test]
    fn identity_can_recover_one_unambiguous_model() {
        let target = p(&[("mmsi", "123456789"), ("name", "Different Name")]);
        let a = p(&[("mmsi", "123456789"), ("model", "J109")]);
        assert!(
            target
                .resolve_model(std::slice::from_ref(&a))
                .same_model(&a)
        );
        let b = p(&[("mmsi", "123456789"), ("model", "J111")]);
        assert!(target.resolve_model(&[a, b]).models.is_empty());

        // A reused sail number cannot override a known identity conflict.
        let target = p(&[
            ("name", "Same name"),
            ("mmsi", "123456789"),
            ("sailNumber", "USA1234"),
            ("builder", "J Boats"),
            ("loa", "10.75"),
        ]);
        let other = p(&[
            ("name", "Same name"),
            ("mmsi", "987654321"),
            ("sailNumber", "USA1234"),
            ("builder", "J Boats"),
            ("loa", "10.75"),
            ("model", "J109"),
        ]);
        assert!(target.resolve_model(&[other]).models.is_empty());
    }
}
