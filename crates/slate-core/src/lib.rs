//! Shared engine for the contact sheet app: project data, discovery, batch
//! operations, layout and PDF export.
//!
//! Keeping this separate from the GPUI binary means sheets can be exported and
//! tested without a display server.

pub mod batch;
pub mod convert;
pub mod import;
pub mod layout;
pub mod model;
pub mod pdf;
pub mod store;

pub use batch::{RenameOptions, RenamePlan, RenameReport, preview_name};
pub use import::{PreviewImage, Probe};
pub use layout::{PagePlan, Placement, SheetLayout, SheetPlan};
pub use model::{
    Filter, Orientation, Paper, Photo, PhotoId, Project, SavedSheet, SheetId, SheetSettings,
};
pub use pdf::{ExportSummary, export_pdf, export_saved_sheet};
