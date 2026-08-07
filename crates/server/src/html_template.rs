use crate::assets;
use crate::middleware::csp;
use crate::{AssetManifest, RenderMode};
use forma_ir as ir;

pub use forma_server::types::{PageConfig, PageOutput};

struct ResolvedAssets {
    fonts: Vec<String>,
    css_urls: Vec<String>,
    js_urls: Vec<String>,
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn build_head(
    title: &str,
    nonce: &str,
    assets: &ResolvedAssets,
    personality_css: Option<&str>,
) -> String {
    let mut head = String::with_capacity(2048);
    head.push_str("<meta charset=\"utf-8\">\n");
    head.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    head.push_str(&format!("<title>{}</title>\n", escape_html(title)));

    for font in &assets.fonts {
        head.push_str(&format!(
            "<link rel=\"preload\" href=\"{}\" as=\"font\" type=\"font/woff2\" crossorigin>\n",
            escape_html(font)
        ));
    }

    for css in &assets.css_urls {
        head.push_str(&format!(
            "<link rel=\"stylesheet\" href=\"{}\">\n",
            escape_html(css)
        ));
    }

    for js in &assets.js_urls {
        head.push_str(&format!(
            "<link rel=\"modulepreload\" href=\"{}\">\n",
            escape_html(js)
        ));
    }

    if let Some(css) = personality_css {
        head.push_str(&format!(
            "<style nonce=\"{nonce}\">{}</style>\n",
            css_text_safe(css)
        ));
    }

    head
}

/// Neutralize a string for embedding inside a `<style>` element.
///
/// `<style>` content is HTML RAWTEXT: character references are not decoded,
/// so there is nothing to "escape" in the usual sense. The tokenizer leaves
/// RAWTEXT only via `</` (end-tag-open), and the tag name that follows is
/// matched ASCII-case-insensitively and may be terminated by whitespace, `/`
/// or `>`. Enumerating spellings therefore cannot work — the previous
/// `.replace("</style>", ..)` caught two of them and missed `</StYlE>`,
/// `</style\n>`, `</style/` and `</style bar>`.
///
/// Neutralizing `</` itself is exact, because it is the only sequence that can
/// begin the exit. `\/` is a valid CSS escape for `/`, so a legitimate
/// declaration survives, and a lone `<` is left alone so Media Queries Level 4
/// range syntax (`@media (400px < width)`) still works.
fn css_text_safe(css: &str) -> String {
    css.replace("</", "<\\/")
}

fn resolve_assets(manifest: &AssetManifest, route_pattern: &str) -> ResolvedAssets {
    let route = manifest.route(route_pattern);
    ResolvedAssets {
        fonts: route
            .map(|r| r.fonts.iter().map(|f| format!("/_assets/{f}")).collect())
            .unwrap_or_default(),
        css_urls: route
            .map(|r| r.css.iter().map(|f| format!("/_assets/{f}")).collect())
            .unwrap_or_default(),
        js_urls: route
            .map(|r| r.js.iter().map(|f| format!("/_assets/{f}")).collect())
            .unwrap_or_default(),
    }
}

struct BodyParts {
    body_class_attr: String,
    body_prefix: String,
    config_script_tag: String,
    page_js_tag: String,
    wasm_script: String,
    sw_script: String,
}

fn build_body_parts(nonce: &str, config: &PageConfig, js_urls: &[String]) -> BodyParts {
    let body_class_attr = config
        .body_class
        .map(|c| format!(" class=\"{}\"", escape_html(c)))
        .unwrap_or_default();
    let body_prefix = config.body_prefix.unwrap_or("").to_string();
    let config_script_tag = config
        .config_script
        .map(|s| format!("<script nonce=\"{nonce}\">{s}</script>\n"))
        .unwrap_or_default();

    let page_js_tag = js_urls
        .last()
        .map(|url| {
            format!(
                "<script type=\"module\" nonce=\"{nonce}\" src=\"{}\"></script>",
                escape_html(url)
            )
        })
        .unwrap_or_default();

    let wasm_script = match (
        &config.manifest.wasm,
        config.manifest.route(config.route_pattern),
    ) {
        (Some(wasm), Some(route)) => match route.ir.as_ref() {
            Some(ir_name) => format!(
                "<script nonce=\"{nonce}\">window.__FORMA_WASM__={{loader:\"/_assets/{}\",binary:\"/_assets/{}\",ir:\"/_assets/{}\"}};</script>\n",
                escape_html(&wasm.loader), escape_html(&wasm.binary), escape_html(ir_name)
            ),
            None => String::new(),
        },
        _ => String::new(),
    };

    let sw_script = if assets::use_filesystem_assets() {
        String::new()
    } else {
        format!(
            "<script nonce=\"{nonce}\">if('serviceWorker' in navigator)navigator.serviceWorker.register('/sw.js');</script>\n"
        )
    };

    BodyParts {
        body_class_attr,
        body_prefix,
        config_script_tag,
        page_js_tag,
        wasm_script,
        sw_script,
    }
}

pub fn render_page(config: &PageConfig) -> PageOutput {
    match config.render_mode {
        RenderMode::Phase1ClientMount => render_page_phase1(config),
        RenderMode::Phase2SsrReconcile => render_page_phase2(config),
    }
}

fn render_page_phase1(config: &PageConfig) -> PageOutput {
    let nonce = csp::generate_csp_nonce();
    let assets = resolve_assets(config.manifest, config.route_pattern);
    let head = build_head(config.title, &nonce, &assets, config.personality_css);
    let parts = build_body_parts(&nonce, config, &assets.js_urls);

    let html = format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n{head}</head>\n<body{bc}>\n{bp}<div id=\"app\"></div>\n{cs}{pj}\n{wasm}{sw}</body>\n</html>",
        bc = parts.body_class_attr,
        bp = parts.body_prefix,
        cs = parts.config_script_tag,
        pj = parts.page_js_tag,
        wasm = parts.wasm_script,
        sw = parts.sw_script,
    );

