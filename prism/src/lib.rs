//! Prism: workbooks, metrics and charts over any data the person may read.

mod api;
mod common;
mod controls;
mod crud;
mod dsl;
mod engine;
mod periods;
mod source;

use aether_sdk::prelude::*;

use crate::api::{ChartBoard, ChartDrill, DrillChart, ExportChart, FieldValues, RunChart, SourceRef, Sources};
use crate::crud::{Id, Search};

handler! {
    fn search_sources(input: Sources) -> Vec<Value> { api::search_sources(input) }
    fn get_source(input: SourceRef) -> Value { api::get_source(input) }

    fn field_values(input: FieldValues) -> Vec<Value> { api::field_values(input) }
    fn run_chart(input: RunChart) -> Value { api::run_chart(input) }
    fn drill_chart(input: DrillChart) -> Value { api::drill_chart(input) }
    fn export_chart(input: ExportChart) -> Value { api::export_chart(input) }
    fn chart_drill(input: ChartDrill) -> Value { api::chart_drill(input) }
    fn chart_board(input: ChartBoard) -> Value { api::chart_board(input) }

    fn create_workbook(input: Value) -> Value { crud::create_workbook(input) }
    fn set_workbook(input: Value) -> Value { crud::set_workbook(input) }
    fn get_workbook(input: Id) -> Value { crud::get_workbook(input) }
    fn search_workbooks(input: Search) -> Value { crud::search_workbooks(input) }
    fn delete_workbook(input: Id) -> Value { crud::delete_workbook(input) }
    fn list_workbook_charts(input: Id) -> Vec<Value> { crud::list_workbook_charts(input) }

    fn create_metric(input: Value) -> Value { crud::create_metric(input) }
    fn set_metric(input: Value) -> Value { crud::set_metric(input) }
    fn get_metric(input: Id) -> Value { crud::get_metric(input) }
    fn search_metrics(input: Search) -> Value { crud::search_metrics(input) }
    fn delete_metric(input: Id) -> Value { crud::delete_metric(input) }

    fn create_chart(input: Value) -> Value { crud::create_chart(input) }
    fn set_chart(input: Value) -> Value { crud::set_chart(input) }
    fn get_chart(input: Id) -> Value { crud::get_chart(input) }
    fn search_charts(input: Search) -> Value { crud::search_charts(input) }
    fn delete_chart(input: Id) -> Value { crud::delete_chart(input) }
    fn duplicate_chart(input: Id) -> Value { crud::duplicate_chart(input) }
}
