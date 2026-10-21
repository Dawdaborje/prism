//! Helpers shared by the parts of the plugin.

pub use aether_sdk::records::{id_of, pick, require, text, Record};

use aether_sdk::prelude::*;

pub const VIEWER_ROLE: &str = "prism.prism_viewer";
pub const ANALYST_ROLE: &str = "prism.prism_analyst";
pub const ADMIN_ROLE: &str = "prism.prism_admin";

pub fn require_viewer() -> Result<()> {
    let context = context::current()?;
    if [VIEWER_ROLE, ANALYST_ROLE, ADMIN_ROLE].iter().any(|role| context.has_role(role)) {
        Ok(())
    } else {
        Err(Error::msg(format!("you need the role `{VIEWER_ROLE}` to do this")))
    }
}

pub fn require_analyst() -> Result<()> {
    let context = context::current()?;
    if context.has_role(ANALYST_ROLE) || context.has_role(ADMIN_ROLE) {
        Ok(())
    } else {
        Err(Error::msg(format!("you need the role `{ANALYST_ROLE}` to do this")))
    }
}

/// The caller's account id (`users:…`).
pub fn me() -> Result<String> {
    context::current()?.actor.id.ok_or_else(|| Error::msg("you need to be signed in"))
}

/// A required, trimmed text field.
pub fn required(input: &Value, field: &str, what: &str) -> Result<String> {
    text(input, field)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| Error::msg(format!("{what} is required")))
}
