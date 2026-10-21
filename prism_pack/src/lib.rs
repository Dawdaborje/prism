//! Prism packs: ready-made dashboards for the apps that are installed.

mod install;
mod packs;

use aether_sdk::prelude::*;

#[derive(Deserialize)]
struct Code {
    /// The pack's code; `id` is accepted so a list row action can send it.
    #[serde(alias = "id")]
    code: String,
}

handler! {
    /// Installs the packs of every installed app; runs on install and upgrade.
    fn seed_packs(_: Empty) -> Value { install::seed() }

    /// The packs, whether their app is installed, and whether they have been installed here.
    fn list_packs(_: Empty) -> Vec<Value> { install::list() }

    fn install_pack(input: Code) -> Value { install::install_by_code(&input.code) }
}
