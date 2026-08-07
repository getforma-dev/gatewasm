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
pub use forma_server::types::{PageConfig, PageOutput};

/// Render a page, suppressing the service-worker registration in dev.
///
/// forma-server emits the registration unconditionally. When the server is
/// reading assets from `admin/dist` on the filesystem, a service worker caches
/// them and the next edit silently serves stale JS — the "something looks
/// stale" failure CONTRIBUTING sends people to. The local template used to
/// gate this and the gate was lost when rendering moved to the framework;
/// `smoke.spec.ts` caught it.
pub fn render_page(config: &PageConfig) -> PageOutput {
    let mut out = forma_server::template::render_page(config);
    if crate::assets::use_filesystem_assets() {
        let sw = sw_registration_tag(out.nonce.as_str());
        debug_assert!(
            out.html.contains(&sw),
            "forma-server changed the service-worker snippet; \
             sw_registration_tag no longer matches and dev would silently \
             register a worker again"
        );
        out.html = out.html.replace(&sw, "");
    }
    out
}

/// The exact element forma-server emits to register the service worker.
///
/// Reconstructed rather than pattern-matched so a change upstream fails the
/// assertion above instead of quietly leaving the worker in place.
fn sw_registration_tag(nonce: &str) -> String {
    format!(
        "<script nonce=\"{nonce}\">if('serviceWorker' in navigator)navigator.serviceWorker.register('/sw.js');</script>\n"
    )
}
