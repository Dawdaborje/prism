//! Dashboard filters, routed into a chart's own filter.
//!
//! A control names a field and a condition. It applies to a chart only when the chart's source
//! has that field; the field `$date` means the chart's own `date_field`, so one date filter
//! steers every chart whatever it calls its date. A control that does not fit a chart is
//! reported as skipped, never silently dropped and never an error.

use chrono::{Duration, NaiveDate};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::periods::bound;
use crate::source::SourceInfo;

pub const DATE: &str = "$date";

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct Control {
    pub field: String,
    /// Equals this value.
    #[serde(default)]
    pub eq: Option<Value>,
    /// One of these values.
    #[serde(default, rename = "in")]
    pub any: Option<Vec<Value>>,
    /// Contains this text.
    #[serde(default)]
    pub like: Option<String>,
    /// A named period (`this_month`, `last_90_days`, …).
    #[serde(default)]
    pub period: Option<String>,
    /// From this date or number, inclusive.
    #[serde(default)]
    pub from: Option<Value>,
    /// To this date or number, inclusive.
    #[serde(default)]
    pub to: Option<Value>,
}

impl Control {
    /// Whether the control asks for nothing, which is what an unset dashboard filter sends.
    pub fn is_empty(&self) -> bool {
        self.eq.as_ref().is_none_or(is_blank)
            && self.any.as_ref().is_none_or(|list| list.is_empty())
            && self.like.as_deref().is_none_or(|text| text.trim().is_empty())
            && self.period.as_deref().is_none_or(|text| text.is_empty() || text == "none")
            && self.from.as_ref().is_none_or(is_blank)
            && self.to.as_ref().is_none_or(is_blank)
    }
}

fn is_blank(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::String(text) => text.trim().is_empty(),
        _ => false,
    }
}

/// The filter a control adds to a chart, or why it does not apply.
pub fn to_filter(control: &Control, source: &SourceInfo, date_field: Option<&str>) -> Result<Value, String> {
    let field = if control.field == DATE {
        date_field.ok_or_else(|| "the chart has no date field".to_string())?
    } else {
        control.field.as_str()
    };
    let info = source
        .field(field)
        .ok_or_else(|| format!("`{field}` is not a field of {}", source.name))?;
    let mut ops = Map::new();
    if let Some(period) = control.period.as_deref().filter(|p| !p.is_empty() && *p != "none") {
        if info.kind != "date" && info.kind != "datetime" {
            return Err(format!("`{field}` is not a date"));
        }
        ops.insert("in_period".into(), json!(period));
    }
    if let Some(value) = control.eq.as_ref().filter(|v| !is_blank(v)) {
        return Ok(json!({ field: value }));
    }
    if let Some(list) = control.any.as_ref().filter(|list| !list.is_empty()) {
        ops.insert("in".into(), Value::Array(list.clone()));
    }
    if let Some(text) = control.like.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
        ops.insert("like".into(), json!(text));
    }
    if let Some(from) = control.from.as_ref().filter(|v| !is_blank(v)) {
        ops.insert("gte".into(), json!(from));
    }
    if let Some(to) = control.to.as_ref().filter(|v| !is_blank(v)) {
        match (info.kind.as_str(), to.as_str()) {
            // A date typed as the last day wanted covers that whole day.
            ("datetime", Some(text)) => match NaiveDate::parse_from_str(text, "%Y-%m-%d") {
                Ok(day) => ops.insert("lt".into(), json!(bound("datetime", day + Duration::days(1)))),
                Err(_) => ops.insert("lte".into(), json!(to)),
            },
            _ => ops.insert("lte".into(), json!(to)),
        };
    }
    if ops.is_empty() {
        return Err("the control asks for nothing".to_string());
    }
    Ok(json!({ field: Value::Object(ops) }))
}

/// All of `parts` that are not empty, as one filter.
pub fn all_of(parts: Vec<Value>) -> Value {
    let mut parts: Vec<Value> = parts
        .into_iter()
        .filter(|part| part.as_object().is_some_and(|object| !object.is_empty()))
        .collect();
    match parts.len() {
        0 => json!({}),
        1 => parts.remove(0),
        _ => json!({ "and": parts }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> SourceInfo {
        SourceInfo::parse(&json!({
            "name": "crm_deal.deal",
            "fields": [
                { "name": "stage", "type": "link", "target": "crm.crm_stage" },
                { "name": "created", "type": "datetime" },
                { "name": "expected_close", "type": "date" },
                { "name": "title", "type": "string" }
            ]
        }))
        .unwrap()
    }

    fn control(value: Value) -> Control {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_value_list_becomes_in() {
        let filter = to_filter(&control(json!({ "field": "stage", "in": ["a", "b"] })), &source(), None).unwrap();
        assert_eq!(filter, json!({ "stage": { "in": ["a", "b"] } }));
    }

    #[test]
    fn the_date_field_stands_for_each_charts_own() {
        let c = control(json!({ "field": "$date", "period": "this_month" }));
        assert_eq!(
            to_filter(&c, &source(), Some("expected_close")).unwrap(),
            json!({ "expected_close": { "in_period": "this_month" } })
        );
        assert!(to_filter(&c, &source(), None).is_err());
    }

    #[test]
    fn the_last_day_of_a_datetime_range_covers_the_whole_day() {
        let c = control(json!({ "field": "created", "from": "2026-03-01", "to": "2026-03-31" }));
        assert_eq!(
            to_filter(&c, &source(), None).unwrap(),
            json!({ "created": { "gte": "2026-03-01", "lt": "2026-04-01T00:00:00Z" } })
        );
        let d = control(json!({ "field": "expected_close", "to": "2026-03-31" }));
        assert_eq!(to_filter(&d, &source(), None).unwrap(), json!({ "expected_close": { "lte": "2026-03-31" } }));
    }

    #[test]
    fn a_control_for_a_missing_field_is_reported() {
        let error = to_filter(&control(json!({ "field": "owner", "eq": "x" })), &source(), None).unwrap_err();
        assert!(error.contains("owner"));
        assert!(to_filter(&control(json!({ "field": "title", "period": "this_month" })), &source(), None).is_err());
    }

    #[test]
    fn unset_controls_ask_for_nothing() {
        assert!(control(json!({ "field": "stage", "eq": "", "in": [] })).is_empty());
        assert!(control(json!({ "field": "$date", "period": "none" })).is_empty());
        assert!(!control(json!({ "field": "stage", "eq": "x" })).is_empty());
    }

    #[test]
    fn filters_combine_without_empty_parts() {
        assert_eq!(all_of(vec![json!({}), json!({ "a": 1 })]), json!({ "a": 1 }));
        assert_eq!(all_of(vec![json!({ "a": 1 }), json!({ "b": 2 })]), json!({ "and": [{ "a": 1 }, { "b": 2 }] }));
        assert_eq!(all_of(vec![]), json!({}));
    }
}
