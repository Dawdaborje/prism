//! What a chart may read: a model, its fields as the person may see them, and the readable
//! names of the records its links point at.

use std::collections::HashMap;

use aether_sdk::prelude::*;

use crate::common::Record;

#[derive(Debug, Clone, PartialEq)]
pub struct FieldInfo {
    pub name: String,
    pub label: String,
    /// `string`, `text`, `int`, `float`, `decimal`, `bool`, `date`, `datetime`, `select`, `link`.
    pub kind: String,
    pub target: Option<String>,
    pub options: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceInfo {
    /// How the plugin names it: `crm_deal.deal`, or a bare name for Prism's own models.
    pub name: String,
    pub fields: Vec<FieldInfo>,
}

impl SourceInfo {
    pub fn parse(source: &Value) -> Option<Self> {
        let name = source.get("name")?.as_str()?.to_string();
        let fields = source
            .get("fields")
            .and_then(Value::as_array)
            .map(|list| list.iter().filter_map(parse_field).collect())
            .unwrap_or_default();
        Some(Self { name, fields })
    }

    pub fn field(&self, name: &str) -> Option<&FieldInfo> {
        self.fields.iter().find(|field| field.name == name)
    }

    /// The plugin part of `plugin.model`, when there is one.
    pub fn plugin(&self) -> Option<&str> {
        self.name.split_once('.').map(|(plugin, _)| plugin)
    }

    /// The model a link field points at, named as the plugin must name it to read it.
    pub fn link_model(&self, field: &FieldInfo) -> Option<String> {
        let target = field.target.as_deref()?;
        if target.contains('.') {
            return Some(target.to_string());
        }
        Some(match self.plugin() {
            Some(plugin) => format!("{plugin}.{target}"),
            None => target.to_string(),
        })
    }

    pub fn is_temporal(&self, name: &str) -> bool {
        self.field(name).is_some_and(|field| field.kind == "date" || field.kind == "datetime")
    }
}

fn parse_field(field: &Value) -> Option<FieldInfo> {
    let name = field.get("name")?.as_str()?.to_string();
    let label = field
        .get("label")
        .and_then(Value::as_str)
        // `%owner%` is a translation key the page layer fills in; here it would only be noise.
        .filter(|label| !(label.starts_with('%') && label.ends_with('%')))
        .map(|label| label.to_string())
        .unwrap_or_else(|| humanize(&name));
    let options = field
        .get("options")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(|option| {
                    let value = option.get("value")?.as_str()?.to_string();
                    let label = option.get("label").and_then(Value::as_str).unwrap_or(&value).to_string();
                    Some((value, label))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(FieldInfo {
        name,
        label,
        kind: field.get("type").and_then(Value::as_str).unwrap_or("string").to_string(),
        target: field.get("target").and_then(Value::as_str).map(str::to_string),
        options,
    })
}

/// `expected_close` as "Expected close".
pub fn humanize(name: &str) -> String {
    let text = name.replace('_', " ");
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The source `model`, or an error saying the person cannot report on it.
pub fn load(model: &str) -> Result<SourceInfo> {
    let answer = report::sources(&json!({ "model": model }))?;
    answer
        .get("sources")
        .and_then(Value::as_array)
        .and_then(|list| list.first())
        .and_then(SourceInfo::parse)
        .ok_or_else(|| Error::msg(format!("`{model}` is not a source you can report on")))
}

/// Every source the person may report on, names only.
pub fn list_names() -> Result<Vec<Value>> {
    let answer = report::sources(&json!({ "fields": false }))?;
    Ok(answer
        .get("sources")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|source| {
            // Prism's own models are not data to analyse.
            source.get("plugin").and_then(Value::as_str) != Some("prism")
                && !source
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.starts_with("prism_"))
        })
        .collect())
}

/// The name of a record as a person reads it: its title, name, number or code.
pub fn title_of(record: &Record) -> Option<String> {
    ["title", "name", "display_name", "label", "number", "code", "subject"]
        .iter()
        .find_map(|field| record.get(*field).and_then(Value::as_str).filter(|text| !text.is_empty()))
        .map(str::to_string)
}

/// Titles of the records `ids` point at in `model`, by id. A record the person cannot read keeps
/// no entry, so its id is shown as it is.
pub fn titles(model: &str, ids: &[String]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if ids.is_empty() {
        return out;
    }
    let found = db::find::<Record>(model)
        .matching(db::Filter::one_of("id", ids.iter().take(500)))
        .limit(500)
        .all()
        .unwrap_or_default();
    for record in found {
        if let (Some(id), Some(title)) = (record.get("id").and_then(Value::as_str), title_of(&record)) {
            out.insert(id.to_string(), title);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deal() -> SourceInfo {
        SourceInfo::parse(&json!({
            "name": "crm_deal.deal",
            "fields": [
                { "name": "title", "type": "string" },
                { "name": "stage", "type": "link", "target": "crm.crm_stage" },
                { "name": "team", "type": "link", "target": "deal_team" },
                { "name": "amount", "type": "decimal", "scale": 2 },
                { "name": "state", "type": "select", "options": [{ "value": "open", "label": "Open" }, { "value": "won" }] },
                { "name": "expected_close", "type": "date" }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn links_name_the_model_to_read() {
        let source = deal();
        assert_eq!(source.link_model(source.field("stage").unwrap()).as_deref(), Some("crm.crm_stage"));
        assert_eq!(source.link_model(source.field("team").unwrap()).as_deref(), Some("crm_deal.deal_team"));
        assert_eq!(source.link_model(source.field("title").unwrap()), None);
    }

    #[test]
    fn a_translation_key_is_not_a_label() {
        let source = SourceInfo::parse(&json!({ "name": "x.y", "fields": [{ "name": "expected_close", "label": "%expected_close%", "type": "date" }] })).unwrap();
        assert_eq!(source.field("expected_close").unwrap().label, "Expected close");
    }

    #[test]
    fn field_kinds_are_known() {
        let source = deal();
        assert!(source.is_temporal("expected_close"));
        assert!(!source.is_temporal("amount"));
        assert_eq!(source.field("state").unwrap().options[1], ("won".to_string(), "won".to_string()));
        assert_eq!(source.field("expected_close").unwrap().label, "Expected close");
    }

    #[test]
    fn a_record_is_named_by_the_first_field_it_has() {
        assert_eq!(title_of(&json!({ "id": "x", "code": "C1", "name": "Acme" })).as_deref(), Some("Acme"));
        assert_eq!(title_of(&json!({ "id": "x", "code": "C1" })).as_deref(), Some("C1"));
        assert_eq!(title_of(&json!({ "id": "x" })), None);
    }
}
