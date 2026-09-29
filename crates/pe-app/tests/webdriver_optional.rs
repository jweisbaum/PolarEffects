#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The WebDriver automation endpoint must not reach a shipped build.
//!
//! The plugin serves an axum WebDriver endpoint on loopback that can click,
//! type, screenshot and read the DOM — the whole interface, to any process on
//! the machine that reads the port it prints. Invariant 4 says nothing is
//! fetched that the user did not ask for and nothing reaches in, and an
//! inbound socket that drives the application is exactly what "nothing
//! reaches in" rules out (D25; copied from VectorEffects).
//!
//! `npm run check:offline` cannot enforce this: it reads source URLs, remote
//! references in the built bundle and the strength of the CSP, none of which
//! sees a listener. The enforcement is that the dependency is *not compiled
//! in* unless someone asks, and these are the tests that say so — against
//! every place that decision could be undone:
//!
//! - `pe-app`'s manifest: every declaration of the crate, in any dependency
//!   table (target-specific ones included, and the `[dependencies.x]` table
//!   form), is optional; and nothing reachable from `default` — directly or
//!   through another feature — turns it on;
//! - every other manifest in the workspace: none names the crate at all, so
//!   it cannot arrive through a sibling;
//! - `package.json`: the build scripts do not pass the feature;
//! - the Tauri config files: none lists it in `build.features` (which
//!   `tauri build` would pass to cargo) or mentions it at all.
//!
//! There is deliberately **no** `cfg!(feature = "webdriver")` assertion here.
//! It reads like a guard and is not one: it is a constant in any given build,
//! true in an end-to-end run and false otherwise, so it says nothing the
//! manifest does not. Clippy rejects it as a constant assertion.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const CRATE: &str = "tauri-plugin-webdriver-automation";
const FEATURE: &str = "webdriver";

// ------------------------------------------------------------ helpers

/// A manifest line with its comment removed (a `#` outside a string).
fn uncommented(line: &str) -> &str {
    let mut quoted = false;
    for (i, c) in line.char_indices() {
        match c {
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..i],
            _ => {}
        }
    }
    line
}

/// The manifest as `(table header, lines)`, comments removed. The lines
/// before the first header have the header `""`.
fn tables(manifest: &str) -> Vec<(String, Vec<String>)> {
    let mut out = vec![(String::new(), Vec::new())];
    for raw in manifest.lines() {
        let line = uncommented(raw).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let header = line
                .trim_matches(|c| c == '[' || c == ']')
                .trim()
                .to_owned();
            out.push((header, Vec::new()));
        } else if let Some(last) = out.last_mut() {
            last.1.push(line.to_owned());
        }
    }
    out
}

/// A key naming the crate: `CRATE`, `"CRATE"`, or dotted `CRATE.optional`.
fn names_crate(key: &str) -> bool {
    let key = key.trim().trim_matches('"');
    key == CRATE
        || key.starts_with(&format!("{CRATE}."))
        || key.starts_with(&format!("\"{CRATE}\"."))
}

/// Every declaration of the crate in any dependency table, as the text that
/// says whether it is optional: `(table, declaration)`.
fn declarations(manifest: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (header, lines) in tables(manifest) {
        let last = header.rsplit('.').next().unwrap_or("").trim_matches('"');
        if last == CRATE {
            // `[dependencies.CRATE]` or `[target.'…'.dependencies.CRATE]`.
            out.push((header.clone(), lines.join("\n")));
            continue;
        }
        if !header.ends_with("dependencies") {
            continue;
        }
        let entry: Vec<&str> = lines
            .iter()
            .map(String::as_str)
            .filter(|line| line.split('=').next().is_some_and(names_crate))
            .collect();
        if !entry.is_empty() {
            out.push((header.clone(), entry.join("\n")));
        }
    }
    out
}

fn is_optional(declaration: &str) -> bool {
    declaration
        .lines()
        .any(|line| line.replace(' ', "").contains("optional=true"))
}

