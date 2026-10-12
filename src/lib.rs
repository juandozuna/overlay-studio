//! Native scoreboard core and adapters, shared by desktop presentation targets.
//!
//! Domain and application modules are independent of the desktop widget toolkit.
//! Renderer and infrastructure modules retain their native adapter dependencies.
pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod layout_ai;
pub mod media;
pub mod model;
pub mod output;
pub mod recovery;
pub mod remote;
pub mod scene;
