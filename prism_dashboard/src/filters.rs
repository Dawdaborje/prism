//! A dashboard's filter bar: what to show, and what each value means for the charts.

use aether_sdk::prelude::*;
use serde_json::Map;

use crate::common::{text, Record};

/// One filter as stored, with the value it has right now.
pub struct Active {
    pub name: String,
    pub label: String,
    pub kind: String,
    pub field: String,
    pub source: Option<String>,
    pub value: Value,
}

/// The value a filter has: what the bar sent under its name, else its default. An empty value the
/// bar sent on purpose (the person cleared the filter) is kept as empty, not replaced by the default.
pub fn value_of(filter: &Record, sent: &Map<String, Value>) -> Value {
    let name = text(filter, "name").unwrap_or("");
    if let Some(value) = sent.get(name) {
        return value.clone();
    }
    match text(filter, "default_value") {
        Some(default) if text(filter, "kind") == Some("select") => json!([default]),
        Some(default) => json!(default),
        None => Value::Null,
    }
}

pub fn active(filters: &[Record], sent: &Map<String, Value>) -> Vec<Active> {
    filters
        .iter()
        .map(|filter| Active {
            name: text(filter, "name").unwrap_or("").to_string(),
            label: text(filter, "label").unwrap_or("").to_string(),
            kind: text(filter, "kind").unwrap_or("period").to_string(),
            field: text(filter, "field").unwrap_or("$date").to_string(),
            source: text(filter, "source").map(str::to_string),
            value: value_of(filter, sent),
        })
        .collect()
}

/// The control a filter's value asks of every chart, or nothing when the filter is unset.
pub fn control_of(filter: &Active) -> Option<Value> {
    let field = &filter.field;
    match (filter.kind.as_str(), &filter.value) {
        ("period", Value::String(period)) if !period.is_empty() && period != "none" => {
            Some(json!({ "field": field, "period": period }))
        }
        ("period", Value::Object(range)) => {
            let from = range.get("from").filter(|v| v.as_str().is_some_and(|s| !s.is_empty()));
            let to = range.get("to").filter(|v| v.as_str().is_some_and(|s| !s.is_empty()));
            (from.is_some() || to.is_some()).then(|| json!({ "field": field, "from": from, "to": to }))
        }
        ("select", Value::Array(values)) if !values.is_empty() => Some(json!({ "field": field, "in": values })),
        ("select", Value::String(one)) if !one.is_empty() => Some(json!({ "field": field, "in": [one] })),
        ("text", Value::String(needle)) if !needle.trim().is_empty() => Some(json!({ "field": field, "like": needle.trim() })),
        _ => None,
    }
}

pub fn controls(filters: &[Active]) -> Vec<Value> {
    filters.iter().filter_map(control_of).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(value: Value) -> Record {
        value
    }

    fn sent(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn the_default_applies_until_the_bar_sends_something() {
        let f = filter(json!({ "name": "date", "kind": "period", "default_value": "this_month" }));
        assert_eq!(value_of(&f, &sent(json!({}))), json!("this_month"));
        assert_eq!(value_of(&f, &sent(json!({ "date": "last_month" }))), json!("last_month"));
        // Cleared on purpose: stays cleared.
        assert_eq!(value_of(&f, &sent(json!({ "date": "" }))), json!(""));
    }

    #[test]
    fn a_default_for_a_picker_is_a_list() {
        let f = filter(json!({ "name": "owner", "kind": "select", "default_value": "ann" }));
        assert_eq!(value_of(&f, &sent(json!({}))), json!(["ann"]));
    }

    fn active_of(kind: &str, field: &str, value: Value) -> Active {
        Active { name: "x".into(), label: "X".into(), kind: kind.into(), field: field.into(), source: None, value }
    }

    #[test]
    fn values_become_controls() {
        assert_eq!(
            control_of(&active_of("period", "$date", json!("this_month"))),
            Some(json!({ "field": "$date", "period": "this_month" }))
        );
        assert_eq!(
            control_of(&active_of("period", "$date", json!({ "from": "2026-01-01", "to": "" }))),
            Some(json!({ "field": "$date", "from": "2026-01-01", "to": null }))
        );
        assert_eq!(
            control_of(&active_of("select", "stage", json!(["a", "b"]))),
            Some(json!({ "field": "stage", "in": ["a", "b"] }))
        );
        assert_eq!(
            control_of(&active_of("text", "title", json!("  acme "))),
            Some(json!({ "field": "title", "like": "acme" }))
        );
    }

    #[test]
    fn unset_filters_ask_for_nothing() {
        for (kind, value) in [("period", json!("")), ("period", json!("none")), ("select", json!([])), ("text", json!("  ")), ("text", Value::Null)] {
            assert_eq!(control_of(&active_of(kind, "f", value)), None);
        }
    }
}
