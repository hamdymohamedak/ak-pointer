//! Record and replay mouse and keyboard actions against a chosen macOS window.
//!
//! The crate is split by responsibility:
//! - [`model`] is the action file's in-memory shape and text editing. It does not touch macOS.
//! - [`store`] loads and saves those actions.
//! - [`platform`] talks to windows, permissions, and CoreGraphics.
//! - [`engine`] records and plays an action.
//! - [`ui`] is the window the user works in.

pub mod engine;
pub mod model;
pub mod platform;
pub mod store;
pub mod ui;
