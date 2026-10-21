//! Helpers shared by the parts of the plugin.

pub use aether_sdk::records::{id_of, require, text, Record};

use aether_sdk::prelude::*;

pub const VIEWER_ROLE: &str = "prism.prism_viewer";
pub const ANALYST_ROLE: &str = "prism.prism_analyst";
pub const ADMIN_ROLE: &str = "prism.prism_admin";

/// Anyone with a Prism role may read what the rules show them.
pub fn require_viewer() -> Result<()> {
    let context = context::current()?;
    if [VIEWER_ROLE, ANALYST_ROLE, ADMIN_ROLE].iter().any(|role| context.has_role(role)) {
        Ok(())
    } else {
        Err(Error::msg(format!("you need the role `{VIEWER_ROLE}` to do this")))
    }
}

/// Building needs analyst rights.
pub fn require_analyst() -> Result<()> {
    let context = context::current()?;
    if context.has_role(ANALYST_ROLE) || context.has_role(ADMIN_ROLE) {
        Ok(())
    } else {
        Err(Error::msg(format!("you need the role `{ANALYST_ROLE}` to do this")))
    }
}

/// The caller's account id (`users:…`); empty for the kernel's own jobs (an install seed), whose
/// records belong to nobody and are changed by administrators.
pub fn me() -> Result<String> {
    Ok(context::current()?.actor.id.unwrap_or_default())
}

/// A number from a JSON number or a decimal sent as text; `None` for anything else.
pub fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse().ok(),
        _ => None,
    }
}
