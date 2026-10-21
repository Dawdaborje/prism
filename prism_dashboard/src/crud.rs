//! Dashboards, their items and their filters.
//!
//! Who may see or change which record is the kernel's job (`rules/`): a private dashboard, and its
//! items and filters, are the owner's and the administrators'; a shared one is read by every Prism role.

use aether_sdk::prelude::*;

use crate::board::sorted;
use crate::common::{id_of, me, pick, require, require_analyst, require_viewer, required, text, Record};

#[derive(Deserialize)]
pub struct Id {
    pub id: String,
}

#[derive(Deserialize, Default)]
pub struct Search {
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub filter: Option<Value>,
    #[serde(default)]
    pub order: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

const DASHBOARD_FIELDS: &[&str] = &["name", "description", "shared", "refresh_secs", "archived"];

pub fn create_dashboard(input: Value) -> Result<Record> {
    require_analyst()?;
    required(&input, "name", "the name")?;
    let mut data = pick(&input, DASHBOARD_FIELDS);
    data["owner"] = json!(me()?);
    // Where a ready-made dashboard came from; only set when it is created.
    if let Some(origin) = text(&input, "origin") {
        data["origin"] = json!(origin);
    }
    db::create::<Record>("prism_dashboard", &data)
}

pub fn set_dashboard(input: Value) -> Result<Record> {
    require_analyst()?;
    let id = required(&input, "id", "the dashboard")?;
    db::update::<Record>("prism_dashboard", &id, &pick(&input, DASHBOARD_FIELDS))?
        .ok_or_else(|| Error::msg("you cannot change that dashboard"))
}

pub fn get_dashboard(input: Id) -> Result<Record> {
    require_viewer()?;
    require("prism_dashboard", &input.id, "dashboard")
}

pub fn search_dashboards(input: Search) -> Result<Value> {
    require_viewer()?;
    let filter = match input.filter {
        Some(value @ Value::Object(_)) => db::Filter::eq("archived", false).and(db::Filter::from_json(value)),
        _ => db::Filter::eq("archived", false),
    };
    let mut answer = db::search(
        "prism_dashboard",
        input.q.as_deref(),
        &["name", "description"],
        filter,
        input.order.as_deref().or(Some("name")),
        Some(input.limit.unwrap_or(50).min(200)),
        input.offset,
    )?;
    // Mark the ones this person starred, and bring them to the top of the page.
    // The kernel's own calls (an install seed) have no one to star for.
    let starred: Vec<String> = match context::current()?.actor.id {
        Some(user) => db::find::<Record>("prism_dashboard_star")
            .filter("user", user.as_str())
            .limit(500)
            .all()?
            .iter()
            .filter_map(|star| text(star, "dashboard").map(str::to_string))
            .collect(),
        None => Vec::new(),
    };
    if let Some(rows) = answer.get_mut("rows").and_then(Value::as_array_mut) {
        for row in rows.iter_mut() {
            let on = row.get("id").and_then(Value::as_str).is_some_and(|id| starred.iter().any(|s| s == id));
            row["starred"] = json!(on);
        }
        rows.sort_by_key(|row| std::cmp::Reverse(row["starred"].as_bool().unwrap_or(false)));
    }
    Ok(answer)
}

/// Star a dashboard for the person calling (again is harmless).
pub fn star_dashboard(input: Id) -> Result<Value> {
    require_viewer()?;
    let dashboard = require("prism_dashboard", &input.id, "dashboard")?;
    let key = id_of(&dashboard)?.to_string();
    let user = me()?;
    let already = db::find::<Record>("prism_dashboard_star").filter("user", user.as_str()).filter("dashboard", key.as_str()).first()?;
    if already.is_none() {
        db::create::<Record>("prism_dashboard_star", &json!({ "dashboard": key, "user": user }))?;
    }
    Ok(json!({ "starred": true }))
}

pub fn unstar_dashboard(input: Id) -> Result<Value> {
    require_viewer()?;
    let user = me()?;
    for star in db::find::<Record>("prism_dashboard_star").filter("user", user.as_str()).filter("dashboard", input.id.as_str()).all()? {
        db::delete::<Record>("prism_dashboard_star", id_of(&star)?)?;
    }
    Ok(json!({ "starred": false }))
}

pub fn delete_dashboard(input: Id) -> Result<Value> {
    require_analyst()?;
    let dashboard = require("prism_dashboard", &input.id, "dashboard")?;
    let key = id_of(&dashboard)?.to_string();
    let mut gone = 1;
    for model in ["prism_dashboard_item", "prism_dashboard_filter"] {
        for row in sorted(model, &key)? {
            db::delete::<Record>(model, id_of(&row)?)?;
            gone += 1;
        }
    }
    db::delete::<Record>("prism_dashboard", &key)?;
    Ok(json!({ "deleted": gone }))
}

/// A private copy to change freely: the dashboard, its items and its filters. `origin` keeps
/// where it came from, so a ready-made dashboard can be forked without touching the original.
pub fn duplicate_dashboard(input: Id) -> Result<Record> {
    require_analyst()?;
    let source = require("prism_dashboard", &input.id, "dashboard")?;
    let source_id = id_of(&source)?.to_string();
    let mut data = pick(&source, DASHBOARD_FIELDS);
    data["name"] = json!(format!("Copy of {}", text(&source, "name").unwrap_or("dashboard")));
    data["owner"] = json!(me()?);
    data["shared"] = json!(false);
    data["origin"] = json!(text(&source, "origin").unwrap_or(&source_id));
    let copy = db::create::<Record>("prism_dashboard", &data)?;
    let copy_id = id_of(&copy)?.to_string();
    // `list_items` knows which charts still exist; an item whose chart was deleted is left behind rather
    // than stopping the whole copy.
    for item in list_items(Id { id: source_id.clone() })? {
        let has_chart = text(&item, "chart").is_some_and(|chart| !chart.is_empty());
        if has_chart && item.get("chart_kind").is_none_or_null() {
            continue;
        }
        let mut row = pick(&item, &["chart", "title", "text", "width", "position"]);
        // A note has no chart; an empty link would be refused.
        if text(&item, "chart").map_or(true, str::is_empty) {
            if let Some(object) = row.as_object_mut() {
                object.remove("chart");
            }
        }
        row["dashboard"] = json!(copy_id);
        db::create::<Record>("prism_dashboard_item", &row)?;
    }
    for filter in sorted("prism_dashboard_filter", &source_id)? {
        let mut row = pick(&filter, &["name", "label", "kind", "field", "source", "default_value", "position"]);
        row["dashboard"] = json!(copy_id);
        db::create::<Record>("prism_dashboard_filter", &row)?;
    }
    Ok(copy)
}

// ---- items ----------------------------------------------------------------------------------

/// The dashboard's items, each with its chart's name and type (one call to Prism for all of them).
pub fn list_items(input: Id) -> Result<Vec<Value>> {
    require_viewer()?;
    let dashboard = require("prism_dashboard", &input.id, "dashboard")?;
    let items = sorted("prism_dashboard_item", id_of(&dashboard)?)?;
    let ids: Vec<&str> = items.iter().filter_map(|item| text(item, "chart")).filter(|id| !id.is_empty()).collect();
    let charts = if ids.is_empty() {
        json!({ "rows": [] })
    } else {
        plugins::call::<Value>("prism", "search_charts", &json!({ "filter": { "id": { "in": ids } }, "limit": 200 }))?
    };
    let charts = charts.get("rows").and_then(Value::as_array).cloned().unwrap_or_default();
    Ok(items
        .into_iter()
        .map(|mut item| {
            let chart = charts.iter().find(|chart| chart.get("id") == item.get("chart"));
            if text(&item, "chart").map_or(true, str::is_empty) {
                item["chart_name"] = json!(text(&item, "text").map(|note| note.lines().next().unwrap_or("")).unwrap_or(""));
                item["chart_kind"] = json!("note");
            } else {
                item["chart_name"] = json!(chart.and_then(|c| text(c, "name")).unwrap_or("(chart not available)"));
                item["chart_kind"] = json!(chart.and_then(|c| text(c, "kind")));
            }
            item
        })
        .collect())
}

pub fn add_item(input: Value) -> Result<Record> {
    require_analyst()?;
    // The page `/prism/dashboards/{id}/items/new` sends the dashboard as `id`.
    let dashboard = text(&input, "dashboard").or_else(|| text(&input, "id")).ok_or_else(|| Error::msg("choose a dashboard"))?;
    let dashboard = require("prism_dashboard", dashboard, "dashboard")?;
    let key = id_of(&dashboard)?.to_string();
    let chart = required(&input, "chart", "the chart")?;
    let position = sorted("prism_dashboard_item", &key)?
        .iter()
        .filter_map(|item| item.get("position").and_then(Value::as_i64))
        .max()
        .map_or(0, |last| last + 1);
    let mut data = pick(&input, &["title", "width"]);
    data["dashboard"] = json!(key);
    data["chart"] = json!(chart);
    data["position"] = json!(position);
    db::create::<Record>("prism_dashboard_item", &data)
}

/// A block of text between charts.
pub fn add_note(input: Value) -> Result<Record> {
    require_analyst()?;
    let dashboard = text(&input, "dashboard").or_else(|| text(&input, "id")).ok_or_else(|| Error::msg("choose a dashboard"))?;
    let dashboard = require("prism_dashboard", dashboard, "dashboard")?;
    let key = id_of(&dashboard)?.to_string();
    let note = required(&input, "text", "the note")?;
    let position = sorted("prism_dashboard_item", &key)?
        .iter()
        .filter_map(|item| item.get("position").and_then(Value::as_i64))
        .max()
        .map_or(0, |last| last + 1);
    let mut data = pick(&input, &["title", "width"]);
    data["dashboard"] = json!(key);
    data["text"] = json!(note);
    data["position"] = json!(position);
    if data.get("width").is_none() {
        data["width"] = json!("12");
    }
    db::create::<Record>("prism_dashboard_item", &data)
}

pub fn set_item(input: Value) -> Result<Record> {
    require_analyst()?;
    let id = required(&input, "id", "the item")?;
    db::update::<Record>("prism_dashboard_item", &id, &pick(&input, &["title", "text", "width"]))?
        .ok_or_else(|| Error::msg("you cannot change that item"))
}

pub fn remove_item(input: Id) -> Result<Value> {
    require_analyst()?;
    db::delete::<Record>("prism_dashboard_item", &input.id)?;
    Ok(json!({ "deleted": 1 }))
}

#[derive(Deserialize)]
pub struct Move {
    pub id: String,
    /// `-1` earlier, `1` later.
    pub delta: i64,
}

pub fn move_item(input: Move) -> Result<Value> {
    require_analyst()?;
    let item = require("prism_dashboard_item", &input.id, "item")?;
    let siblings = sorted("prism_dashboard_item", text(&item, "dashboard").unwrap_or(""))?;
    let at = siblings.iter().position(|row| row.get("id") == item.get("id")).unwrap_or(0) as i64;
    let to = (at + input.delta.signum()).clamp(0, siblings.len() as i64 - 1);
    if to == at {
        return Ok(json!({ "moved": false }));
    }
    // Renumber so positions stay distinct, then swap the two.
    let mut order: Vec<&Record> = siblings.iter().collect();
    order.swap(at as usize, to as usize);
    for (position, row) in order.iter().enumerate() {
        if row.get("position").and_then(Value::as_i64) != Some(position as i64) {
            db::update::<Record>("prism_dashboard_item", id_of(row)?, &json!({ "position": position }))?;
        }
    }
    Ok(json!({ "moved": true }))
}

/// One step earlier in the dashboard, for a row action (which sends only the row's id).
pub fn item_up(input: Id) -> Result<Value> {
    move_item(Move { id: input.id, delta: -1 })
}

/// One step later in the dashboard.
pub fn item_down(input: Id) -> Result<Value> {
    move_item(Move { id: input.id, delta: 1 })
}

// ---- filters --------------------------------------------------------------------------------

const FILTER_FIELDS: &[&str] = &["name", "label", "kind", "field", "source", "default_value"];

pub fn list_filters(input: Id) -> Result<Vec<Record>> {
    require_viewer()?;
    let dashboard = require("prism_dashboard", &input.id, "dashboard")?;
    sorted("prism_dashboard_filter", id_of(&dashboard)?)
}

pub fn add_filter(input: Value) -> Result<Record> {
    require_analyst()?;
    let dashboard = text(&input, "dashboard").or_else(|| text(&input, "id")).ok_or_else(|| Error::msg("choose a dashboard"))?;
    let dashboard = require("prism_dashboard", dashboard, "dashboard")?;
    let key = id_of(&dashboard)?.to_string();
    let name = required(&input, "name", "the name")?;
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(Error::msg("the name uses letters, digits and _ only"));
    }
    let existing = sorted("prism_dashboard_filter", &key)?;
    if existing.iter().any(|filter| text(filter, "name") == Some(name.as_str())) {
        return Err(Error::msg(format!("this dashboard already has a filter named `{name}`")));
    }
    required(&input, "label", "the label")?;
    if text(&input, "kind") == Some("select") && text(&input, "source").is_none() {
        return Err(Error::msg("a value picker needs the data its values come from"));
    }
    let mut data = pick(&input, FILTER_FIELDS);
    data["dashboard"] = json!(key);
    data["position"] = json!(existing.len());
    db::create::<Record>("prism_dashboard_filter", &data)
}

pub fn set_filter(input: Value) -> Result<Record> {
    require_analyst()?;
    let id = required(&input, "id", "the filter")?;
    db::update::<Record>("prism_dashboard_filter", &id, &pick(&input, &["label", "field", "source", "default_value"]))?
        .ok_or_else(|| Error::msg("you cannot change that filter"))
}

pub fn remove_filter(input: Id) -> Result<Value> {
    require_analyst()?;
    db::delete::<Record>("prism_dashboard_filter", &input.id)?;
    Ok(json!({ "deleted": 1 }))
}

trait NullOrMissing {
    fn is_none_or_null(&self) -> bool;
}

impl NullOrMissing for Option<&Value> {
    fn is_none_or_null(&self) -> bool {
        self.map_or(true, Value::is_null)
    }
}
