//! Exact spreadsheet domain for SIM office documents.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod doc;
pub mod formula;
pub mod graph;
pub mod model;

pub use doc::{SHEET_EDIT_DOMAIN, apply_sheet_edit, doc_to_sheet, set_cell_edit, sheet_to_doc};
pub use formula::{SheetFormulaEngine, eval_formula};
pub use graph::{
    ExcelBridgeRangeReply, ExcelBridgeRangeRequest, GRAPH_RANGE_EDIT_DOMAIN, MsGraphSite,
    WorkbookRangeTarget, plan_write_graph_range, read_graph_range,
};
pub use model::{
    CellRef, CellValue, SHEET_DOC_KIND, Sheet, SheetError, rational_from_str, rational_to_canonical,
};

/// Cookbook recipes for this lib, embedded at build time.
pub static RECIPES: sim_cookbook::EmbeddedDir =
    include!(concat!(env!("OUT_DIR"), "/cookbook_recipes.rs"));

#[cfg(test)]
mod tests;
