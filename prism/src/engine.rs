//! Running a chart: plan the query, ask the report engine, shape the answer for the screen.
//!
//! Everything the person may not read is already gone by the time rows come back: the kernel
//! applies the source model's record rules and hidden fields to `report::query` and `db::find`,
//! so a number here never includes a record, or a field, they could not open themselves.

use std::collections::HashMap;

use aether_sdk::prelude::*;
use chrono::{Duration, NaiveDate};
use serde::Deserialize;

use crate::common::{number, Record};
use crate::controls::{self, Control};
use crate::dsl::{self, Parsed};
use crate::periods;
use crate::source::{self, SourceInfo};

pub const KINDS: &[&str] = &[
    "number", "bar", "stacked_bar", "row", "line", "area", "pie", "donut", "funnel", "map", "bubble", "sankey", "table", "pivot",
    "heatmap",
];
const SPLITTABLE: &[&str] = &["bar", "stacked_bar", "row", "line", "area"];
const MAX_SERIES: usize = 8;

/// A chart as stored (or as the builder previews it before it is saved).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ChartSpec {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub dimensions: Option<String>,
    #[serde(default)]
    pub measures: Option<String>,
    #[serde(default)]
    pub filter: Option<String>,
    #[serde(default)]
    pub sort: Option<String>,
    #[serde(default)]
    pub row_limit: Option<i64>,
    #[serde(default)]
    pub date_field: Option<String>,
    #[serde(default)]
    pub period: Option<String>,
    #[serde(default)]
    pub compare: Option<bool>,
    #[serde(default)]
    pub target: Option<f64>,
    /// A fall is the good news (cost, days to resolve): colours and goal checks turn around.
    #[serde(default)]
    pub lower_is_better: Option<bool>,
    /// A line drawn over a bar, line or area chart: `average`, `median` or `trend`.
    #[serde(default)]
    pub guide: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub info: Option<String>,
}

impl ChartSpec {
    pub fn kind(&self) -> &str {
        self.kind.as_deref().filter(|kind| !kind.is_empty()).unwrap_or("bar")
    }

    fn limit(&self) -> u32 {
        self.row_limit.unwrap_or(12).clamp(1, 1000) as u32
    }

    fn date_field(&self) -> Option<&str> {
        self.date_field.as_deref().map(str::trim).filter(|field| !field.is_empty())
    }

    fn period(&self) -> &str {
        self.period.as_deref().filter(|period| !period.is_empty()).unwrap_or("none")
    }
}

/// The stretch of the date field a chart looks at.
#[derive(Debug, Clone, PartialEq)]
enum Scope {
    All,
    Named(String),
    Range(NaiveDate, NaiveDate),
}

struct Plan {
    spec: ChartSpec,
    kind: String,
    source: SourceInfo,
    dims: Vec<Value>,
    parsed: Parsed,
    /// Filters other than the date scope: the chart's own and the dashboard controls'.
    parts: Vec<Value>,
    scope: Scope,
    today: NaiveDate,
    applied: Vec<String>,
    skipped: Vec<Value>,
}

fn metric_lookup(code: &str) -> Result<String> {
    let metric = db::find::<Record>("prism_metric")
        .filter("code", code)
        .first()?
        .ok_or_else(|| Error::msg(format!("there is no metric `{code}`")))?;
    metric
        .get("measure")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| Error::msg(format!("metric `{code}` has no measure")))
}

fn plan(spec: &ChartSpec, controls: &[Control]) -> Result<Plan> {
    let kind = spec.kind().to_string();
    if !KINDS.contains(&kind.as_str()) {
        return Err(Error::msg(format!("`{kind}` is not a chart type")));
    }
    if spec.source.trim().is_empty() {
        return Err(Error::msg("pick the data this chart reads"));
    }
    let source = source::load(spec.source.trim())?;
    let parsed = dsl::parse_measures(spec.measures.as_deref().unwrap_or(""), &metric_lookup)?;
    let dims = dsl::parse_dimensions(spec.dimensions.as_deref().unwrap_or(""))?;
    for dimension in &dims {
        let field = dsl::dimension_field(dimension);
        if source.field(field).is_none() {
            return Err(unknown_field(&source, field));
        }
        if dimension.is_object() && !source.is_temporal(field) {
            return Err(Error::msg(format!("`{field}` is not a date, so it cannot be grouped by month or year")));
        }
    }
    if let Some(field) = spec.date_field() {
        if !source.is_temporal(field) {
            return Err(Error::msg(format!("`{field}` is not a date field of {}", source.name)));
        }
    }
    let own: Value = match spec.filter.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
        None => json!({}),
        Some(text) => match serde_json::from_str::<Value>(text) {
            Ok(value @ Value::Object(_)) => value,
            _ => return Err(Error::msg("the filter must be a JSON object, such as {\"active\": true}")),
        },
    };
    let today = parse_day(context::current()?.today())?;
    let mut scope = match spec.period() {
        "none" => Scope::All,
        named => Scope::Named(named.to_string()),
    };
    let mut parts = vec![own];
    let (mut applied, mut skipped) = (Vec::new(), Vec::new());
    for control in controls.iter().filter(|control| !control.is_empty()) {
        let targets_date = control.field == controls::DATE || Some(control.field.as_str()) == spec.date_field();
        if targets_date && spec.date_field().is_some() && (control.period.is_some() || control.from.is_some() || control.to.is_some()) {
            if let Some(next) = scope_of(control) {
                scope = next;
                applied.push(control.field.clone());
                continue;
            }
        }
        match controls::to_filter(control, &source, spec.date_field()) {
            Ok(filter) => {
                parts.push(filter);
                applied.push(control.field.clone());
            }
            Err(reason) => skipped.push(json!({ "field": control.field, "reason": reason })),
        }
    }
    Ok(Plan {
        spec: spec.clone(),
        kind,
        source,
        dims,
        parsed,
        parts,
        scope,
        today,
        applied,
        skipped,
    })
}

