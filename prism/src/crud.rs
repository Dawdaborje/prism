//! Workbooks, metrics and charts: the records a person builds.
//!
//! Who may see or change which record is the kernel's job (`rules/`): a private workbook, and the
//! charts in it, are the owner's and the administrators'; a shared one is read by every Prism role.

use aether_sdk::prelude::*;
use aether_sdk::records::pick;

use crate::common::{id_of, me, require, require_analyst, require_viewer, text, Record};
use crate::dsl;
use crate::engine::{ChartSpec, KINDS};

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

fn search(model: &str, fields: &[&str], extra: db::Filter, input: Search) -> Result<Value> {
    let filter = match input.filter {
        Some(value @ Value::Object(_)) => extra.and(db::Filter::from_json(value)),
        _ => extra,
    };
    db::search(
        model,
        input.q.as_deref(),
        fields,
        filter,
        input.order.as_deref().or(Some("name")),
        Some(input.limit.unwrap_or(50).min(200)),
        input.offset,
    )
}

fn required(input: &Value, field: &str, what: &str) -> Result<String> {
    text(input, field)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::msg(format!("{what} is required")))
}

// ---- workbooks ----------------------------------------------------------------------------

const WORKBOOK_FIELDS: &[&str] = &["name", "description", "shared", "archived"];

pub fn create_workbook(input: Value) -> Result<Record> {
    require_analyst()?;
    required(&input, "name", "the name")?;
    let mut data = pick(&input, WORKBOOK_FIELDS);
    data["owner"] = json!(me()?);
    db::create::<Record>("prism_workbook", &data)
}

pub fn set_workbook(input: Value) -> Result<Record> {
    require_analyst()?;
    let id = required(&input, "id", "the workbook")?;
    db::update::<Record>("prism_workbook", &id, &pick(&input, WORKBOOK_FIELDS))?
        .ok_or_else(|| Error::msg("you cannot change that workbook"))
}

pub fn get_workbook(input: Id) -> Result<Record> {
    require_viewer()?;
    require("prism_workbook", &input.id, "workbook")
}

pub fn search_workbooks(input: Search) -> Result<Value> {
    require_viewer()?;
    search("prism_workbook", &["name", "description"], db::Filter::eq("archived", false), input)
}

pub fn delete_workbook(input: Id) -> Result<Value> {
    require_analyst()?;
    require("prism_workbook", &input.id, "workbook")?;
    let charts = db::find::<Record>("prism_chart").filter("workbook", input.id.as_str()).limit(1000).all()?;
    for chart in &charts {
        db::delete::<Record>("prism_chart", id_of(chart)?)?;
    }
    db::delete::<Record>("prism_workbook", &input.id)?;
    Ok(json!({ "deleted": charts.len() + 1 }))
}

pub fn list_workbook_charts(input: Id) -> Result<Vec<Record>> {
    require_viewer()?;
    let mut charts = db::find::<Record>("prism_chart").filter("workbook", input.id.as_str()).limit(500).all()?;
    charts.sort_by_key(|chart| (text(chart, "folder").unwrap_or("").to_lowercase(), text(chart, "name").unwrap_or("").to_lowercase()));
    Ok(charts)
}

// ---- metrics ------------------------------------------------------------------------------

const METRIC_FIELDS: &[&str] = &["code", "name", "source", "measure", "format", "currency", "description"];

/// A metric holds exactly one measure, which must parse.
fn check_metric(input: &Value) -> Result<()> {
    let measure = required(input, "measure", "the measure")?;
    let parsed = dsl::parse_measures(&format!("m = {measure}"), &|_| Err(Error::msg("a metric cannot use another metric")))?;
    if parsed.measures.len() != 1 || !parsed.formulas.is_empty() {
        return Err(Error::msg("a metric is one measure such as sum(amount), not a formula"));
    }
    Ok(())
}

pub fn create_metric(input: Value) -> Result<Record> {
    require_analyst()?;
    for (field, what) in [("code", "the code"), ("name", "the name"), ("source", "the source")] {
        required(&input, field, what)?;
    }
    let code = required(&input, "code", "the code")?;
    if !code.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(Error::msg("the code uses letters, digits and _ only"));
    }
    check_metric(&input)?;
    let mut data = pick(&input, METRIC_FIELDS);
    data["owner"] = json!(me()?);
    db::create::<Record>("prism_metric", &data)
}

pub fn set_metric(input: Value) -> Result<Record> {
    require_analyst()?;
    let id = required(&input, "id", "the metric")?;
    if input.get("measure").is_some() {
        check_metric(&input)?;
    }
    let mut changes = pick(&input, METRIC_FIELDS);
    if let Some(object) = changes.as_object_mut() {
        object.remove("code");
    }
    db::update::<Record>("prism_metric", &id, &changes)?.ok_or_else(|| Error::msg("there is no such metric"))
}

pub fn get_metric(input: Id) -> Result<Record> {
    require_viewer()?;
    require("prism_metric", &input.id, "metric")
}

