//! Preserve certificate facts independently of the derived boat-speed grids.

use std::collections::BTreeMap;

use dom_query::{Document, Selection};
use pe_core::orr::{OrrCertificateData, OrrField, OrrRating, OrrTable};

use super::{CERTIFICATE_URL, Certificate, error, normalized};
use crate::error::Result;

fn attribute(node: &Selection<'_>, name: &str) -> String {
    node.attr(name).map(|s| s.to_string()).unwrap_or_default()
}

fn field(row: &Selection<'_>, name: &str, value: String) -> OrrField {
    let displayed = row
        .select(".dataElement, .input, .inputtextarea")
        .iter()
        .find(|n| {
            n.attr("id")
                .is_some_and(|id| id.as_ref() == name || id.as_ref() == format!("{name}-sel"))
        });
    let display = displayed.as_ref().map(|n| normalized(&n.text()));
    let class = displayed
        .as_ref()
        .map(|n| attribute(n, "class"))
        .unwrap_or_default();
    let quantity = class
        .split_whitespace()
        .find_map(|c| c.strip_prefix("input").filter(|s| !s.is_empty()))
        .unwrap_or(if class.contains("dropdownSelected") {
            "enum"
        } else {
            "text"
        })
        .to_owned();
    let heading = normalized(&row.select(".heading").first().text());
    OrrField {
        section: attribute(&row.ancestors(None).filter(".gridDivs").first(), "id"),
        label: if heading.is_empty() {
            name.into()
        } else {
            heading
        },
        display: display.filter(|s| *s != value),
        value,
        quantity,
    }
}

fn ratings(scope: &Selection<'_>) -> Result<BTreeMap<String, Vec<OrrRating>>> {
    let mut out = BTreeMap::new();
    if scope.select(".ratBlock:not([data-ratingjson])").length() > 0 {
        return Err(error("ratings", "a published rating block has no data"));
    }
    for block in scope.select("[data-ratingjson]").iter() {
        let group = attribute(&block.ancestors(None).filter(".gridDivs").first(), "id");
        let at = format!("ratings {group}");
        let raw = attribute(&block, "data-ratingjson");
        let mut values: Vec<serde_json::Value> =
            serde_json::from_str(&raw).map_err(|e| error(&at, e.to_string()))?;
        for value in &mut values {
            // Some years encode numbers as JSON numbers, others as strings.
            // Retain the source precision; null and blank remain distinct.
            for key in ["spin", "nonspin"] {
                if let Some(number) = value.get_mut(key).filter(|v| v.is_number()) {
                    *number = serde_json::Value::String(number.to_string());
                }
            }
        }
        let rows: Vec<OrrRating> = values
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(|e| error(&at, e.to_string())))
            .collect::<Result<_>>()?;
        for rating in &rows {
            for value in [&rating.spin, &rating.nonspin].into_iter().flatten() {
                if !value.is_empty()
                    && value
                        .replace(',', "")
                        .parse::<f64>()
                        .ok()
                        .is_none_or(|n| !n.is_finite())
                {
                    return Err(error(&at, format!("invalid rating {value:?}")));
                }
            }
        }
        let entries = out.entry(group).or_insert_with(Vec::new);
        for row in rows {
            if !entries.contains(&row) {
                entries.push(row);
            }
        }
    }
    if out.is_empty() {
        return Err(error("ratings", "no public rating data found"));
    }
    Ok(out)
}

pub(super) fn parse(doc: &Document, certificate: &Certificate) -> Result<OrrCertificateData> {
    let scope = doc.select("#cert_form .tabData:not([data-tbname='Glossary'])");
    let mut fields: BTreeMap<String, OrrField> = BTreeMap::new();
    for input in scope.select("[name][data-origval]").iter() {
        let name = attribute(&input, "name");
        if name.starts_with("blank_line") || name.is_empty() {
            continue;
        }
        let row = input.ancestors(None).filter(".row").first();
        let value = normalized(&attribute(&input, "data-origval"));
        let datum = field(&row, &name, value);
        // A hidden flag and its visible enum may occur in different sections.
        // Prefer the labeled instance without discarding conflicting originals.
        if let Some(old) = fields.get(&name) {
            if old.value != datum.value {
                return Err(error(&name, "conflicting certificate values"));
            }
            if old.label != name || datum.label == name {
                continue;
            }
        }
        fields.insert(name, datum);
    }
    for printed in scope.select(".row span[id]").iter().filter(|n| {
        attribute(n, "class")
            .split_whitespace()
            .any(|c| c == "dataElement" || c.starts_with("input"))
    }) {
        let name = attribute(&printed, "id");
        if name.starts_with("blank_line")
            || name
                .strip_suffix("-sel")
                .is_some_and(|base| fields.contains_key(base))
        {
            continue;
        }
        let row = printed.ancestors(None).filter(".row").first();
        fields
            .entry(name.clone())
            .or_insert_with(|| field(&row, &name, normalized(&printed.text())));
    }
    if fields.is_empty() {
        return Err(error("data", "no public certificate fields found"));
    }
    if fields
        .get("cert_id-ce")
        .is_none_or(|f| f.value != certificate.certificate)
        || fields
            .get("yr-ce")
            .is_none_or(|f| f.value != certificate.year.to_string())
    {
        return Err(error(
            "identity",
            "certificate number/year differs from the valid list",
        ));
    }
    let mut tables = BTreeMap::new();
    for table in scope.select("table.ORRPolars").iter() {
        let container = table.ancestors(None).filter(".gridDivs").first();
        let id = attribute(&container, "id");
        let columns: Vec<_> = table
            .select("thead th")
            .iter()
            .map(|c| normalized(&c.text()))
            .collect();
        let rows: Vec<Vec<_>> = table
            .select("tbody > tr")
            .iter()
            .map(|r| {
                r.select("td")
                    .iter()
                    .map(|c| normalized(&c.text()))
                    .collect()
            })
            .collect();
        if columns.is_empty()
            || rows.is_empty()
            || rows.iter().any(|row| row.len() != columns.len())
        {
            return Err(error(&id, "incomplete certificate table"));
        }
        let unit = if id.starts_with("polar_speed") {
            "kn"
        } else if id.starts_with("polar_time") {
            "s/nmi"
        } else {
            return Err(error(&id, "unknown polar table units"));
        };
        tables.insert(
            id,
            OrrTable {
                unit: unit.into(),
                columns,
                rows,
                notes: container
                    .select(".polarFootnote")
                    .iter()
                    .map(|n| normalized(&n.text()))
                    .collect(),
            },
        );
    }
    Ok(OrrCertificateData {
        source_url: format!("{CERTIFICATE_URL}{}", certificate.sku),
        list_fields: certificate.fields.clone(),
        fields,
        ratings: ratings(&scope)?,
        tables,
    })
}
