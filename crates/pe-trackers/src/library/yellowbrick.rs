//! The mobile catalogue has numeric IDs; only MyRaces supplies tracker codes.
//! Associate missing free products with the configured account, then re-read
//! MyRaces. Never infer a code from a title or treat an API/product ID as a code.
use crate::{Fetcher, Result, TrackerError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const CATALOGUE_URL: &str = "https://app.yb.tl/App/Races?version=3";
const MY_RACES: &str = "https://app.yb.tl/App/MyRaces";
const ASSOCIATE: &str = "https://app.yb.tl/App/purchase";

/// Supplied by the person; never bundled with the application or printed.
pub struct Credentials<'a> {
    pub user_key: &'a str,
    pub device_id: &'a str,
}
impl std::fmt::Debug for Credentials<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("YellowBrickCredentials([redacted])")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Race {
    pub id: String,
    pub title: String,
    pub date: String,
    /// Includes every child race/leg, sorted and deduplicated.
    pub urls: Vec<String>,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Discovery {
    pub races: Vec<Race>,
    pub warnings: Vec<String>,
}
impl Discovery {
    pub fn urls(&self) -> Vec<String> {
        self.races
            .iter()
            .flat_map(|r| r.urls.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn unresolved(&self) -> usize {
        self.races.iter().filter(|r| r.urls.is_empty()).count()
    }
}

#[derive(Debug, Default, Deserialize)]
struct Envelope {
    races: Option<RaceList>,
    myraces: Option<RaceList>,
    purchase: Option<Purchase>,
}
#[derive(Debug, Default, Deserialize)]
struct RaceList {
    #[serde(default, rename = "race")]
    races: Vec<MobileRace>,
}
#[derive(Debug, Default, Deserialize)]
struct MobileRace {
    #[serde(default, rename = "@race-id")]
    id: String,
    #[serde(default, rename = "@date")]
    date: String,
    #[serde(default)]
    title: String,
    #[serde(default, rename = "@price-type")]
    price: String,
    #[serde(default, rename = "@ios-productid")]
    product: String,
    #[serde(default, rename = "base-url")]
    bases: Vec<String>,
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    endpointl: String,
    #[serde(default)]
    children: RaceList,
}
#[derive(Debug, Deserialize)]
struct Purchase {
    #[serde(rename = "@success")]
    success: String,
}
fn decode(message: &str) -> TrackerError {
    TrackerError::Decode {
        what: "YellowBrick catalogue",
        at: "response".into(),
        why: message.into(),
    }
}
fn parse(bytes: &[u8]) -> Result<Envelope> {
    // Do not copy remote response content into errors: authenticated responses
    // and even XML parser diagnostics must never disclose credentials.
    quick_xml::de::from_reader(bytes).map_err(|_| decode("Invalid catalogue XML"))
}
fn credential_url(
    base: &str,
    credentials: &Credentials<'_>,
    params: &[(&str, &str)],
) -> Result<String> {
    let mut url =
        reqwest::Url::parse(base).map_err(|_| decode("Invalid YellowBrick API address"))?;
    url.query_pairs_mut()
        .extend_pairs(params.iter().copied())
        .append_pair("user-key", credentials.user_key.trim())
        .append_pair("udid", credentials.device_id.trim());
    Ok(url.to_string())
}
fn private_error(error: TrackerError) -> TrackerError {
    match error {
        TrackerError::Cancelled => TrackerError::Cancelled,
        TrackerError::Http { status, .. } => TrackerError::Http {
            status,
            why: format!("YellowBrick authenticated lookup failed (HTTP {status})"),
        },
        _ => TrackerError::Network(
            "YellowBrick authenticated lookup failed; check credentials and connection".into(),
        ),
    }
}
fn my_races(
    get: &mut impl FnMut(&str) -> Result<Vec<u8>>,
    credentials: &Credentials<'_>,
) -> Result<Vec<MobileRace>> {
    let url = credential_url(
        MY_RACES,
        credentials,
        &[
            ("version", "4"),
            ("os", "i"),
            ("sv", "4021"),
            ("osv", "12.4.1"),
        ],
    )?;
    parse(&get(&url).map_err(private_error)?)?
        .myraces
        .map(|r| r.races)
        .ok_or_else(|| decode("Missing MyRaces list"))
}
fn canonical_url(base: &str) -> Option<String> {
    let event = super::resolve_source("YELLOWBRICK", base.trim()).ok()?;
    // YB's keys are case insensitive. Normalize before queueing, while the
    // database retains its original scrapedUrl on subsequent upserts.
    Some(format!(
        "https://yb.tl/{}",
        crate::yellowbrick::url_key(&event.key.to_ascii_lowercase())
    ))
}
fn race_urls(race: &MobileRace, out: &mut BTreeSet<String>) {
    for base in &race.bases {
        if let Some(url) = canonical_url(base) {
            out.insert(url);
        }
    }
    // A host-only endpoint is not a race. Some API generations put the whole
    // race URL here; base-url is otherwise the actual code, not the host.
    for endpoint in [&race.endpointl, &race.endpoint] {
        if (endpoint.starts_with("https://") || endpoint.starts_with("http://"))
            && let Some(url) = canonical_url(endpoint)
        {
            out.insert(url);
        }
    }
    for child in &race.children.races {
        race_urls(child, out);
    }
}

/// `known` maps exact CalendarEvents.scrapedOriginalId values to stored URLs.
/// Called only during the person's requested/scheduled scrape. The callback
/// contains no credentials; HTTP stays in Rust, cancellable and bounded.
pub fn discover(
    fetcher: &Fetcher,
    credentials: Option<&Credentials<'_>>,
    known: &BTreeMap<String, Vec<String>>,
    progress: &mut dyn FnMut(String),
) -> Result<Discovery> {
    discover_with(
        &mut |url| fetcher.get(url, &mut |_, _| {}),
        credentials,
        known,
        progress,
    )
}
fn discover_with(
    get: &mut impl FnMut(&str) -> Result<Vec<u8>>,
    credentials: Option<&Credentials<'_>>,
    known: &BTreeMap<String, Vec<String>>,
    progress: &mut dyn FnMut(String),
) -> Result<Discovery> {
    progress("Reading YellowBrick version 3 catalogue".into());
    let catalogue = parse(&get(CATALOGUE_URL)?)?
        .races
        .ok_or_else(|| decode("Missing version 3 race list"))?
        .races;
    if catalogue.iter().any(|r| r.id.is_empty()) {
        return Err(decode("A catalogue race has no race ID"));
    }
    let mut warnings = Vec::new();
    let mut owned = Vec::new();
    if let Some(credentials) = credentials {
        progress("Reading YellowBrick race codes".into());
        owned = my_races(get, credentials)?;
        let owned_ids: BTreeSet<_> = owned.iter().map(|r| r.id.as_str()).collect();
        let mut products = BTreeSet::new();
        let missing: Vec<_> = catalogue
            .iter()
            .filter(|r| {
                !owned_ids.contains(r.id.as_str())
                    && r.price == "free"
                    && !r.product.is_empty()
                    && products.insert(r.product.clone())
            })
            .collect();
        let mut consecutive_failures = 0;
        for (index, race) in missing.iter().enumerate() {
            progress(format!(
                "Resolving YellowBrick race codes: {} of {}",
                index + 1,
                missing.len()
            ));
            let url = credential_url(
                ASSOCIATE,
                credentials,
                &[
                    ("version", "2"),
                    ("product-id", &race.product),
                    ("receipt", &race.product),
                    ("try", "0"),
                ],
            )?;
            // Association is permitted only for a product explicitly listed as
            // free above. No paid product or missing/unknown price is requested.
            match get(&url).map_err(private_error).and_then(|b| parse(&b)) {
                Ok(response) if response.purchase.as_ref().is_some_and(|p| p.success == "1") => {
                    consecutive_failures = 0;
                }
                Err(TrackerError::Cancelled) => return Err(TrackerError::Cancelled),
                _ => {
                    warnings.push(format!(
                        "YellowBrick race {}: free race association failed",
                        race.id
                    ));
                    consecutive_failures += 1;
                    if consecutive_failures >= 3 {
                        warnings.push("YellowBrick association stopped after three consecutive failures; check credentials and connection".into());
                        break;
                    }
                }
            }
        }
        if !missing.is_empty() {
            progress("Refreshing YellowBrick race codes".into());
            owned = my_races(get, credentials)?;
        }
    }
    let mut mapped: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for race in &owned {
        race_urls(race, mapped.entry(race.id.clone()).or_default());
    }
    let mut seen = BTreeSet::new();
    let mut races = Vec::new();
    for race in catalogue {
        if !seen.insert(race.id.clone()) {
            continue;
        }
        let urls = mapped.entry(race.id.clone()).or_default();
        race_urls(&race, urls);
        if let Some(previous) = known.get(&race.id) {
            urls.extend(previous.iter().filter_map(|u| canonical_url(u)));
        }
        races.push(Race {
            id: race.id,
            title: race.title,
            date: race.date,
            urls: urls.iter().cloned().collect(),
        });
    }
    let result = Discovery { races, warnings };
    progress(format!(
        "YellowBrick: {} catalogue races, {} resolved URLs, {} races without codes",
        result.races.len(),
        result.urls().len(),
        result.unresolved()
    ));
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_mobile_responses() {
        let catalogue = parse(include_bytes!(
            "../../tests/fixtures/yellowbrick/Races-v3.xml"
        ))
        .unwrap()
        .races
        .unwrap()
        .races;
        assert_eq!(catalogue.len(), 3);
        assert_eq!(catalogue[0].id, "4205");
        assert_eq!(catalogue[0].title, "2026 Round Aeolian Race");
        let owned = parse(include_bytes!(
            "../../tests/fixtures/yellowbrick/MyRaces-v4.xml"
        ))
        .unwrap()
        .myraces
        .unwrap()
        .races;
        let stars = owned.iter().find(|r| r.id == "2634").unwrap();
        let mut stars_urls = BTreeSet::new();
        race_urls(stars, &mut stars_urls);
        assert!(stars_urls.contains("https://yb.tl/stars&spokes2023"));
        let mint = owned.iter().find(|r| r.id == "617").unwrap();
        assert_eq!(mint.bases, ["mint400-2016-group", "mint400-2016"]);
        assert_eq!(mint.children.races.len(), 3);
        let mut urls = BTreeSet::new();
        race_urls(mint, &mut urls);
        assert_eq!(urls.len(), 4);
        assert!(urls.contains("https://yb.tl/mint400-2016-unlimited"));
    }
    const CATALOGUE: &str = r#"<r><races>
      <race race-id="10" date="2026-09-25" price-type="free" ios-productid="FREE10"><title>New &amp; free</title></race>
      <race race-id="20" price-type="paid" ios-productid="PAID20"><title>Paid</title></race>
      <race race-id="30" price-type="free" ios-productid="FREE30"><title>Known</title></race>
      <race race-id="40" ios-productid="UNKNOWN40"><title>Unknown price</title></race>
    </races></r>"#;
    const OWNED: &str = r#"<r><myraces><race race-id="30"><title>Known</title><endpoint>https://yb.tl/</endpoint><base-url>Known2022</base-url><children><race><base-url>Known2022_2</base-url><base-url>known2022_2</base-url></race></children></race></myraces></r>"#;
    #[test]
    fn associates_only_missing_free_products_and_reads_every_child_code() {
        let credentials = Credentials {
            user_key: "key&private",
            device_id: "device/private",
        };
        let mut requests = Vec::new();
        let mut reads = 0;
        let discovery = discover_with(&mut |url| {
            requests.push(url.to_owned());
            let url = reqwest::Url::parse(url).unwrap();
            if url.path() == "/App/Races" { return Ok(CATALOGUE.as_bytes().to_vec()); }
            let query: BTreeMap<_, _> = url.query_pairs().into_owned().collect();
            assert_eq!(query["user-key"], "key&private");
            assert_eq!(query["udid"], "device/private");
            if url.path() == "/App/purchase" {
                assert_eq!(query["product-id"], "FREE10");
                return Ok(b"<r><purchase success=\"1\"/></r>".to_vec());
            }
            assert_eq!(query["version"], "4");
            reads += 1;
            Ok(if reads == 1 { OWNED.into() } else { OWNED.replace("</myraces>", "<race race-id=\"10\"><endpointl>https://cf.yb.tl/</endpointl><base-url>raeolian2026</base-url></race></myraces>") }.into_bytes())
        }, Some(&credentials), &BTreeMap::new(), &mut |_| {}).unwrap();
        assert_eq!(reads, 2);
        assert_eq!(requests.len(), 4);
        assert_eq!(discovery.races[0].title, "New & free");
        assert_eq!(
            discovery.urls(),
            [
                "https://yb.tl/known2022",
                "https://yb.tl/known2022_2",
                "https://yb.tl/raeolian2026"
            ]
        );
        assert_eq!(discovery.unresolved(), 2);
        assert!(
            !serde_json::to_string(&discovery)
                .unwrap()
                .contains("private")
        );
    }
    #[test]
    fn public_discovery_uses_exact_ids_never_titles_or_product_ids() {
        let known = BTreeMap::from([
            ("30".into(), vec!["https://yb.tl/known2022".into()]),
            ("999".into(), vec!["wrong".into()]),
        ]);
        let report = discover_with(
            &mut |url| {
                assert_eq!(url, CATALOGUE_URL);
                Ok(CATALOGUE.as_bytes().to_vec())
            },
            None,
            &known,
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(report.urls(), ["https://yb.tl/known2022"]);
        assert_eq!(report.unresolved(), 3);
        assert!(parse(b"<r><races>").is_err());
        assert!(canonical_url("https://example.invalid/race").is_none());
        assert!(canonical_url("https://yb.tl").is_none());
    }
    #[test]
    fn cancellation_stops_association_and_repeated_failures_are_bounded() {
        let credentials = Credentials {
            user_key: "private",
            device_id: "private",
        };
        let catalogue = format!(
            "<r><races>{}</races></r>",
            (1..=5)
                .map(|id| format!(
                    r#"<race race-id="{id}" price-type="free" ios-productid="FREE{id}"/>"#
                ))
                .collect::<String>()
        );
        for cancelled in [true, false] {
            let mut associations = 0;
            let result = discover_with(
                &mut |url| {
                    let url = reqwest::Url::parse(url).unwrap();
                    match url.path() {
                        "/App/Races" => Ok(catalogue.as_bytes().to_vec()),
                        "/App/MyRaces" => Ok(b"<r><myraces/></r>".to_vec()),
                        "/App/purchase" => {
                            associations += 1;
                            if cancelled {
                                Err(TrackerError::Cancelled)
                            } else {
                                Err(TrackerError::Network("private authenticated URL".into()))
                            }
                        }
                        _ => panic!("unexpected request"),
                    }
                },
                Some(&credentials),
                &BTreeMap::new(),
                &mut |_| {},
            );
            if cancelled {
                assert!(matches!(result, Err(TrackerError::Cancelled)));
                assert_eq!(associations, 1);
            } else {
                let report = result.unwrap();
                assert_eq!(associations, 3);
                assert_eq!(report.unresolved(), 5);
                assert_eq!(report.warnings.len(), 4);
                assert!(!serde_json::to_string(&report).unwrap().contains("private"));
            }
        }
    }

    #[test]
    fn authenticated_errors_and_debug_never_include_secrets() {
        let c = Credentials {
            user_key: "secret-key",
            device_id: "secret-device",
        };
        assert!(!format!("{c:?}").contains("secret-key"));
        let e = private_error(TrackerError::Http {
            status: 401,
            why: "https://app.yb.tl/?user-key=secret-key".into(),
        });
        assert!(!e.to_string().contains("secret-key"));
        assert!(matches!(
            private_error(TrackerError::Cancelled),
            TrackerError::Cancelled
        ));
    }
}
