//! What pages and other plugins call to read charts: sources, a chart's result, its records and
//! its download.

use aether_sdk::prelude::*;

use crate::common::require_viewer;
use crate::controls::{Control, DATE};
use crate::crud::resolve;
use crate::engine::{self, ChartSpec};
use crate::source::{self, SourceInfo};

#[derive(Deserialize, Default)]
pub struct Sources {
    #[serde(default)]
    pub q: Option<String>,
}

/// The data a person can build on, for a picker: `{ id, name }` rows.
pub fn search_sources(input: Sources) -> Result<Vec<Value>> {
    require_viewer()?;
    let needle = input.q.as_deref().map(str::trim).map(str::to_lowercase).filter(|q| !q.is_empty());
    let mut rows: Vec<Value> = source::list_names()?
        .into_iter()
        .filter_map(|source| {
            let name = source.get("name")?.as_str()?.to_string();
            let model = source.get("model")?.as_str()?;
            let label = match source.get("plugin").and_then(Value::as_str) {
                Some(plugin) => format!("{} ({plugin})", source::humanize(model)),
                None => source::humanize(model),
            };
            if needle.as_ref().is_some_and(|q| !name.to_lowercase().contains(q) && !label.to_lowercase().contains(q)) {
                return None;
            }
            Some(json!({ "id": name, "name": label }))
        })
        .collect();
    rows.sort_by_key(|row| row["name"].as_str().unwrap_or("").to_lowercase());
    rows.truncate(200);
    Ok(rows)
}

#[derive(Deserialize)]
pub struct SourceRef {
    pub source: String,
}

/// The fields of one source, as the person may see them: what the builder offers.
pub fn get_source(input: SourceRef) -> Result<Value> {
    require_viewer()?;
    let info: SourceInfo = source::load(&input.source)?;
    let fields: Vec<Value> = info
        .fields
        .iter()
        .map(|field| {
            json!({
                "name": field.name,
                "label": field.label,
                "kind": field.kind,
                "link": info.link_model(field),
                "options": field.options.iter().map(|(value, label)| json!({ "value": value, "label": label })).collect::<Vec<_>>(),
                "groupable": !matches!(field.kind.as_str(), "text"),
                "numeric": matches!(field.kind.as_str(), "int" | "float" | "decimal"),
                "temporal": field.kind == "date" || field.kind == "datetime",
            })
        })
        .collect();
    Ok(json!({ "name": info.name, "fields": fields }))
}

/// A chart to run: a stored one (`chart`), an unsaved one (`spec`), or a stored one with edits.
#[derive(Deserialize, Default)]
pub struct RunChart {
    #[serde(default)]
    pub chart: Option<String>,
    #[serde(default)]
    pub spec: Option<Value>,
    #[serde(default)]
    pub controls: Vec<Control>,
}

pub fn run_chart(input: RunChart) -> Result<Value> {
    require_viewer()?;
    let spec = resolve(input.chart.as_deref(), input.spec.as_ref())?;
    engine::run(&spec, &input.controls)
}