fn unknown_field(source: &SourceInfo, field: &str) -> Error {
    let names: Vec<&str> = source.fields.iter().map(|field| field.name.as_str()).take(40).collect();
    Error::msg(format!("`{field}` is not a field of {}; you can use: {}", source.name, names.join(", ")))
}

fn parse_day(text: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").map_err(|_| Error::msg("the kernel did not say what day it is"))
}

/// A date control's `period`, or its `from`/`to` days.
fn scope_of(control: &Control) -> Option<Scope> {
    if let Some(period) = control.period.as_deref().filter(|p| !p.is_empty() && *p != "none") {
        return Some(Scope::Named(period.to_string()));
    }
    let day = |value: &Option<Value>| {
        value
            .as_ref()
            .and_then(Value::as_str)
            .and_then(|text| text.get(..10))
            .and_then(|text| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok())
    };
    match (day(&control.from), day(&control.to)) {
        (Some(from), Some(to)) if from <= to => Some(Scope::Range(from, to)),
        _ => None,
    }
}

impl Plan {
    /// The whole filter: the chart's and the controls', plus the date scope or the stretch before it.
    /// `None` when asked for the previous stretch and there is none to compare with.
    fn filter(&self, previous: bool) -> Option<Value> {
        let mut parts = self.parts.clone();
        match (self.spec.date_field(), &self.scope) {
            (_, Scope::All) if previous => return None,
            (Some(field), scope) if *scope != Scope::All => {
                parts.push(self.date_filter(field, scope, previous)?);
            }
            (None, scope) if *scope != Scope::All && previous => return None,
            _ => {}
        }
        Some(controls::all_of(parts))
    }

    fn date_filter(&self, field: &str, scope: &Scope, previous: bool) -> Option<Value> {
        let kind = if self.source.is_temporal(field) {
            self.source.field(field).map(|f| f.kind.as_str()).unwrap_or("datetime")
        } else {
            "datetime"
        };
        let between = |from: NaiveDate, to_exclusive: NaiveDate| {
            json!({ field: { "gte": periods::bound(kind, from), "lt": periods::bound(kind, to_exclusive) } })
        };
        match scope {
            Scope::All => None,
            Scope::Named(name) if !previous => Some(json!({ field: { "in_period": name } })),
            Scope::Named(name) => match periods::previous_name(name) {
                Some(before) => Some(json!({ field: { "in_period": before } })),
                None => periods::previous_days(name, self.today).map(|(from, to)| between(from, to)),
            },
            Scope::Range(from, to) if !previous => Some(between(*from, *to + Duration::days(1))),
            Scope::Range(from, to) => {
                let length = (*to - *from).num_days() + 1;
                Some(between(*from - Duration::days(length), *from))
            }
        }
    }

    /// The figure a card leads with: the first one it shows.
    fn headline(&self) -> &str {
        self.parsed.visible().first().copied().unwrap_or("n")
    }

    /// The grain for a trend over the plan's scope.
    fn trend_grain(&self) -> &'static str {
        match &self.scope {
            Scope::Named(name) => periods::grain_for(name),
            Scope::Range(from, to) if (*to - *from).num_days() <= 62 => "day",
            _ => "month",
        }
    }

    fn query(
        &self,
        dims: &[Value],
        filter: &Value,
        order: Option<&str>,
        limit: u32,
        subtotals: bool,
    ) -> Result<Value> {
        let mut payload = json!({
            "model": self.source.name,
            "filter": filter,
            "dimensions": dims,
            "measures": self.parsed.measures,
            "limit": limit,
        });
        if !self.parsed.formulas.is_empty() {
            payload["formulas"] = json!(self.parsed.formulas);
        }
        if let Some(order) = order {
            payload["order"] = json!(order);
        }
        if subtotals {
            payload["subtotals"] = json!(true);
        }
        report::query(&payload)
    }

    fn meta(&self, filter: &Value, total_groups: Option<i64>) -> Value {
        json!({
            "source": self.source.name,
            "filter": filter,
            "applied": self.applied,
            "skipped": self.skipped,
            "total_groups": total_groups,
        })
    }

    /// `order` for the first dimension: a date reads left to right, anything else by size.
    fn default_order(&self) -> Option<String> {
        let valid = |name: &str| {
            self.parsed.names.iter().any(|n| n == name)
                || self.dims.iter().any(|d| dsl::dimension_field(d) == name)
        };
        if let Some(sort) = self.spec.sort.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            if valid(sort.trim_start_matches('-')) {
                return Some(sort.to_string());
            }
        }
        let first = self.dims.first()?;
        if first.is_object() || self.source.is_temporal(dsl::dimension_field(first)) {
            return Some(dsl::dimension_field(first).to_string());
        }
        Some(format!("-{}", self.headline()))
    }
}

