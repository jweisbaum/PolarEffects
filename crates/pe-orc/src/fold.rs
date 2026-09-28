//! Case and accent folding for the search (spec.md 5.2).
//!
//! Boat names come from every sailing nation: `Fröken`, `Ñandú`, `Kåre`.
//! Someone typing on an English keyboard types `froken`, and must find it.
//! Folding lower-cases and drops diacritics from the Latin letters the
//! catalogue actually holds; anything else passes through lower-cased.

/// The ASCII spelling of a folded Latin letter with a diacritic, if any.
fn plain(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'ď' | 'đ' | 'ð' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'ĥ' | 'ħ' => "h",
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => "i",
        'ĵ' => "j",
        'ķ' => "k",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'ñ' | 'ń' | 'ņ' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => "o",
        'œ' => "oe",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'ś' | 'ŝ' | 'ş' | 'š' | 'ș' => "s",
        'ß' => "ss",
        'ţ' | 'ť' | 'ŧ' | 'ț' => "t",
        'þ' => "th",
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'ŵ' => "w",
        'ý' | 'ÿ' | 'ŷ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        _ => return None,
    })
}

/// Whether `c` is a combining diacritical mark (U+0300–U+036F): the accent
/// of decomposed text (`e` + U+0301), and the dot `İ` lower-cases to.
fn combining(c: char) -> bool {
    ('\u{300}'..='\u{36f}').contains(&c)
}

/// Lower-cases `text` and removes accents.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for upper in text.chars() {
        for c in upper.to_lowercase().filter(|c| !combining(*c)) {
            match plain(c) {
                Some(ascii) => out.push_str(ascii),
                None => out.push(c),
            }
        }
    }
    out
}

/// The folded letters and digits of `text`, nothing else: `GBR 1124`,
/// `GBR/1124` and `gbr-1124` are all `gbr1124`.
pub fn compact(text: &str) -> String {
    fold(text).chars().filter(|c| c.is_alphanumeric()).collect()
}

/// The words of `text`, folded: split at anything that is not a letter or a
/// digit, so `J/109` is `j` and `109`.
pub fn words(text: &str) -> Vec<String> {
    fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding_drops_case_and_accents() {
        assert_eq!(fold("Fröken Ñandú"), "froken nandu");
        assert_eq!(fold("KÅRE ØSTBY"), "kare ostby");
        assert_eq!(fold("Straße"), "strasse");
        assert_eq!(fold("Æolus"), "aeolus");
        assert_eq!(fold("Łódź"), "lodz");
        assert_eq!(fold("J/109"), "j/109");
        // Decomposed accents, and the Turkish dotted capital I.
        assert_eq!(fold("Fro\u{308}ken Jose\u{301}"), "froken jose");
        assert_eq!(fold("İSTANBUL"), "istanbul");
    }

    #[test]
    fn sail_numbers_compact_whatever_the_separator() {
        assert_eq!(compact("GBR1124"), "gbr1124");
        assert_eq!(compact("GBR 1124"), "gbr1124");
        assert_eq!(compact("GBR/1124"), "gbr1124");
        assert_eq!(compact(" gbr-1124 "), "gbr1124");
    }

    #[test]
    fn words_split_at_punctuation() {
        assert_eq!(words("  Farr 40, 2023 "), vec!["farr", "40", "2023"]);
        assert_eq!(words("J/109"), vec!["j", "109"]);
        assert!(words(" / ").is_empty());
    }
}
