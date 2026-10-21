//! Making, changing and finding alerts.
//!
//! An alert is checked as the person who made it. The person is never read from the alert itself: a
//! chain of background jobs starts from their own save and each job queues the next one as the same
//! account, so editing a stored `owner` can never make a check run as someone else.

use aether_sdk::prelude::*;
use aether_sdk::records::pick;

use crate::common::{id_of, me, require, require_analyst, require_viewer, required, text, Record};
use crate::rules;

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

const FIELDS: &[&str] = &[
    "name", "chart", "reading", "condition", "threshold", "frequency", "active", "emails", "message", "notify_clear",
];

/// Everything that can be checked before the alert is saved. The chart is fetched as the person, so a chart
/// they cannot open cannot be watched.
fn check(input: &Value) -> Result<()> {
    if let Some(condition) = text(input, "condition") {
        if !rules::CONDITIONS.contains(&condition) {
            return Err(Error::msg(format!("`{condition}` is not a condition")));
        }
    }
    if let Some(frequency) = text(input, "frequency") {
        if rules::interval_secs(frequency).is_none() {
            return Err(Error::msg("check every hour, day or week"));
        }
    }
    if let Some(emails) = text(input, "emails") {
        rules::addresses(emails).map_err(Error::msg)?;
    }
    if let Some(chart) = text(input, "chart") {
        let chart: Value = plugins::call("prism", "get_chart", &json!({ "id": chart }))?;
        if text(&chart, "kind") != Some("number") {
            return Err(Error::msg("an alert watches a number chart; make one with the type Number"));
        }
    }
    Ok(())
}

pub fn create_alert(input: Value) -> Result<Record> {
    require_analyst()?;
    required(&input, "name", "the name")?;
    required(&input, "chart", "the chart")?;
    check(&input)?;
    let condition = text(&input, "condition").unwrap_or("below");
    if condition != "is_empty" && input.get("threshold").is_none_or_null() {
        return Err(Error::msg("give the limit to compare with"));
    }
    let mut data = pick(&input, FIELDS);
    data["owner"] = json!(me()?);
    data["state"] = json!("new");
    let alert = db::create::<Record>("prism_alert", &data)?;
    seed(&alert)
}

pub fn set_alert(input: Value) -> Result<Record> {
    require_analyst()?;
    let id = required(&input, "id", "the alert")?;
    let current = require("prism_alert", &id, "alert")?;
    if text(&current, "owner") != Some(me()?.as_str()) {
        return Err(Error::msg("only the person who made this alert can change it"));
    }
    check(&input)?;
    let saved = db::update::<Record>("prism_alert", &id, &pick(&input, FIELDS))?
        .ok_or_else(|| Error::msg("you cannot change that alert"))?;
    seed(&saved)
}

pub fn get_alert(input: Id) -> Result<Record> {
    require_viewer()?;
    require("prism_alert", &input.id, "alert")
}

pub fn search_alerts(input: Search) -> Result<Value> {
    require_viewer()?;
    let filter = match input.filter {
        Some(value @ Value::Object(_)) => db::Filter::from_json(value),
        _ => db::Filter::all(),
    };
    db::search(
        "prism_alert",
        input.q.as_deref(),
        &["name"],
        filter,
        input.order.as_deref().or(Some("name")),
        Some(input.limit.unwrap_or(50).min(200)),
        input.offset,
    )
}

pub fn delete_alert(input: Id) -> Result<Value> {
    require_analyst()?;
    require("prism_alert", &input.id, "alert")?;
    db::delete::<Record>("prism_alert", &input.id)?;
    Ok(json!({ "deleted": 1 }))
}

/// Start (or stop) the chain of checks for an alert saved by its owner. A new chain makes every job of the
/// old one stop, so editing an alert never leaves two checking it.
fn seed(alert: &Record) -> Result<Record> {
    let id = id_of(alert)?.to_string();
    let active = alert.get("active").and_then(Value::as_bool).unwrap_or(true);
    let chain = if active { format!("{}-{}", context::current()?.now, id) } else { String::new() };
    let saved = db::update::<Record>("prism_alert", &id, &json!({ "chain": chain }))?.unwrap_or_else(|| alert.clone());
    if active {
        crate::run::queue(&id, &chain, 2)?;
    }
    Ok(saved)
}

trait NullOrMissing {
    fn is_none_or_null(&self) -> bool;
}

impl NullOrMissing for Option<&Value> {
    fn is_none_or_null(&self) -> bool {
        self.map_or(true, Value::is_null)
    }
}