/// Run `spec` for the person calling, with the dashboard's `controls` routed into it.
pub fn run(spec: &ChartSpec, controls: &[Control]) -> Result<Value> {
    let plan = plan(spec, controls)?;
    let mut card = match plan.kind.as_str() {
        "number" => run_number(&plan)?,
        "table" => run_table(&plan)?,
        "pivot" | "heatmap" => run_pivot(&plan)?,
        "bubble" => run_bubble(&plan)?,
        "sankey" => run_sankey(&plan)?,
        _ => run_series(&plan)?,
    };
    let object = card.as_object_mut().ok_or_else(|| Error::msg("the chart did not produce a result"))?;
    object.insert("id".into(), json!(spec.id));
    object.insert("kind".into(), json!(plan.kind));
    object.insert("title".into(), json!(spec.name.clone().unwrap_or_default()));
    object.insert("description".into(), json!(spec.description));
    object.insert("info".into(), json!(spec.info));
    object.insert("format".into(), json!(spec.format.as_deref().unwrap_or("number")));
    object.insert("currency".into(), json!(spec.currency));
    object.insert("guide".into(), json!(spec.guide.as_deref().filter(|g| ["average", "median", "trend"].contains(g))));
    object.insert("lower_is_better".into(), json!(spec.lower_is_better == Some(true)));
    if !object.contains_key("target") {
        object.insert("target".into(), json!(spec.target));
    }
    object.insert("source".into(), json!(plan.source.name));
    Ok(card)
}

// ---- number -----------------------------------------------------------------------------

fn run_number(plan: &Plan) -> Result<Value> {
    let current = plan.filter(false).unwrap_or_else(|| json!({}));
    let answer = plan.query(&[], &current, None, 1, false)?;
    let total = answer.get("total").cloned().unwrap_or(json!({}));
    let previous = if plan.spec.compare == Some(true) {
        match plan.filter(true) {
            Some(filter) => plan
                .query(&[], &filter, None, 1, false)?
                .get("total")
                .cloned()
                .or(Some(json!({}))),
            None => None,
        }
    } else {
        None
    };
    let lower_is_better = plan.spec.lower_is_better == Some(true);
    let readings: Vec<Value> = plan
        .parsed
        .visible()
        .into_iter()
        .map(|name| {
            let value = total.get(name).and_then(number);
            let before = previous.as_ref().map(|row| row.get(name).and_then(number).unwrap_or(0.0));
            let change = change(value, before);
            json!({
                "name": name,
                "label": source::humanize(name),
                "value": value.map(round),
                "previous": before.map(round),
                "change": change,
                "tone": tone(change, lower_is_better),
            })
        })
        .collect();
    let headline = readings.first().and_then(|reading| reading["value"].as_f64());
    let progress = plan
        .spec
        .target
        .filter(|target| *target > 0.0)
        .zip(headline)
        .map(|(target, value)| round(value / target));

    let status = plan
        .spec
        .target
        .zip(headline)
        .map(|(target, value)| if (lower_is_better && value <= target) || (!lower_is_better && value >= target) { "on_track" } else { "behind" });

    let mut trend = Vec::new();
    if let Some(field) = plan.spec.date_field() {
        let grain = plan.trend_grain();
        let (limit, order) = if plan.scope == Scope::All { (if grain == "day" { 30 } else { 12 }, format!("-{field}")) } else { (120, field.to_string()) };
        let dimension = json!({ "field": field, "grain": grain });
        let rows = plan.query(&[dimension], &current, Some(&order), limit, false)?;
        let mut rows = rows.get("rows").and_then(Value::as_array).cloned().unwrap_or_default();
        if order.starts_with('-') {
            rows.reverse();
        }
        for row in rows {
            let label = row.get(field).and_then(Value::as_str).unwrap_or("").to_string();
            trend.push(json!({ "label": label, "value": row.get(plan.headline()).and_then(number).map(round).unwrap_or(0.0) }));
        }
    }
    Ok(json!({
        "readings": readings,
        "value": readings.first().map(|r| r["value"].clone()),
        "target": plan.spec.target,
        "progress": progress,
        "status": status,
        "compared": previous.is_some(),
        "trend": trend,
        "meta": plan.meta(&current, None),
    }))
}

/// Whether a change is good news, bad news or neither: a rise is good unless lower is better.
fn tone(change: Option<f64>, lower_is_better: bool) -> &'static str {
    match change {
        Some(change) if change > 0.0 => if lower_is_better { "bad" } else { "good" },
        Some(change) if change < 0.0 => if lower_is_better { "good" } else { "bad" },
        _ => "flat",
    }
}

/// `(now - before) / before`; `None` without a previous figure or when it was zero.
fn change(now: Option<f64>, before: Option<f64>) -> Option<f64> {
    match (now, before) {
        (Some(now), Some(before)) if before != 0.0 => Some(round((now - before) / before.abs())),
        _ => None,
    }
}

fn round(value: f64) -> f64 {
    (value * 10_000.0).round() / 10_000.0
}

// ---- labels -----------------------------------------------------------------------------

/// Turns the raw value of a dimension into what a person reads.
pub(crate) struct Labels {
    maps: HashMap<String, HashMap<String, String>>,
    kinds: HashMap<String, String>,
}

impl Labels {
    pub(crate) fn build(source: &SourceInfo, dims: &[Value], rows: &[Value]) -> Self {
        let mut maps = HashMap::new();
        let mut kinds = HashMap::new();
        for dimension in dims {
            let name = dsl::dimension_field(dimension);
            let Some(field) = source.field(name) else { continue };
            kinds.insert(name.to_string(), if dimension.is_object() { "grain".to_string() } else { field.kind.clone() });
            match field.kind.as_str() {
                "select" if !dimension.is_object() => {
                    maps.insert(name.to_string(), field.options.iter().cloned().collect());
                }
                "link" => {
                    let ids: Vec<String> = rows
                        .iter()
                        .filter_map(|row| row.get(name).and_then(Value::as_str).map(str::to_string))
                        .collect::<std::collections::BTreeSet<_>>()
                        .into_iter()
                        .collect();
                    if let Some(model) = source.link_model(field) {
                        maps.insert(name.to_string(), source::titles(&model, &ids));
                    }
                }
                _ => {}
            }
        }
        Self { maps, kinds }
    }

