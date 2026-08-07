//! Page rendering.
//!
//! This was a local copy of forma-server's page template. As of forma-server
//! 0.2.0 the framework does the same job with the same guarantees, so the copy
//! is gone and this module is a thin seam over it — handlers keep importing
//! `crate::html_template::{render_page, PageConfig, PageOutput}` unchanged.
//!
//! What the framework now owns, and what the local copy used to get wrong:
//!
//! * `personality_css` is emitted with `</` neutralized as a lossless CSS
//!   escape. The local copy replaced the two literal spellings `</style>` and
//!   `</STYLE>`, which missed `</StYlE>`, `</style\n>`, `</style/` and
//!   `</style bar>` — end-tag matching is ASCII-case-insensitive and accepts
//!   whitespace or `/` after the name.
//! * `config_script` has both script-data exits neutralized: `</script` and
//!   `<!--`. It remains a raw, trusted escape hatch — anything derived from
//!   user input belongs in [`PageConfig::config_json`], which serializes with
//!   every `<` escaped.
//! * The CSP nonce is a [`CspNonce`] newtype rather than a bare `&str`, so it
//!   cannot be interpolated from an arbitrary string.

pub use forma_server::csp::CspNonce;
pub use forma_server::template::render_page;
pub use forma_server::types::{PageConfig, PageOutput};
