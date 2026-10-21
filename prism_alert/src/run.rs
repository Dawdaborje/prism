//! Checking an alert, and telling its owner.
//!
//! `run_alert` is a job that runs **as the owner** (`scheduler::enqueue` with `run_as`), so the chart is read
//! with their roles and record rules. It queues the next check first, so a failed check never ends the chain;
//! the chain ends when the alert is turned off, deleted, changed (a new chain replaces it) or when its owner
//! can no longer read the chart.

use aether_sdk::notify::{Audience, Level, Notification};
use aether_sdk::prelude::*;

use crate::alerts::Id;
use crate::common::{id_of, me, text, Record};
use crate::rules;

#[derive(Deserialize)]
pub struct Job {
    pub id: String,
    pub chain: String,
}

/// Queue a check of this alert for the person calling, `delay` seconds from now.
pub fn queue(alert: &str, chain: &str, delay: u64) -> Result<()> {
    let user = me()?;
    scheduler::enqueue("run_alert")
        .payload(&json!({ "id": alert, "chain": chain }))
        .delay_secs(delay)
        .unique_key(&format!("alert:{alert}:{chain}:{delay}:{}", context::current()?.now))
        .run_as(&user)
        .send()?;
    Ok(())
}

/// What a check found.
struct Reading {
    value: Option<f64>,
    change: Option<f64>,
}

fn read(alert: &Record) -> Result<Reading> {
    let chart = text(alert, "chart").ok_or_else(|| Error::msg("the alert has no chart"))?;
    let card: Value = plugins::call("prism", "run_chart", &json!({ "chart": chart }))?;
    let wanted = text(alert, "reading").filter(|name| !name.is_empty());
    let readings = card["readings"].as_array().cloned().unwrap_or_default();
    let reading = match wanted {
        Some(name) => readings
            .iter()
            .find(|r| r["name"] == name || r["label"] == name)
            .ok_or_else(|| Error::msg(format!("the chart has no reading called `{name}`")))?,
        None => readings.first().ok_or_else(|| Error::msg("the chart shows no figure"))?,
    };
    Ok(Reading { value: reading["value"].as_f64(), change: reading["change"].as_f64() })
}

/// Look at the chart now and record what was found. Returns whether the owner was told.
fn evaluate(alert: &Record) -> Result<Value> {
    let id = id_of(alert)?.to_string();
    let now = context::current()?.now;
    let limit = alert.get("threshold").and_then(Value::as_f64);
    let condition = text(alert, "condition").unwrap_or("below");
    let was = text(alert, "state").unwrap_or("new");
    let outcome = read(alert).and_then(|found| {
        let firing = rules::firing(condition, limit, found.value, found.change).map_err(Error::msg)?;
        Ok((found, firing))
    });
    let (found, firing) = match outcome {
        Ok(done) => done,
        Err(error) => {
            let words: String = error.to_string().chars().take(300).collect();
            db::update::<Record>("prism_alert", &id, &json!({ "state": "error", "last_error": words, "last_checked": now }))?;
            return Ok(json!({ "state": "error", "error": words }));
        }
    };
    let state = if firing { "firing" } else { "ok" };
    let mut changes = json!({ "state": state, "last_value": found.value, "last_checked": now, "last_error": "" });
    let tell = (firing && was != "firing") || (!firing && was == "firing" && alert.get("notify_clear").and_then(Value::as_bool).unwrap_or(true));
    if firing && was != "firing" {
        changes["last_fired"] = json!(now);
    }
    db::update::<Record>("prism_alert", &id, &changes)?;
    if tell {
        tell_owner(alert, &id, firing, &found, limit)?;
    }
    Ok(json!({ "state": state, "value": found.value, "told": tell }))
}

fn tell_owner(alert: &Record, id: &str, firing: bool, found: &Reading, limit: Option<f64>) -> Result<()> {
    let name = text(alert, "name").unwrap_or("Alert");
    let custom = text(alert, "message").filter(|m| !m.trim().is_empty());
    let body = match (firing, custom) {
        (true, Some(template)) => rules::fill(template, name, found.value, limit, found.change),
        (true, None) => rules::fill("{name} needs attention: {value} (limit {limit}, change {change})", name, found.value, limit, found.change),
        (false, _) => rules::fill("{name} is back to normal: {value}", name, found.value, limit, found.change),
    };
    let title = if firing { format!("{name} needs attention") } else { format!("{name} is back to normal") };
    Notification::new(title.clone())
        .body(body.clone())
        .link(format!("/prism/alerts/{id}"))
        .level(if firing { Level::Warning } else { Level::Success })
        .to(Audience::Actors(vec![me()?]))
        .send()?;
    if let Some(emails) = text(alert, "emails") {
        let to = rules::addresses(emails).map_err(Error::msg)?;
        if !to.is_empty() {
            // A mail that cannot go does not undo the check; the notification above already reached the owner.
            let _ = communication::email(to).subject(&title).text(&body).send();
        }
    }
    Ok(())
}

/// The background job: runs as the owner.
pub fn run_alert(job: Job) -> Result<Value> {
    let me = me()?;
    let Some(alert) = db::find::<Record>("prism_alert").filter("id", job.id.as_str()).first()? else {
        return Ok(json!({ "skipped": "the alert is gone" }));
    };
    // A job of an older chain, an alert that was turned off, or one that is not this person's, ends here.
    let live = text(&alert, "chain") == Some(job.chain.as_str())
        && alert.get("active").and_then(Value::as_bool).unwrap_or(true)
        && text(&alert, "owner") == Some(me.as_str());
    if !live {
        return Ok(json!({ "skipped": "the alert changed or was turned off" }));
    }
    let seconds = rules::interval_secs(text(&alert, "frequency").unwrap_or("daily")).unwrap_or(86_400);
    queue(&job.id, &job.chain, seconds)?;
    evaluate(&alert)
}

/// "Check now" from the alert's page: the same check, once, without touching the schedule.
pub fn check_now(input: Id) -> Result<Value> {
    crate::common::require_analyst()?;
    let alert = crate::common::require("prism_alert", &input.id, "alert")?;
    if text(&alert, "owner") != Some(me()?.as_str()) {
        return Err(Error::msg("only the person who made this alert can check it"));
    }
    evaluate(&alert)
}