    pub(crate) fn label(&self, field: &str, raw: &Value) -> String {
        match raw {
            Value::Null => "(none)".to_string(),
            Value::Bool(true) => "Yes".to_string(),
            Value::Bool(false) => "No".to_string(),
            Value::String(text) => match self.maps.get(field).and_then(|map| map.get(text)) {
                Some(label) => label.clone(),
                None if self.kinds.get(field).is_some_and(|kind| kind == "link") => text
                    .split_once(':')
                    .map(|(_, key)| key.to_string())
                    .unwrap_or_else(|| text.clone()),
                None if text.is_empty() => "(none)".to_string(),
                None => text.clone(),
            },
            other => other.to_string(),
        }
    }
}

/// A stable text for a raw dimension value, for grouping rows.
fn key_of(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

/// A report's total row with its figures as numbers (a decimal sum arrives as text).
fn totals_of(total: &Value) -> Value {
    match total.as_object() {
        Some(object) => Value::Object(
            object
                .iter()
                .map(|(name, value)| (name.clone(), number(value).map_or_else(|| value.clone(), |n| json!(round(n)))))
                .collect(),
        ),
        None => total.clone(),
    }
}

fn rows_of(answer: &Value) -> Vec<Value> {
    answer.get("rows").and_then(Value::as_array).cloned().unwrap_or_default()
}

// ---- bar, line, pie … ---------------------------------------------------------------------

fn run_series(plan: &Plan) -> Result<Value> {
    let category = plan.dims.first().ok_or_else(|| Error::msg("group this chart by a field, such as `stage`"))?;
    let split = plan.dims.get(1).filter(|_| SPLITTABLE.contains(&plan.kind.as_str()));
    let dims: Vec<Value> = std::iter::once(category.clone()).chain(split.cloned()).collect();
    let filter = plan.filter(false).unwrap_or_else(|| json!({}));
    let order = plan.default_order();
    let limit = if split.is_some() { 1000 } else { plan.spec.limit() };
    let answer = plan.query(&dims, &filter, order.as_deref(), limit, false)?;
    let rows = rows_of(&answer);
    let labels = Labels::build(&plan.source, &dims, &rows);
    let (cat_field, split_field) = (dsl::dimension_field(category), split.map(dsl::dimension_field));
    let measures: Vec<&str> = if split.is_some() || matches!(plan.kind.as_str(), "pie" | "donut" | "funnel" | "map") {
        vec![plan.headline()]
    } else {
        plan.parsed.visible().into_iter().take(MAX_SERIES).collect()
    };
    let by_date = category.is_object();
    let shaped = shape_series(&rows, cat_field, split_field, &measures, by_date, plan.spec.limit() as usize);
    let categories: Vec<Value> = shaped.keys.iter().map(|key| json!(labels.label(cat_field, key))).collect();
    let series: Vec<Value> = shaped
        .series
        .iter()
        .map(|series| {
            let name = match (&series.key, split_field) {
                (Some(key), Some(field)) => labels.label(field, key),
                _ => source::humanize(&series.measure),
            };
            json!({ "name": name, "key": series.key, "measure": series.measure, "values": series.values })
        })
        .collect();
    let total_groups = answer.get("total_groups").and_then(Value::as_i64);
    Ok(json!({
        "categories": categories,
        "keys": shaped.keys,
        "series": series,
        "dimensions": [cat_field, split_field],
        "total": answer.get("total"),
        "truncated": total_groups.is_some_and(|groups| groups > rows.len() as i64) && split.is_none(),
        "meta": plan.meta(&filter, total_groups),
    }))
}

struct Series {
    key: Option<Value>,
    measure: String,
    values: Vec<f64>,
}

struct Shaped {
    keys: Vec<Value>,
    series: Vec<Series>,
}

/// Lay rows out as categories × series. Without `split`, each measure is a series. With it, the
/// first measure is split by the second dimension. Categories keep the row order when they are
/// dates; otherwise they are ordered by size and cut at `cap`, and series are cut at eight.
fn shape_series(
    rows: &[Value],
    category: &str,
    split: Option<&str>,
    measures: &[&str],
    by_date: bool,
    cap: usize,
) -> Shaped {
    let raw = |row: &Value, field: &str| row.get(field).cloned().unwrap_or(Value::Null);
    let value = |row: &Value, measure: &str| row.get(measure).and_then(number).map(round).unwrap_or(0.0);
    let Some(split) = split else {
        let keys: Vec<Value> = rows.iter().map(|row| raw(row, category)).collect();
        let series = measures
            .iter()
            .map(|measure| Series {
                key: None,
                measure: measure.to_string(),
                values: rows.iter().map(|row| value(row, measure)).collect(),
            })
            .collect();
        return Shaped { keys, series };
    };
    let measure = measures.first().copied().unwrap_or("n");
    let mut category_totals: Vec<(String, Value, f64)> = Vec::new();
    let mut series_totals: Vec<(String, Value, f64)> = Vec::new();
    let bump = |totals: &mut Vec<(String, Value, f64)>, key: String, raw: Value, amount: f64| {
        match totals.iter_mut().find(|(existing, _, _)| *existing == key) {
            Some(entry) => entry.2 += amount,
            None => totals.push((key, raw, amount)),
        }
    };
    for row in rows {
        let amount = value(row, measure);
        bump(&mut category_totals, key_of(row.get(category)), raw(row, category), amount);
        bump(&mut series_totals, key_of(row.get(split)), raw(row, split), amount);
    }
    if !by_date {
        category_totals.sort_by(|a, b| b.2.total_cmp(&a.2));
        category_totals.truncate(cap);
    }
    series_totals.sort_by(|a, b| b.2.total_cmp(&a.2));
    series_totals.truncate(MAX_SERIES);
    let series = series_totals
        .iter()
        .map(|(series_key, series_raw, _)| Series {
            key: Some(series_raw.clone()),
            measure: measure.to_string(),
            values: category_totals
                .iter()
                .map(|(category_key, _, _)| {
                    rows.iter()
                        .filter(|row| key_of(row.get(category)) == *category_key && key_of(row.get(split)) == *series_key)
                        .map(|row| value(row, measure))
                        .sum()
                })
                .collect(),
        })
        .collect();
    Shaped { keys: category_totals.into_iter().map(|(_, raw, _)| raw).collect(), series }
}

// ---- bubble -----------------------------------------------------------------------------

/// One bubble per group: the first measure places it across, the second up, the optional third
/// sizes it.
fn run_bubble(plan: &Plan) -> Result<Value> {
    let category = plan.dims.first().ok_or_else(|| Error::msg("group this chart by a field: each group becomes one bubble"))?;
    let names = plan.parsed.visible();
    if names.len() < 2 {
        return Err(Error::msg("a bubble chart needs two measures, one for each axis; a third sets the size of the bubble"));
    }
    let field = dsl::dimension_field(category);
    let filter = plan.filter(false).unwrap_or_else(|| json!({}));
    let order = plan.default_order();
    let answer = plan.query(std::slice::from_ref(category), &filter, order.as_deref(), plan.spec.limit(), false)?;
    let rows = rows_of(&answer);
    let labels = Labels::build(&plan.source, std::slice::from_ref(category), &rows);
    let figure = |row: &Value, name: &str| row.get(name).and_then(number).map(round).unwrap_or(0.0);
    let points: Vec<Value> = rows
        .iter()
        .map(|row| {
            let raw = row.get(field).cloned().unwrap_or(Value::Null);
            json!({
                "label": labels.label(field, &raw),
                "key": raw,
                "x": figure(row, names[0]),
                "y": figure(row, names[1]),
                "size": names.get(2).map(|name| figure(row, name)),
            })
        })
        .collect();
    let total_groups = answer.get("total_groups").and_then(Value::as_i64);
    Ok(json!({
        "points": points,
        "axes": { "x": source::humanize(names[0]), "y": source::humanize(names[1]), "size": names.get(2).map(|name| source::humanize(name)) },
        "dimensions": [field, null],
        "truncated": total_groups.is_some_and(|groups| groups > rows.len() as i64),
        "meta": plan.meta(&filter, total_groups),
    }))
}

// ---- sankey -----------------------------------------------------------------------------

/// Flows between the values of consecutive group-by fields (two to four): the first measure is how
/// much flows. Each field is a column of nodes; a link joins a value to the value beside it.
fn run_sankey(plan: &Plan) -> Result<Value> {
    if plan.dims.len() < 2 {
        return Err(Error::msg("a flow chart groups by at least two fields, such as `source, stage`"));
    }
    let filter = plan.filter(false).unwrap_or_else(|| json!({}));
    let measure = plan.headline().to_string();
    let answer = plan.query(&plan.dims, &filter, Some(&format!("-{measure}")), 1000, false)?;
    let rows = rows_of(&answer);
    let labels = Labels::build(&plan.source, &plan.dims, &rows);
    let fields: Vec<&str> = plan.dims.iter().map(dsl::dimension_field).collect();
    let flow = SankeyFlow::build(&rows, &fields, &measure, plan.spec.limit() as usize);
    let nodes: Vec<Value> = flow
        .nodes
        .iter()
        .map(|node| {
            json!({
                "level": node.level,
                "field": fields[node.level],
                "key": node.raw,
                "name": labels.label(fields[node.level], &node.raw),
                "value": round(node.value),
            })
        })
        .collect();
    let links: Vec<Value> = flow
        .links
        .iter()
        .map(|link| json!({ "source": link.from, "target": link.to, "value": round(link.value) }))
        .collect();
    Ok(json!({
        "nodes": nodes,
        "links": links,
        "measure": measure,
        "dimensions": fields,
        "truncated": flow.truncated,
        "meta": plan.meta(&filter, answer.get("total_groups").and_then(Value::as_i64)),
    }))
}

struct FlowNode {
    level: usize,
    raw: Value,
    value: f64,
}

struct FlowLink {
    from: usize,
    to: usize,
    value: f64,
}

struct SankeyFlow {
    nodes: Vec<FlowNode>,
    links: Vec<FlowLink>,
    truncated: bool,
}

impl SankeyFlow {
    /// Add `rows` up into links between level `i` and `i + 1`, keep the `cap` largest links, and
    /// drop nodes left without one. A node's value is the larger of what flows in and out.
    fn build(rows: &[Value], fields: &[&str], measure: &str, cap: usize) -> Self {
        let mut nodes: Vec<FlowNode> = Vec::new();
        let mut index: HashMap<(usize, String), usize> = HashMap::new();
        let mut links: Vec<FlowLink> = Vec::new();
        let mut at = |level: usize, row: &Value, nodes: &mut Vec<FlowNode>| -> usize {
            let raw = row.get(fields[level]).cloned().unwrap_or(Value::Null);
            *index.entry((level, key_of(Some(&raw)))).or_insert_with(|| {
                nodes.push(FlowNode { level, raw, value: 0.0 });
                nodes.len() - 1
            })
        };
        for row in rows {
            let amount = row.get(measure).and_then(number).unwrap_or(0.0);
            if amount <= 0.0 {
                continue;
            }
            for level in 0..fields.len() - 1 {
                let (from, to) = (at(level, row, &mut nodes), at(level + 1, row, &mut nodes));
                match links.iter_mut().find(|link| link.from == from && link.to == to) {
                    Some(link) => link.value += amount,
                    None => links.push(FlowLink { from, to, value: amount }),
                }
            }
        }
        links.sort_by(|a, b| b.value.total_cmp(&a.value));
        let truncated = links.len() > cap;
        links.truncate(cap.max(1));
        let mut used: Vec<usize> = links.iter().flat_map(|link| [link.from, link.to]).collect();
        used.sort_unstable();
        used.dedup();
        let remap: HashMap<usize, usize> = used.iter().enumerate().map(|(new, old)| (*old, new)).collect();
        for link in &mut links {
            link.from = remap[&link.from];
            link.to = remap[&link.to];
        }
        let mut kept: Vec<FlowNode> = nodes.into_iter().enumerate().filter(|(old, _)| remap.contains_key(old)).map(|(_, node)| node).collect();
        for (position, node) in kept.iter_mut().enumerate() {
            let inflow: f64 = links.iter().filter(|link| link.to == position).map(|link| link.value).sum();
            let outflow: f64 = links.iter().filter(|link| link.from == position).map(|link| link.value).sum();
            node.value = inflow.max(outflow);
        }
        Self { nodes: kept, links, truncated }
    }
}

// ---- table ----------------------------------------------------------------------------------

fn run_table(plan: &Plan) -> Result<Value> {
    let filter = plan.filter(false).unwrap_or_else(|| json!({}));
    let order = plan.default_order();
    let answer = plan.query(&plan.dims, &filter, order.as_deref(), plan.spec.limit(), false)?;
    let rows = rows_of(&answer);
    let labels = Labels::build(&plan.source, &plan.dims, &rows);
    let mut columns = Vec::new();
    for dimension in &plan.dims {
        let field = dsl::dimension_field(dimension);
        let label = plan.source.field(field).map(|f| f.label.clone()).unwrap_or_else(|| source::humanize(field));
        columns.push(json!({ "key": field, "label": label, "type": "dimension" }));
    }
    for name in plan.parsed.visible() {
        columns.push(json!({ "key": name, "label": source::humanize(name), "type": "measure" }));
    }
    let shaped: Vec<Value> = rows
        .iter()
        .map(|row| {
            let mut cells = serde_json::Map::new();
            let mut keys = serde_json::Map::new();
            for dimension in &plan.dims {
                let field = dsl::dimension_field(dimension);
                let raw = row.get(field).cloned().unwrap_or(Value::Null);
                cells.insert(field.to_string(), json!(labels.label(field, &raw)));
                keys.insert(field.to_string(), raw);
            }
            for name in plan.parsed.visible() {
                cells.insert(name.to_string(), row.get(name).and_then(number).map(round).map_or(Value::Null, |v| json!(v)));
            }
            json!({ "cells": cells, "keys": keys })
        })
        .collect();
    let total_groups = answer.get("total_groups").and_then(Value::as_i64);
    Ok(json!({
        "columns": columns,
        "rows": shaped,
        "totals": answer.get("total").map(totals_of),
        "dimensions": plan.dims.iter().map(|d| dsl::dimension_field(d)).collect::<Vec<_>>(),
        "truncated": total_groups.is_some_and(|groups| groups > rows.len() as i64),
        "meta": plan.meta(&filter, total_groups),
    }))
}

// ---- pivot ----------------------------------------------------------------------------------

fn run_pivot(plan: &Plan) -> Result<Value> {
    let row_dim = plan.dims.first().ok_or_else(|| Error::msg("a pivot needs a row field, such as `stage`"))?;
    let col_dim = plan.dims.get(1);
    let dims: Vec<Value> = std::iter::once(row_dim.clone()).chain(col_dim.cloned()).collect();
    let filter = plan.filter(false).unwrap_or_else(|| json!({}));
    let answer = plan.query(&dims, &filter, None, 1000, col_dim.is_some())?;
    let rows = rows_of(&answer);
    let measure = plan.headline().to_string();
    let (row_field, col_field) = (dsl::dimension_field(row_dim), col_dim.map(dsl::dimension_field));

    let row_totals = match col_field {
        Some(_) => answer.pointer("/subtotals/1").and_then(Value::as_array).cloned().unwrap_or_default(),
        None => rows.clone(),
    };
    let col_totals = match col_dim {
        Some(col) => rows_of(&plan.query(std::slice::from_ref(col), &filter, None, 1000, false)?),
        None => Vec::new(),
    };
    let amount = |row: &Value| row.get(&measure).and_then(number).map(round).unwrap_or(0.0);
    let order = |totals: &[Value], field: &str, by_date: bool| -> Vec<Value> {
        let mut keyed: Vec<(Value, f64)> = totals.iter().map(|row| (row.get(field).cloned().unwrap_or(Value::Null), amount(row))).collect();
        if by_date {
            keyed.sort_by_key(|(raw, _)| key_of(Some(raw)));
        } else {
            keyed.sort_by(|a, b| b.1.total_cmp(&a.1));
        }
        keyed.into_iter().map(|(raw, _)| raw).collect()
    };
    let row_keys = order(&row_totals, row_field, row_dim.is_object());
    let col_keys = match col_field {
        Some(field) => order(&col_totals, field, col_dim.is_some_and(Value::is_object)),
        None => vec![Value::Null],
    };
    let matrix: Vec<Vec<f64>> = row_keys
        .iter()
        .map(|row_key| {
            col_keys
                .iter()
                .map(|col_key| {
                    rows.iter()
                        .find(|row| {
                            key_of(row.get(row_field)) == key_of(Some(row_key))
                                && col_field.is_none_or(|field| key_of(row.get(field)) == key_of(Some(col_key)))
                        })
                        .map(amount)
                        .unwrap_or(0.0)
                })
                .collect()
        })
        .collect();
    let labels = Labels::build(&plan.source, &dims, &rows);
    let row_labels: Vec<String> = row_keys.iter().map(|key| labels.label(row_field, key)).collect();
    let col_labels: Vec<String> = match col_field {
        Some(field) => col_keys.iter().map(|key| labels.label(field, key)).collect(),
        None => vec![source::humanize(&measure)],
    };
    let totals_for = |keys: &[Value], totals: &[Value], field: &str| -> Vec<f64> {
        keys.iter()
            .map(|key| totals.iter().find(|row| key_of(row.get(field)) == key_of(Some(key))).map(amount).unwrap_or(0.0))
            .collect()
    };
    let row_sums = totals_for(&row_keys, &row_totals, row_field);
    let col_sums = col_field.map(|field| totals_for(&col_keys, &col_totals, field)).unwrap_or_default();
    Ok(json!({
        "rows": row_labels,
        "row_keys": row_keys,
        "columns": col_labels,
        "column_keys": col_keys,
        "matrix": matrix,
        "row_totals": row_sums,
        "column_totals": col_sums,
        "grand": answer.pointer(&format!("/total/{measure}")).and_then(number).map(round),
        "measure": measure,
        "dimensions": [row_field, col_field],
        "meta": plan.meta(&filter, answer.get("total_groups").and_then(Value::as_i64)),
    }))
}

// ---- drill-down -------------------------------------------------------------------------

/// The records behind one bar, slice or cell: `segment` maps a dimension field to the raw value
/// the chart gave for it. Date groups become the date range they stand for.
pub fn drill(spec: &ChartSpec, controls: &[Control], segment: &serde_json::Map<String, Value>, limit: u32, offset: u32) -> Result<Value> {
    let plan = plan(spec, controls)?;
    let mut parts = vec![plan.filter(false).unwrap_or_else(|| json!({}))];
    for (field, raw) in segment {
        let Some(info) = plan.source.field(field) else {
            return Err(unknown_field(&plan.source, field));
        };
        let grain = plan.dims.iter().find(|d| dsl::dimension_field(d) == field).and_then(|d| d.get("grain")).and_then(Value::as_str);
        parts.push(match (raw, grain) {
            (Value::Null, _) => json!({ field: { "null": true } }),
            (Value::String(label), Some(grain)) => match periods::grain_range(grain, label) {
                Some((from, to)) => json!({ field: { "gte": periods::bound(&info.kind, from), "lt": periods::bound(&info.kind, to) } }),
                None => return Err(Error::msg(format!("`{label}` is not a {grain}"))),
            },
            (value, _) => json!({ field: value }),
        });
    }
    let filter = controls::all_of(parts);
    let total = db::count(&plan.source.name, db::Filter::from_json(filter.clone()))?;
    let columns = drill_columns(&plan);
    let records = db::find::<Record>(&plan.source.name)
        .matching(db::Filter::from_json(filter.clone()))
        .limit(limit.clamp(1, 200))
        .offset(offset)
        .all()?;
    let mut maps: HashMap<String, HashMap<String, String>> = HashMap::new();
    for name in &columns {
        let Some(field) = plan.source.field(name) else { continue };
        match field.kind.as_str() {
            "link" => {
                let ids: Vec<String> = records.iter().filter_map(|r| r.get(name).and_then(Value::as_str).map(str::to_string)).collect();
                if let Some(model) = plan.source.link_model(field) {
                    maps.insert(name.clone(), source::titles(&model, &ids));
                }
            }
            "select" => {
                maps.insert(name.clone(), field.options.iter().cloned().collect());
            }
            _ => {}
        }
    }
    let rows: Vec<Value> = records
        .iter()
        .map(|record| {
            let cells: serde_json::Map<String, Value> = columns
                .iter()
                .map(|name| {
                    let raw = record.get(name).cloned().unwrap_or(Value::Null);
                    let shown = match (&raw, maps.get(name)) {
                        (Value::String(text), Some(map)) => json!(map.get(text).cloned().unwrap_or_else(|| text.clone())),
                        _ => raw,
                    };
                    (name.clone(), shown)
                })
                .collect();
            json!({ "id": record.get("id"), "cells": cells })
        })
        .collect();
    let labels: Vec<Value> = columns
        .iter()
        .map(|name| {
            let label = plan.source.field(name).map(|f| f.label.clone()).unwrap_or_else(|| source::humanize(name));
            json!({ "key": name, "label": label })
        })
        .collect();
    Ok(json!({ "columns": labels, "rows": rows, "total": total, "limit": limit, "offset": offset, "meta": plan.meta(&filter, None) }))
}

/// The columns a drill shows: what the chart groups and adds up first, then the record's own
/// plain fields, at most ten.
fn drill_columns(plan: &Plan) -> Vec<String> {
    let mut wanted: Vec<String> = Vec::new();
    let mut add = |name: &str| {
        if plan.source.field(name).is_some() && !wanted.iter().any(|w| w == name) {
            wanted.push(name.to_string());
        }
    };
    for name in ["title", "name", "number", "code"] {
        add(name);
    }
    for dimension in &plan.dims {
        add(dsl::dimension_field(dimension));
    }
    for measure in plan.parsed.measures.values() {
        match measure {
            Value::Object(object) => {
                if let Some((_, body)) = object.iter().next() {
                    match body {
                        Value::String(field) => add(field),
                        Value::Object(inner) => {
                            if let Some(field) = inner.get("field").and_then(Value::as_str) {
                                add(field);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    for field in &plan.source.fields {
        if matches!(field.kind.as_str(), "text" | "json") {
            continue;
        }
        add(&field.name);
    }
    wanted.truncate(10);
    wanted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fall_is_good_news_when_lower_is_better() {
        assert_eq!(tone(Some(0.2), false), "good");
        assert_eq!(tone(Some(0.2), true), "bad");
        assert_eq!(tone(Some(-0.2), true), "good");
        assert_eq!(tone(None, true), "flat");
        assert_eq!(tone(Some(0.0), false), "flat");
    }

    #[test]
    fn change_is_relative_to_the_previous_figure() {
        assert_eq!(change(Some(150.0), Some(100.0)), Some(0.5));
        assert_eq!(change(Some(50.0), Some(100.0)), Some(-0.5));
        assert_eq!(change(Some(5.0), Some(0.0)), None);
        assert_eq!(change(Some(5.0), None), None);
        assert_eq!(change(None, Some(5.0)), None);
    }

    fn rows() -> Vec<Value> {
        vec![
            json!({ "stage": "s1", "owner": "ann", "v": 10.0 }),
            json!({ "stage": "s1", "owner": "bob", "v": 5.0 }),
            json!({ "stage": "s2", "owner": "ann", "v": 40.0 }),
            json!({ "stage": "s3", "owner": "cat", "v": "2.50" }),
        ]
    }

    #[test]
    fn without_a_split_each_measure_is_a_series() {
        let rows = vec![json!({ "stage": "a", "n": 3, "v": "9.5" }), json!({ "stage": "b", "n": 1, "v": null })];
        let shaped = shape_series(&rows, "stage", None, &["n", "v"], false, 12);
        assert_eq!(shaped.keys, vec![json!("a"), json!("b")]);
        assert_eq!(shaped.series[0].values, vec![3.0, 1.0]);
        assert_eq!(shaped.series[1].values, vec![9.5, 0.0]);
    }

    #[test]
    fn a_split_makes_a_series_per_value_ordered_by_size() {
        let shaped = shape_series(&rows(), "stage", Some("owner"), &["v"], false, 12);
        // Categories by total: s2 (40), s1 (15), s3 (2.5).
        assert_eq!(shaped.keys, vec![json!("s2"), json!("s1"), json!("s3")]);
        // Series by total: ann (50), bob (5), cat (2.5).
        let names: Vec<_> = shaped.series.iter().map(|s| s.key.clone().unwrap()).collect();
        assert_eq!(names, vec![json!("ann"), json!("bob"), json!("cat")]);
        assert_eq!(shaped.series[0].values, vec![40.0, 10.0, 0.0]);
        assert_eq!(shaped.series[1].values, vec![0.0, 5.0, 0.0]);
    }

    #[test]
    fn date_categories_keep_the_order_they_came_in() {
        let rows = vec![
            json!({ "month": "2026-01", "owner": "x", "v": 1 }),
            json!({ "month": "2026-02", "owner": "x", "v": 9 }),
            json!({ "month": "2026-03", "owner": "x", "v": 4 }),
        ];
        let shaped = shape_series(&rows, "month", Some("owner"), &["v"], true, 2);
        assert_eq!(shaped.keys.len(), 3);
        assert_eq!(shaped.series[0].values, vec![1.0, 9.0, 4.0]);
    }

    #[test]
    fn other_categories_are_cut_at_the_limit() {
        let shaped = shape_series(&rows(), "stage", Some("owner"), &["v"], false, 2);
        assert_eq!(shaped.keys, vec![json!("s2"), json!("s1")]);
    }

    #[test]
    fn flows_add_up_between_neighbouring_levels() {
        let rows = vec![
            json!({ "src": "web", "stage": "won", "n": 3 }),
            json!({ "src": "web", "stage": "lost", "n": 1 }),
            json!({ "src": "ads", "stage": "won", "n": 2 }),
            json!({ "src": "ads", "stage": "won", "n": 4 }),
            json!({ "src": "ads", "stage": "open", "n": 0 }),
        ];
        let flow = SankeyFlow::build(&rows, &["src", "stage"], "n", 10);
        assert_eq!(flow.nodes.len(), 4);
        assert_eq!(flow.links.len(), 3);
        let won = flow.nodes.iter().position(|n| n.raw == json!("won")).unwrap();
        assert_eq!(flow.nodes[won].value, 9.0);
        assert_eq!(flow.links[0].value, 6.0);
        assert!(!flow.truncated);
    }

    #[test]
    fn the_smallest_flows_are_cut_and_their_nodes_dropped() {
        let rows = vec![
            json!({ "a": "x", "b": "p", "n": 5 }),
            json!({ "a": "y", "b": "q", "n": 1 }),
        ];
        let flow = SankeyFlow::build(&rows, &["a", "b"], "n", 1);
        assert!(flow.truncated);
        assert_eq!(flow.nodes.len(), 2);
        assert_eq!(flow.links.len(), 1);
        assert_eq!((flow.links[0].from, flow.links[0].to), (0, 1));
    }

    #[test]
    fn three_levels_chain() {
        let rows = vec![json!({ "a": "x", "b": "p", "c": "m", "n": 2 })];
        let flow = SankeyFlow::build(&rows, &["a", "b", "c"], "n", 10);
        assert_eq!(flow.nodes.len(), 3);
        assert_eq!(flow.links.len(), 2);
    }

    #[test]
    fn labels_fall_back_to_the_key_of_an_unreadable_link() {
        let labels = Labels {
            maps: HashMap::from([("stage".to_string(), HashMap::from([("crm_stage:a".to_string(), "Won".to_string())]))]),
            kinds: HashMap::from([("stage".to_string(), "link".to_string())]),
        };
        assert_eq!(labels.label("stage", &json!("crm_stage:a")), "Won");
        assert_eq!(labels.label("stage", &json!("crm_stage:zz")), "zz");
        assert_eq!(labels.label("stage", &Value::Null), "(none)");
        assert_eq!(labels.label("active", &json!(true)), "Yes");
    }
}
