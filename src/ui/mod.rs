//! The GTK4 / libadwaita presentation layer.
//!
//! The UI owns widgets, input, and render state only; all cross-boundary data
//! arrives as typed events from `events.rs` (Arch §5). It never imports
//! `index/`, `db/` (beyond opaque persistence handles), `thumbnail/`, or
//! filesystem modules directly. The widget tree is assembled bottom-up in
//! [`window::build`] and mirrors Architecture §9 one-to-one.

pub mod filter_controller;
pub mod grid_cell;
pub mod header;
pub mod model;
pub mod selection_bar;
pub mod settings;
pub mod shortcuts;
pub mod sidebar;
pub mod viewer;
pub mod window;

pub use window::build as build_ui;
