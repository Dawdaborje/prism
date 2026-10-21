//! Putting a pack into the organization: its workbook, its charts, its dashboard.
//!
//! Everything goes through Prism's own functions, so a pack is checked like anything a person
//! builds. A pack is re-synced every time it is installed: charts are matched by name inside the
//! pack's workbook and updated, the dashboard keeps its place and gets its items and filters again.

use aether_sdk::prelude::*;

use crate::packs;

fn available(requires: &Value) -> bool {
    requires
        .as_array()
        .map(|list| list.iter().all(|name| name.as_str().is_some_and(|n| plugins::available(n).unwrap_or(false))))
        .unwrap_or(false)
}

fn text<'a>(value: &'a Value, field: &str) -> &'a str {
    value.get(field).and_then(Value::as_str).unwrap_or("")
}

fn rows(answer: Value) -> Vec<Value> {
    match answer {
        Value::Array(list) => list,
        other => other.get("rows").and_then(Value::as_array).cloned().unwrap_or_default(),
    }
}

/// The record named `name` among `candidates`, owned by nobody (what a pack creates).
fn by_name<'a>(candidates: &'a [Value], name: &str) -> Option<&'a Value> {
    candidates.iter().find(|row| text(row, "name") == name)
}

fn upsert_workbook(pack: &Value) -> Result<String> {
    let name = text(&pack["workbook"], "name");
    let found = rows(plugins::call("prism", "search_workbooks", &json!({ "q": name, "limit": 50 }))?);
    if let Some(existing) = by_name(&found, name) {
        return Ok(text(existing, "id").to_string());
    }
    let created: Value = plugins::call(
        "prism",
        "create_workbook",
        &json!({ "name": name, "description": text(&pack["workbook"], "description"), "shared": true }),
    )?;
    Ok(text(&created, "id").to_string())
}

/// Create or update each chart; answers chart code → chart id.
fn upsert_charts(pack: &Value, workbook: &str) -> Result<std::collections::HashMap<String, String>> {
    let existing = rows(plugins::call("prism", "list_workbook_charts", &json!({ "id": workbook }))?);
    let mut ids = std::collections::HashMap::new();
    for chart in pack["charts"].as_array().cloned().unwrap_or_default() {
        let code = text(&chart, "code").to_string();
        let mut body = chart.clone();
        if let Some(object) = body.as_object_mut() {
            object.remove("code");
        }
        let saved: Value = match by_name(&existing, text(&chart, "name")) {
            Some(found) => {
                body["id"] = json!(text(found, "id"));
                plugins::call("prism", "set_chart", &body)?
            }
            None => {
                body["workbook"] = json!(workbook);
                plugins::call("prism", "create_chart", &body)?
            }
        };
        ids.insert(code, text(&saved, "id").to_string());
    }
    Ok(ids)
}

fn upsert_dashboard(pack: &Value, charts: &std::collections::HashMap<String, String>) -> Result<String> {
    let dashboard = &pack["dashboard"];
    let name = text(dashboard, "name");
    let origin = format!("pack:{}", text(pack, "code"));
    let found = rows(plugins::call("prism_dashboard", "search_dashboards", &json!({ "q": name, "limit": 50 }))?);
    let existing = found.iter().find(|row| text(row, "name") == name && text(row, "origin") == origin);
    let id = match existing {
        Some(row) => text(row, "id").to_string(),
        None => {
            let created: Value = plugins::call(
                "prism_dashboard",
                "create_dashboard",
                &json!({ "name": name, "description": text(dashboard, "description"), "shared": true, "origin": origin }),
            )?;
            text(&created, "id").to_string()
        }
    };
    // Items and filters are put back as the pack defines them.
    for item in rows(plugins::call("prism_dashboard", "list_items", &json!({ "id": id }))?) {
        plugins::call::<Value>("prism_dashboard", "remove_item", &json!({ "id": text(&item, "id") }))?;
    }
    for filter in rows(plugins::call("prism_dashboard", "list_filters", &json!({ "id": id }))?) {
        plugins::call::<Value>("prism_dashboard", "remove_filter", &json!({ "id": text(&filter, "id") }))?;
    }
    for item in dashboard["items"].as_array().cloned().unwrap_or_default() {
        if let Some(note) = item.get("note").and_then(Value::as_str) {
            plugins::call::<Value>(
                "prism_dashboard",
                "add_note",
                &json!({ "dashboard": id, "title": text(&item, "title"), "text": note, "width": text(&item, "width") }),
            )?;
            continue;
        }
        let chart = charts
            .get(text(&item, "chart"))
            .ok_or_else(|| Error::msg(format!("pack `{}` has no chart `{}`", text(pack, "code"), text(&item, "chart"))))?;
        plugins::call::<Value>(
            "prism_dashboard",
            "add_item",
            &json!({ "dashboard": id, "chart": chart, "width": text(&item, "width") }),
        )?;
    }
    for filter in dashboard["filters"].as_array().cloned().unwrap_or_default() {
        let mut body = filter.clone();
        body["dashboard"] = json!(id);
        plugins::call::<Value>("prism_dashboard", "add_filter", &body)?;
    }
    Ok(id)
}

fn install(pack: &Value) -> Result<Value> {
    let workbook = upsert_workbook(pack)?;
    let charts = upsert_charts(pack, &workbook)?;
    let dashboard = upsert_dashboard(pack, &charts)?;
    Ok(json!({ "pack": text(pack, "code"), "workbook": workbook, "dashboard": dashboard, "charts": charts.len() }))
}

pub fn install_by_code(code: &str) -> Result<Value> {
    let pack = packs::find(code).ok_or_else(|| Error::msg(format!("there is no pack `{code}`")))?;
    if !available(&pack["requires"]) {
        return Err(Error::msg(format!(
            "`{}` needs {} installed first",
            text(&pack, "name"),
            pack["requires"].as_array().map(|list| list.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")).unwrap_or_default()
        )));
    }
    install(&pack)
}

/// Every pack whose app is installed. A pack that fails does not stop the others.
pub fn seed() -> Result<Value> {
    let mut installed = Vec::new();
    let mut skipped = Vec::new();
    let mut failed = Vec::new();
    for pack in packs::all() {
        let code = text(&pack, "code").to_string();
        if !available(&pack["requires"]) {
            skipped.push(code);
            continue;
        }
        match install(&pack) {
            Ok(done) => installed.push(done),
            Err(error) => failed.push(json!({ "pack": code, "error": error.to_string() })),
        }
    }
    Ok(json!({ "installed": installed, "skipped": skipped, "failed": failed }))
}

pub fn list() -> Result<Vec<Value>> {
    let dashboards = rows(plugins::call("prism_dashboard", "search_dashboards", &json!({ "limit": 200 }))?);
    Ok(packs::all()
        .into_iter()
        .map(|pack| {
            let origin = format!("pack:{}", text(&pack, "code"));
            let installed = dashboards.iter().any(|row| text(row, "origin") == origin);
            json!({
                "id": text(&pack, "code"),
                "name": text(&pack, "name"),
                "description": text(&pack, "description"),
                "requires": pack["requires"],
                "available": available(&pack["requires"]),
                "installed": installed,
                "charts": pack["charts"].as_array().map(Vec::len).unwrap_or(0),
            })
        })
        .collect())
}