    PageOutput {
        html,
        csp: csp::build_csp_header(&nonce),
    }
}

fn render_page_phase2(config: &PageConfig) -> PageOutput {
    let (ir_module, slots) = match (config.ir_module, config.slots) {
        (Some(m), Some(s)) => (m, s),
        _ => {
            tracing::warn!(
                route = config.route_pattern,
                "Phase 2 SSR: missing IR module or slots, falling back to Phase 1"
            );
            return render_page_phase1(config);
        }
    };

    let ssr_body = match ir::walker::walk_to_html(ir_module, slots) {
        Ok(html) => html,
        Err(err) => {
            tracing::warn!(
                route = config.route_pattern,
                error = %err,
                "Phase 2 SSR: IR walk failed, falling back to Phase 1"
            );
            return render_page_phase1(config);
        }
    };

    let nonce = csp::generate_csp_nonce();
    let assets = resolve_assets(config.manifest, config.route_pattern);
    let head = build_head(config.title, &nonce, &assets, config.personality_css);
    let parts = build_body_parts(&nonce, config, &assets.js_urls);

    let html = format!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n{head}</head>\n<body{bc}>\n{bp}<div id=\"app\" data-forma-ssr>{ssr}</div>\n{cs}{pj}\n{wasm}{sw}</body>\n</html>",
        bc = parts.body_class_attr,
        bp = parts.body_prefix,
        ssr = ssr_body,
        cs = parts.config_script_tag,
        pj = parts.page_js_tag,
        wasm = parts.wasm_script,
        sw = parts.sw_script,
    );

    PageOutput {
        html,
        csp: csp::build_csp_header(&nonce),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_assets() -> ResolvedAssets {
        ResolvedAssets {
            fonts: vec![],
            css_urls: vec![],
            js_urls: vec![],
        }
    }

    #[test]
    fn css_text_safe_blocks_every_end_tag_spelling() {
        // End-tag matching is ASCII-case-insensitive and the tag name may be
        // followed by whitespace, `/` or `>`. The previous two-literal
        // replace caught only `</style>` and `</STYLE>`.
        for payload in [
            "</style>",
            "</STYLE>",
            "</StYlE>",
            "</style\n>",
            "</style\t>",
            "</style/",
            "</style bar>",
            "</style",
        ] {
            let out = css_text_safe(&format!("body{{color:red}}{payload}<img src=x>"));
            assert!(
                !out.contains("</"),
                "`{payload}` left an end-tag-open in {out:?}"
            );
        }
    }

    #[test]
    fn css_text_safe_leaves_ordinary_css_alone() {
        let css = "body{background:#0a0a0c;--glow-size:1100px;}";
        assert_eq!(css_text_safe(css), css);
        // Media Queries L4 range syntax survives — this is why we neutralize
        // `</` rather than stripping every `<`.
        assert_eq!(
            css_text_safe("@media (400px < width)"),
            "@media (400px < width)"
        );
    }

    #[test]
    fn personality_css_cannot_close_the_style_element() {
        let head = build_head("t", "N0NCE", &no_assets(), Some("</StYlE ><img src=x>"));
        // Match case-insensitively on the tag-open prefix. Asserting on the
        // literal `</style>` would be case-sensitive, so the mixed-case
        // payload could never match it and the test would pass even with the
        // escaper deleted.
        assert_eq!(
            head.to_lowercase().matches("</style").count(),
            1,
            "branding closed the <style> element: {head}"
        );
        assert!(
            head.contains("<\\/StYlE"),
            "payload should be neutralized in place, not stripped: {head}"
        );
    }

    #[test]
    fn build_head_escapes_the_title_exactly_once() {
        let head = build_head("A&B <x>", "N0NCE", &no_assets(), None);
        assert!(head.contains("<title>A&amp;B &lt;x&gt;</title>"));
        assert!(!head.contains("&amp;amp;"), "title was double-escaped");
    }
}