/// `[features]` as name → the strings it lists (multi-line lists included).
fn features(manifest: &str) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    for (header, lines) in tables(manifest) {
        if header != "features" {
            continue;
        }
        let text = lines.join("\n");
        let mut rest = text.as_str();
        while let Some(eq) = rest.find('=') {
            let name = rest[..eq].trim().trim_matches('"').to_owned();
            let after = &rest[eq + 1..];
            let (Some(open), Some(close)) = (after.find('['), after.find(']')) else {
                break;
            };
            let list = &after[open + 1..close];
            let items = list
                .split(',')
                .map(|s| s.trim().trim_matches('"').to_owned())
                .filter(|s| !s.is_empty())
                .collect();
            out.insert(name, items);
            rest = &after[close + 1..];
        }
    }
    out
}

/// Everything `default` turns on, following features through features.
fn default_closure(features: &BTreeMap<String, Vec<String>>) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut queue = vec!["default".to_owned()];
    while let Some(name) = queue.pop() {
        for item in features.get(&name).into_iter().flatten() {
            if seen.insert(item.clone()) && features.contains_key(item) {
                queue.push(item.clone());
            }
        }
    }
    seen
}

/// Whether a feature-list item turns the endpoint on.
fn enables_endpoint(item: &str) -> bool {
    let base = item.strip_prefix("dep:").unwrap_or(item);
    let base = base.split('/').next().unwrap_or(base).trim_end_matches('?');
    base == FEATURE || base == CRATE
}

/// The problems with a manifest that should hold the endpoint optional.
fn manifest_problems(manifest: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let found = declarations(manifest);
    if found.is_empty() {
        problems.push(format!("{CRATE} is not in the manifest at all"));
    }
    for (table, text) in &found {
        if !is_optional(text) {
            problems.push(format!(
                "[{table}] declares {CRATE} without optional = true: {text}"
            ));
        }
    }
    let table = features(manifest);
    for item in default_closure(&table) {
        if enables_endpoint(&item) {
            problems.push(format!(
                "the default features reach {item:?}, which compiles the endpoint in"
            ));
        }
    }
    if !table.contains_key(FEATURE) {
        problems.push(
            "the `webdriver` feature is gone; `npm run dev:webdriver` would build nothing"
                .to_owned(),
        );
    }
    problems
}

/// The problems with `package.json`: a build script that passes the feature.
fn package_problems(package: &str) -> Vec<String> {
    let json: serde_json::Value = serde_json::from_str(package).expect("package.json is JSON");
    let scripts = json["scripts"]
        .as_object()
        .expect("package.json has scripts");
    scripts
        .iter()
        .filter_map(|(name, script)| {
            let script = script.as_str().unwrap_or("");
            let builds =
                name == "build" || script.contains("tauri build") || script.contains("--release");
            (builds && script.contains(FEATURE))
                .then(|| format!("script {name:?} builds with the endpoint: {script}"))
        })
        .collect()
}

/// The problems with a Tauri config: the feature in `build.features`, or
/// anywhere at all.
fn tauri_config_problems(name: &str, text: &str) -> Vec<String> {
    let mut problems = Vec::new();
    if name.ends_with(".json")
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(text)
        && let Some(list) = json["build"]["features"].as_array()
        && list
            .iter()
            .any(|f| f.as_str().is_some_and(enables_endpoint))
    {
        problems.push(format!(
            "{name}: build.features turns the endpoint on for `tauri build`"
        ));
    }
    if text.contains(FEATURE) {
        problems.push(format!("{name} mentions {FEATURE:?}"));
    }
    problems
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifests_under(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path().join("Cargo.toml")))
        .filter(|p| p.is_file())
        .collect();
    out.sort();
    out
}

// -------------------------------------------------------------- tests