#[derive(Deserialize)]
pub struct DrillChart {
    #[serde(default)]
    pub chart: Option<String>,
    #[serde(default)]
    pub spec: Option<Value>,
    #[serde(default)]
    pub controls: Vec<Control>,
    /// Dimension field → the raw value of the bar, slice or cell that was clicked.
    #[serde(default)]
    pub segment: serde_json::Map<String, Value>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

pub fn drill_chart(input: DrillChart) -> Result<Value> {
    require_viewer()?;
    let spec = resolve(input.chart.as_deref(), input.spec.as_ref())?;
    engine::drill(&spec, &input.controls, &input.segment, input.limit.unwrap_or(25), input.offset.unwrap_or(0))
}

#[derive(Deserialize)]
pub struct ExportChart {
    pub chart: String,
    #[serde(default)]
    pub controls: Vec<Control>,
    #[serde(default)]
    pub format: Option<String>,
}

/// Excel or PDF of what the chart shows.
pub fn export_chart(input: ExportChart) -> Result<Value> {
    require_viewer()?;
    let spec: ChartSpec = resolve(Some(&input.chart), None)?;
    let card = engine::run(&spec, &input.controls)?;
    let (columns, rows) = table_of(&card);
    let title = spec.name.clone().unwrap_or_else(|| "Chart".to_string());
    report::export(&json!({
        "format": input.format.as_deref().unwrap_or("xlsx"),
        "title": title,
        "columns": columns,
        "rows": rows,
    }))
}

/// A chart result as flat columns and rows, whatever its kind.
pub fn table_of(card: &Value) -> (Vec<Value>, Vec<Value>) {
    let text = |value: &Value| -> Value {
        match value {
            Value::String(_) | Value::Number(_) | Value::Bool(_) => value.clone(),
            Value::Null => Value::Null,
            other => json!(other.to_string()),
        }
    };
    match card["kind"].as_str().unwrap_or("") {
        "table" => {
            let columns: Vec<Value> = card["columns"]
                .as_array()
                .map(|list| list.iter().map(|c| json!({ "field": c["key"], "label": c["label"] })).collect())
                .unwrap_or_default();
            let rows = card["rows"].as_array().map(|list| list.iter().map(|row| row["cells"].clone()).collect()).unwrap_or_default();
            (columns, rows)
        }
        "pivot" | "heatmap" => {
            let mut columns = vec![json!({ "field": "row", "label": "" })];
            let labels: Vec<String> = card["columns"].as_array().map(|l| l.iter().map(|v| v.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default();
            for (index, label) in labels.iter().enumerate() {
                columns.push(json!({ "field": format!("c{index}"), "label": label }));
            }
            columns.push(json!({ "field": "total", "label": "Total" }));
            let rows = card["rows"]
                .as_array()
                .map(|names| {
                    names
                        .iter()
                        .enumerate()
                        .map(|(r, name)| {
                            let mut row = serde_json::Map::new();
                            row.insert("row".into(), name.clone());
                            for index in 0..labels.len() {
                                row.insert(format!("c{index}"), card["matrix"][r][index].clone());
                            }
                            row.insert("total".into(), card["row_totals"][r].clone());
                            Value::Object(row)
                        })
                        .collect()
                })
                .unwrap_or_default();
            (columns, rows)
        }
        "bubble" => {
            let axes = &card["axes"];
            let mut columns = vec![
                json!({ "field": "label", "label": card["dimensions"][0] }),
                json!({ "field": "x", "label": axes["x"] }),
                json!({ "field": "y", "label": axes["y"] }),
            ];
            if axes["size"].is_string() {
                columns.push(json!({ "field": "size", "label": axes["size"] }));
            }
            let rows = card["points"].as_array().cloned().unwrap_or_default();
            (columns, rows.into_iter().map(|point| json!({ "label": point["label"], "x": point["x"], "y": point["y"], "size": point["size"] })).collect())
        }
        "sankey" => {
            let nodes = card["nodes"].as_array().cloned().unwrap_or_default();
            let name = |index: &Value| index.as_u64().and_then(|i| nodes.get(i as usize)).map(|n| n["name"].clone()).unwrap_or(Value::Null);
            let columns = vec![
                json!({ "field": "from", "label": "From" }),
                json!({ "field": "to", "label": "To" }),
                json!({ "field": "value", "label": card["measure"] }),
            ];
            let rows = card["links"]
                .as_array()
                .map(|list| list.iter().map(|link| json!({ "from": name(&link["source"]), "to": name(&link["target"]), "value": link["value"] })).collect())
                .unwrap_or_default();
            (columns, rows)
        }
        "number" => {
            let columns = vec![
                json!({ "field": "name", "label": "Measure" }),
                json!({ "field": "value", "label": "Value" }),
                json!({ "field": "previous", "label": "Previous period" }),
                json!({ "field": "change", "label": "Change" }),
            ];
            let rows = card["readings"]
                .as_array()
                .map(|list| list.iter().map(|r| json!({ "name": r["label"], "value": r["value"], "previous": r["previous"], "change": r["change"] })).collect())
                .unwrap_or_default();
            (columns, rows)
        }
        _ => {
            let series = card["series"].as_array().cloned().unwrap_or_default();
            let mut columns = vec![json!({ "field": "category", "label": card["dimensions"][0] })];
            for (index, s) in series.iter().enumerate() {
                columns.push(json!({ "field": format!("s{index}"), "label": s["name"] }));
            }
            let rows = card["categories"]
                .as_array()
                .map(|names| {
                    names
                        .iter()
                        .enumerate()
                        .map(|(i, name)| {
                            let mut row = serde_json::Map::new();
                            row.insert("category".into(), text(name));
                            for (index, s) in series.iter().enumerate() {
                                row.insert(format!("s{index}"), s["values"][i].clone());
                            }
                            Value::Object(row)
                        })
                        .collect()
                })
                .unwrap_or_default();
            (columns, rows)
        }
    }
}

#[derive(Deserialize)]
pub struct FieldValues {
    pub source: String,
    pub field: String,
    #[serde(default)]
    pub q: Option<String>,
    #[serde(default)]
    pub limit: Option<u32>,
}

/// The values a field takes in a source, most common first, with readable labels: what a
/// dashboard filter offers. Only values in records the person may read appear.
pub fn field_values(input: FieldValues) -> Result<Vec<Value>> {
    require_viewer()?;
    let info = source::load(&input.source)?;
    let field = info
        .field(&input.field)
        .ok_or_else(|| Error::msg(format!("`{}` is not a field of {}", input.field, info.name)))?;
    let answer = report::query(&json!({
        "model": info.name,
        "dimensions": [field.name],
        "measures": { "n": "count" },
        "order": "-n",
        "limit": 500,
    }))?;
    let rows = answer.get("rows").and_then(Value::as_array).cloned().unwrap_or_default();
    let labels = engine::Labels::build(&info, &[json!(field.name)], &rows);
    let needle = input.q.as_deref().map(str::trim).map(str::to_lowercase).filter(|q| !q.is_empty());
    Ok(rows
        .iter()
        .filter_map(|row| {
            let raw = row.get(&field.name).cloned().unwrap_or(Value::Null);
            if raw.is_null() {
                return None;
            }
            let label = labels.label(&field.name, &raw);
            if needle.as_ref().is_some_and(|q| !label.to_lowercase().contains(q)) {
                return None;
            }
            Some(json!({ "value": raw, "label": label, "count": row.get("n") }))
        })
        .take(input.limit.unwrap_or(100).min(200) as usize)
        .collect())
}

// ---- one chart as a board -------------------------------------------------------------------

#[derive(Deserialize, Default)]
pub struct ChartBoard {
    pub id: String,
    /// What the board's filter bar sent: `{ "date": "this_month" }` or `{ "date": { from, to } }`.
    #[serde(default)]
    pub filters: serde_json::Map<String, Value>,
}

/// A chart on its own page, in the shape the `board` view draws: a date filter when the chart has a
/// date, and the chart's card. A chart that fails to run shows its error in place of the card.
pub fn chart_board(input: ChartBoard) -> Result<Value> {
    require_viewer()?;
    let spec = resolve(Some(&input.id), None)?;
    let mut filters = Vec::new();
    let mut controls = Vec::new();
    if spec.date_field.as_deref().is_some_and(|field| !field.is_empty()) {
        filters.push(json!({ "name": "date", "label": "Date", "kind": "period", "value": input.filters.get("date") }));
        if let Some(control) = date_control(input.filters.get("date")) {
            controls.push(control);
        }
    }
    let item = match engine::run(&spec, &controls) {
        Ok(card) => json!({ "id": input.id, "width": 12, "card": card }),
        Err(error) => json!({ "id": input.id, "width": 12, "title": spec.name, "error": error.to_string() }),
    };
    Ok(json!({ "title": spec.name, "filters": filters, "items": [item] }))
}

#[derive(Deserialize)]
pub struct ChartDrill {
    /// The chart's id from the page's route.
    pub id: String,
    #[serde(default)]
    pub segment: serde_json::Map<String, Value>,
    #[serde(default)]
    pub filters: serde_json::Map<String, Value>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub offset: Option<u32>,
}

/// The records behind a bar of a chart opened on its own page, under that page's date filter.
pub fn chart_drill(input: ChartDrill) -> Result<Value> {
    require_viewer()?;
    let spec = resolve(Some(&input.id), None)?;
    let controls: Vec<Control> = date_control(input.filters.get("date")).into_iter().collect();
    engine::drill(&spec, &controls, &input.segment, input.limit.unwrap_or(25), input.offset.unwrap_or(0))
}

/// A date control from a filter bar value: a period name, or `{ from, to }`.
pub fn date_control(value: Option<&Value>) -> Option<Control> {
    let mut control = Control { field: DATE.to_string(), ..Control::default() };
    match value? {
        Value::String(period) if !period.is_empty() => control.period = Some(period.clone()),
        Value::Object(range) => {
            control.from = range.get("from").cloned();
            control.to = range.get("to").cloned();
        }
        _ => return None,
    }
    Some(control).filter(|control| !control.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_filter_bar_value_becomes_a_date_control() {
        let named = date_control(Some(&json!("this_month"))).unwrap();
        assert_eq!(named.period.as_deref(), Some("this_month"));
        assert_eq!(named.field, "$date");
        let range = date_control(Some(&json!({ "from": "2026-01-01", "to": "2026-01-31" }))).unwrap();
        assert_eq!(range.from, Some(json!("2026-01-01")));
        assert!(date_control(Some(&json!(""))).is_none());
        assert!(date_control(Some(&json!({ "from": "", "to": "" }))).is_none());
        assert!(date_control(None).is_none());
    }

    #[test]
    fn a_series_chart_flattens_to_a_table() {
        let card = json!({
            "kind": "bar", "dimensions": ["stage", null],
            "categories": ["Won", "Lost"],
            "series": [{ "name": "Deals", "values": [3.0, 1.0] }, { "name": "Value", "values": [30.0, 5.0] }]
        });
        let (columns, rows) = table_of(&card);
        assert_eq!(columns.len(), 3);
        assert_eq!(rows[0], json!({ "category": "Won", "s0": 3.0, "s1": 30.0 }));
        assert_eq!(rows[1]["category"], "Lost");
    }

    #[test]
    fn a_flow_chart_flattens_to_from_and_to() {
        let card = json!({
            "kind": "sankey", "measure": "n",
            "nodes": [{ "name": "Web" }, { "name": "Won" }],
            "links": [{ "source": 0, "target": 1, "value": 4.0 }]
        });
        let (columns, rows) = table_of(&card);
        assert_eq!(columns.len(), 3);
        assert_eq!(rows[0], json!({ "from": "Web", "to": "Won", "value": 4.0 }));
    }

    #[test]
    fn a_bubble_chart_flattens_with_its_axes() {
        let card = json!({
            "kind": "bubble", "dimensions": ["team", null],
            "axes": { "x": "Deals", "y": "Value", "size": null },
            "points": [{ "label": "North", "x": 3.0, "y": 9.0, "size": null }]
        });
        let (columns, rows) = table_of(&card);
        assert_eq!(columns.len(), 3);
        assert_eq!(rows[0]["y"], 9.0);
    }

    #[test]
    fn a_pivot_flattens_with_its_totals() {
        let card = json!({
            "kind": "pivot", "rows": ["A", "B"], "columns": ["x", "y"],
            "matrix": [[1.0, 2.0], [3.0, 4.0]], "row_totals": [3.0, 7.0]
        });
        let (columns, rows) = table_of(&card);
        assert_eq!(columns.len(), 4);
        assert_eq!(rows[1], json!({ "row": "B", "c0": 3.0, "c1": 4.0, "total": 7.0 }));
    }
}