pub fn search_metrics(input: Search) -> Result<Value> {
    require_viewer()?;
    search("prism_metric", &["code", "name", "source"], db::Filter::all(), input)
}

pub fn delete_metric(input: Id) -> Result<Value> {
    require_analyst()?;
    db::delete::<Record>("prism_metric", &input.id)?;
    Ok(json!({ "deleted": 1 }))
}

// ---- charts -------------------------------------------------------------------------------

const CHART_FIELDS: &[&str] = &[
    "name", "folder", "kind", "source", "dimensions", "measures", "filter", "sort", "row_limit", "date_field", "period",
    "compare", "target", "lower_is_better", "guide", "format", "currency", "description", "info",
];

/// Everything that can be checked without reading data: the type, the text of the measures and
/// dimensions, the filter's JSON.
pub fn check_chart(input: &Value) -> Result<()> {
    if let Some(kind) = text(input, "kind") {
        if !KINDS.contains(&kind) {
            return Err(Error::msg(format!("`{kind}` is not a chart type")));
        }
    }
    if let Some(guide) = text(input, "guide") {
        if !["none", "average", "median", "trend"].contains(&guide) {
            return Err(Error::msg(format!("`{guide}` is not a guide line")));
        }
    }
    dsl::parse_dimensions(text(input, "dimensions").unwrap_or(""))?;
    let lookup = |code: &str| -> Result<String> {
        let metric = db::find::<Record>("prism_metric")
            .filter("code", code)
            .first()?
            .ok_or_else(|| Error::msg(format!("there is no metric `{code}`")))?;
        Ok(text(&metric, "measure").unwrap_or("count").to_string())
    };
    dsl::parse_measures(text(input, "measures").unwrap_or(""), &lookup)?;
    if let Some(filter) = text(input, "filter") {
        match serde_json::from_str::<Value>(filter) {
            Ok(Value::Object(_)) => {}
            _ => return Err(Error::msg("the filter must be a JSON object, such as {\"active\": true}")),
        }
    }
    Ok(())
}

pub fn create_chart(input: Value) -> Result<Record> {
    require_analyst()?;
    // The page `/prism/workbooks/{id}/charts/new` sends the workbook as `id`.
    let workbook = text(&input, "workbook").or_else(|| text(&input, "id")).ok_or_else(|| Error::msg("choose a workbook"))?;
    require("prism_workbook", workbook, "workbook")?;
    required(&input, "name", "the name")?;
    required(&input, "source", "the data")?;
    check_chart(&input)?;
    let mut data = pick(&input, CHART_FIELDS);
    data["workbook"] = json!(workbook);
    db::create::<Record>("prism_chart", &data)
}

pub fn set_chart(input: Value) -> Result<Record> {
    require_analyst()?;
    let id = required(&input, "id", "the chart")?;
    let current = require("prism_chart", &id, "chart")?;
    let mut merged = current;
    if let (Some(base), Some(changes)) = (merged.as_object_mut(), pick(&input, CHART_FIELDS).as_object()) {
        for (key, value) in changes {
            base.insert(key.clone(), value.clone());
        }
    }
    check_chart(&merged)?;
    db::update::<Record>("prism_chart", &id, &pick(&input, CHART_FIELDS))?
        .ok_or_else(|| Error::msg("you cannot change that chart"))
}

pub fn get_chart(input: Id) -> Result<Record> {
    require_viewer()?;
    require("prism_chart", &input.id, "chart")
}

pub fn search_charts(input: Search) -> Result<Value> {
    require_viewer()?;
    search("prism_chart", &["name", "description", "source"], db::Filter::all(), input)
}

pub fn delete_chart(input: Id) -> Result<Value> {
    require_analyst()?;
    require("prism_chart", &input.id, "chart")?;
    db::delete::<Record>("prism_chart", &input.id)?;
    Ok(json!({ "deleted": 1 }))
}

/// A copy in the same workbook, named "Copy of …", for a person to change.
pub fn duplicate_chart(input: Id) -> Result<Record> {
    require_analyst()?;
    let chart = require("prism_chart", &input.id, "chart")?;
    let mut data = pick(&chart, CHART_FIELDS);
    data["workbook"] = chart["workbook"].clone();
    data["name"] = json!(format!("Copy of {}", text(&chart, "name").unwrap_or("chart")));
    db::create::<Record>("prism_chart", &data)
}

/// A stored chart, with the changes of an inline `spec` on top when the builder previews edits.
pub fn resolve(chart: Option<&str>, spec: Option<&Value>) -> Result<ChartSpec> {
    let mut base = match chart {
        Some(id) => require("prism_chart", id, "chart")?,
        None => json!({}),
    };
    if let (Some(base), Some(spec)) = (base.as_object_mut(), spec.and_then(Value::as_object)) {
        for (key, value) in spec {
            base.insert(key.clone(), value.clone());
        }
    }
    if chart.is_none() {
        require_analyst()?;
    }
    Ok(serde_json::from_value(base)?)
}
