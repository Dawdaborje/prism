//! Drawing a dashboard: its filter bar and a card for each chart.

use aether_sdk::prelude::*;

use crate::common::{id_of, require, require_viewer, text, Record};
use crate::filters::{self, Active};

#[derive(Deserialize)]
pub struct BoardInput {
    pub id: String,
    /// What the filter bar sent, by filter name.
    #[serde(default)]
    pub filters: serde_json::Map<String, Value>,
}

pub fn sorted(model: &str, dashboard: &str) -> Result<Vec<Record>> {
    let mut rows = db::find::<Record>(model).filter("dashboard", dashboard).limit(500).all()?;
    rows.sort_by_key(|row| row.get("position").and_then(Value::as_i64).unwrap_or(0));
    Ok(rows)
}

/// The filter bar as the board view draws it, with each picker's values.
fn descriptors(active: &[Active]) -> Vec<Value> {
    active
        .iter()
        .map(|filter| {
            let mut bar = json!({ "name": filter.name, "label": filter.label, "kind": filter.kind, "value": filter.value });
            if filter.kind == "select" {
                bar["options"] = options_for(filter);
            }
            bar
        })
        .collect()
}

fn options_for(filter: &Active) -> Value {
    let Some(source) = filter.source.as_deref().filter(|s| !s.is_empty()) else {
        return json!([]);
    };
    let values = plugins::call::<Value>(
        "prism",
        "field_values",
        &json!({ "source": source, "field": filter.field, "limit": 100 }),
    );
    // A picker that cannot load its values is empty rather than breaking the dashboard.
    values.unwrap_or_else(|_| json!([]))
}

pub fn dashboard_board(input: BoardInput) -> Result<Value> {
    require_viewer()?;
    let dashboard = require("prism_dashboard", &input.id, "dashboard")?;
    let key = id_of(&dashboard)?.to_string();
    let stored = sorted("prism_dashboard_filter", &key)?;
    let active = filters::active(&stored, &input.filters);
    let controls = filters::controls(&active);
    let items = sorted("prism_dashboard_item", &key)?;
    let cards: Vec<Value> = items
        .iter()
        .map(|item| {
            let width = text(item, "width").and_then(|w| w.parse::<u8>().ok()).unwrap_or(6);
            let chart = text(item, "chart").unwrap_or("");
            if chart.is_empty() {
                let card = json!({ "kind": "text", "title": text(item, "title"), "text": text(item, "text").unwrap_or("") });
                return json!({ "id": item.get("id"), "width": width, "card": card });
            }
            let mut cell = json!({ "id": item.get("id"), "chart": chart, "width": width });
            match plugins::call::<Value>("prism", "run_chart", &json!({ "chart": chart, "controls": controls })) {
                Ok(mut card) => {
                    if let Some(title) = text(item, "title") {
                        card["title"] = json!(title);
                    }
                    cell["card"] = card;
                }
                Err(error) => {
                    cell["title"] = json!(text(item, "title"));
                    cell["error"] = json!(error.to_string());
                }
            }
            cell
        })
        .collect();
    Ok(json!({
        "title": text(&dashboard, "name"),
        "description": text(&dashboard, "description"),
        "refresh_secs": dashboard.get("refresh_secs").and_then(Value::as_i64).unwrap_or(0),
        "filters": descriptors(&active),
        "items": cards,
    }))
}

#[derive(Deserialize)]
pub struct DrillInput {
    /// The dashboard's id from the page's route.
    #[serde(alias = "id")]
    pub dashboard: String,
    pub chart: String,
    #[serde(default)]
    pub segment: serde_json::Map<String, Value>,
    #[serde(default)]
    pub filters: serde_json::Map<String, Value>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

/// The records behind a bar, slice or cell of one dashboard chart, under the dashboard's filters.
pub fn dashboard_drill(input: DrillInput) -> Result<Value> {
    require_viewer()?;
    let dashboard = require("prism_dashboard", &input.dashboard, "dashboard")?;
    let stored = sorted("prism_dashboard_filter", id_of(&dashboard)?)?;
    let controls = filters::controls(&filters::active(&stored, &input.filters));
    plugins::call(
        "prism",
        "drill_chart",
        &json!({
            "chart": input.chart,
            "controls": controls,
            "segment": input.segment,
            "limit": input.limit,
            "offset": input.offset,
        }),
    )
}
