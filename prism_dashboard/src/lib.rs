//! Prism dashboards: a grid of charts under a bar of shared filters.

mod board;
mod common;
mod crud;
mod filters;

use aether_sdk::prelude::*;

use crate::board::{BoardInput, DrillInput};
use crate::crud::{Id, Move, Search};

handler! {
    fn dashboard_board(input: BoardInput) -> Value { board::dashboard_board(input) }
    fn dashboard_drill(input: DrillInput) -> Value { board::dashboard_drill(input) }

    fn create_dashboard(input: Value) -> Value { crud::create_dashboard(input) }
    fn set_dashboard(input: Value) -> Value { crud::set_dashboard(input) }
    fn get_dashboard(input: Id) -> Value { crud::get_dashboard(input) }
    fn search_dashboards(input: Search) -> Value { crud::search_dashboards(input) }
    fn star_dashboard(input: Id) -> Value { crud::star_dashboard(input) }
    fn unstar_dashboard(input: Id) -> Value { crud::unstar_dashboard(input) }
    fn delete_dashboard(input: Id) -> Value { crud::delete_dashboard(input) }
    fn duplicate_dashboard(input: Id) -> Value { crud::duplicate_dashboard(input) }

    fn list_items(input: Id) -> Vec<Value> { crud::list_items(input) }
    fn add_item(input: Value) -> Value { crud::add_item(input) }
    fn add_note(input: Value) -> Value { crud::add_note(input) }
    fn set_item(input: Value) -> Value { crud::set_item(input) }
    fn remove_item(input: Id) -> Value { crud::remove_item(input) }
    fn move_item(input: Move) -> Value { crud::move_item(input) }

    fn item_up(input: Id) -> Value { crud::item_up(input) }
    fn item_down(input: Id) -> Value { crud::item_down(input) }

    fn list_filters(input: Id) -> Vec<Value> { crud::list_filters(input) }
    fn add_filter(input: Value) -> Value { crud::add_filter(input) }
    fn set_filter(input: Value) -> Value { crud::set_filter(input) }
    fn remove_filter(input: Id) -> Value { crud::remove_filter(input) }
}