#[test]
fn the_webdriver_plugin_is_optional_and_off_by_default() {
    let problems = manifest_problems(include_str!("../Cargo.toml"));
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn no_other_workspace_manifest_names_the_plugin() {
    let root = repo();
    let own = root
        .join("crates/pe-app/Cargo.toml")
        .canonicalize()
        .unwrap();
    let mut others = vec![root.join("Cargo.toml")];
    others.extend(manifests_under(&root.join("crates")));
    others.extend(manifests_under(&root.join("tools")));
    assert!(
        others.len() > 5,
        "the workspace's manifests were not found: {others:?}"
    );
    for path in others {
        if path.canonicalize().unwrap() == own {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            declarations(&text).is_empty() && !text.contains(CRATE),
            "{} names {CRATE}; only pe-app may, and only optionally",
            path.display()
        );
    }
}

#[test]
fn the_build_scripts_do_not_pass_the_feature() {
    let package = std::fs::read_to_string(repo().join("package.json")).unwrap();
    let problems = package_problems(&package);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn no_tauri_config_turns_the_feature_on() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut seen = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        if !(lower.starts_with("tauri") && (lower.contains(".conf") || lower == "tauri.toml")) {
            continue;
        }
        seen += 1;
        let text = std::fs::read_to_string(&path).unwrap();
        let problems = tauri_config_problems(&name, &text);
        assert!(problems.is_empty(), "{problems:#?}");
    }
    assert!(seen >= 1, "tauri.conf.json was not found");
}

// ------------------------------------------- the guards bite

/// Without these, every check above could be vacuous: a pattern that matches
/// nothing passes as happily as one that matches the right thing.
#[test]
fn the_guards_catch_every_way_of_shipping_it() {
    let good = format!(
        "[dependencies]\n{CRATE} = {{ version = \"0.1.3\", optional = true }}\n\n\
         [features]\n# default = [\"webdriver\"] in a comment is fine\nwebdriver = [\"dep:{CRATE}\"]\n"
    );
    assert_eq!(manifest_problems(&good), Vec::<String>::new());

    // Made unconditional.
    let plain = good.replace(", optional = true", "");
    assert!(
        !manifest_problems(&plain).is_empty(),
        "a non-optional dependency"
    );

    // A second, non-optional declaration in a target table.
    let target = format!(
        "{good}\n[target.'cfg(target_os = \"macos\")'.dependencies]\n{CRATE} = \"0.1.3\"\n"
    );
    assert!(
        !manifest_problems(&target).is_empty(),
        "a target-specific declaration"
    );

    // The table form.
    let table = format!("{good}\n[dependencies.{CRATE}]\nversion = \"0.1.3\"\n");
    assert!(
        !manifest_problems(&table).is_empty(),
        "a [dependencies.x] table"
    );

    // Default, directly, transitively, and through `dep:`.
    for defaults in [
        "default = [\"webdriver\"]\n".to_owned(),
        "default = [\"e2e\"]\ne2e = [\"webdriver\"]\n".to_owned(),
        "default = [\n  \"e2e\",\n]\ne2e = [\"testing\"]\ntesting = [\"webdriver\"]\n".to_owned(),
        format!("default = [\"dep:{CRATE}\"]\n"),
    ] {
        let manifest = good.replace("[features]\n", &format!("[features]\n{defaults}"));
        assert!(
            !manifest_problems(&manifest).is_empty(),
            "a default reaching it: {defaults}"
        );
    }

    // The feature removed.
    let gone = good.replace(&format!("webdriver = [\"dep:{CRATE}\"]\n"), "");
    assert!(!manifest_problems(&gone).is_empty(), "the feature missing");

    // package.json and the Tauri config.
    let package = r#"{"scripts":{"dev:webdriver":"tauri dev --features webdriver","build":"tauri build --features webdriver"}}"#;
    assert_eq!(package_problems(package).len(), 1);
    assert!(package_problems(r#"{"scripts":{"dev:webdriver":"tauri dev --features webdriver","build":"tauri build"}}"#).is_empty());
    let conf = r#"{"build":{"features":["webdriver"]}}"#;
    assert!(!tauri_config_problems("tauri.macos.conf.json", conf).is_empty());
    assert!(tauri_config_problems("tauri.conf.json", r#"{"build":{"devUrl":"x"}}"#).is_empty());
}
