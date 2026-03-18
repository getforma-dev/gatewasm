//! HTML template rendering — delegates to forma-server.
//!
//! Previously a local copy; now uses the published crate which includes
//! HTML escaping on all user-controlled fields.

pub use forma_server::template::render_page;
pub use forma_server::types::{PageConfig, PageOutput};
