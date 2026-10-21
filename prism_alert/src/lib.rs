//! Prism alerts: tell me when a number crosses a line.

mod alerts;
mod common;
mod rules;
mod run;

use aether_sdk::prelude::*;

use crate::alerts::{Id, Search};
use crate::run::Job;

handler! {
    fn create_alert(input: Value) -> Value { alerts::create_alert(input) }
    fn set_alert(input: Value) -> Value { alerts::set_alert(input) }
    fn get_alert(input: Id) -> Value { alerts::get_alert(input) }
    fn search_alerts(input: Search) -> Value { alerts::search_alerts(input) }
    fn delete_alert(input: Id) -> Value { alerts::delete_alert(input) }
    fn check_alert_now(input: Id) -> Value { run::check_now(input) }

    // The background job. It runs as the alert's owner and queues its own next run.
    fn run_alert(input: Job) -> Value { run::run_alert(input) }
}
