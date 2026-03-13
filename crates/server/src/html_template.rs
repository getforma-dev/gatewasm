use crate::ir;
use crate::middleware::csp;
use crate::RenderMode;

// Re-export page types from forma-server so handler code using
// `crate::html_template::PageConfig`, `PageOutput` continues to work.
pub use forma_server::types::{PageConfig, PageOutput};
use crate::assets::AssetManifest;

/// Resolved asset URLs for a route — shared between Phase 1 and Phase 2 paths.
struct ResolvedAssets {
    fonts: Vec<String>,
    css_urls: Vec<String>,
    js_urls: Vec<String>,
}

/// Build the common `<head>` content shared by both render paths.
fn build_head(
    title: &str,
    nonce: &str,
    assets: &ResolvedAssets,
    personality_css: Option<&str>,
) -> String {
    let mut head = String::with_capacity(2048);
    head.push_str("<meta charset=\"utf-8\">\n");
    head.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    head.push_str(&format!("<title>{}</title>\n", title));

    // Font preloads
    for font in &assets.fonts {
        head.push_str(&format!(
            "<link rel=\"preload\" href=\"{font}\" as=\"font\" type=\"font/woff2\" crossorigin>\n"
        ));
    }

    // CSS stylesheets
    for css in &assets.css_urls {
        head.push_str(&format!("<link rel=\"stylesheet\" href=\"{css}\">\n"));
    }

    // JS modulepreloads
    for js in &assets.js_urls {
        head.push_str(&format!("<link rel=\"modulepreload\" href=\"{js}\">\n"));
    }

    // Personality CSS (inline, small)
    if let Some(css) = personality_css {
        head.push_str(&format!("<style nonce=\"{nonce}\">{css}</style>\n"));
    }

    head
}

/// Resolve asset URLs from the manifest for a given route pattern.
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

/// Build shared body fragments (class attr, prefix, config script, page JS tag).
struct BodyParts {
    body_class_attr: String,
    body_prefix: String,
    config_script_tag: String,
    page_js_tag: String,
    wasm_script: String,
    sw_script: String,
}

fn build_body_parts(
    nonce: &str,
    config: &PageConfig,
    js_urls: &[String],
) -> BodyParts {
    let body_class_attr = config
        .body_class
        .map(|c| format!(" class=\"{c}\""))
        .unwrap_or_default();
    let body_prefix = config.body_prefix.unwrap_or("").to_string();
    let config_script_tag = config
        .config_script
        .map(|s| format!("<script nonce=\"{nonce}\">{s}</script>\n"))
        .unwrap_or_default();

    let page_js_tag = js_urls
        .last()
        .map(|url| format!("<script type=\"module\" src=\"{url}\"></script>"))
        .unwrap_or_default();

    let wasm_script = match (&config.manifest.wasm, config.manifest.route(config.route_pattern)) {
        (Some(wasm), Some(route)) if route.ir.is_some() => {
            let ir_name = route.ir.as_ref().unwrap();
            format!(
                "<script nonce=\"{nonce}\">window.__FORMA_WASM__={{loader:\"/_assets/{}\",binary:\"/_assets/{}\",ir:\"/_assets/{}\"}};</script>\n",
                wasm.loader, wasm.binary, ir_name
            )
        }
        _ => String::new(),
    };

    let sw_script = format!(
        "<script nonce=\"{nonce}\">if('serviceWorker' in navigator)navigator.serviceWorker.register('/sw.js');</script>\n"
    );

    BodyParts {
        body_class_attr,
        body_prefix,
        config_script_tag,
        page_js_tag,
        wasm_script,
        sw_script,
    }
}

/// Render a complete HTML page from a PageConfig.
///
/// NOTE: This is kept as a local implementation rather than delegating to
/// `forma_server::render_page` because the GateWASM version intentionally
/// omits the `nonce` attribute on external `<script type="module" src="...">` tags
/// (the CSP `'self'` directive covers same-origin scripts). The forma-server
/// version adds `nonce` redundantly. Keeping local avoids any Playwright test
/// regressions from changed HTML output.
///
/// TODO: Align with forma-server::render_page once both versions converge.
pub fn render_page(config: &PageConfig) -> PageOutput {
    match config.render_mode {
        RenderMode::Phase1ClientMount => render_page_phase1(config),
        RenderMode::Phase2SsrReconcile => render_page_phase2(config),
    }
}

/// Phase 1: empty `<div id="app"></div>` — client JS mounts from scratch.
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

/// Phase 2: server-render the IR module into `<div id="app" data-forma-ssr>`.
/// Falls back to Phase 1 on any error (missing IR module, walk failure, etc.).
fn render_page_phase2(config: &PageConfig) -> PageOutput {
    // Both ir_module and slots must be present for SSR
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

    // Attempt IR walk
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

    // SSR succeeded — build the full page with SSR content
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
    use crate::assets::{AssetManifest, RouteAssets, WasmAssets};
    use crate::ir::parser::IrModule;
    use crate::ir::slot::{SlotData, SlotValue};
    use crate::RenderMode;
    use std::collections::HashMap;

    /// Build a minimal AssetManifest with no routes for testing.
    fn empty_manifest() -> AssetManifest {
        AssetManifest {
            version: 1,
            build_hash: "test".to_string(),
            assets: HashMap::new(),
            routes: HashMap::new(),
            wasm: None,
        }
    }

    /// Build a valid IR module that renders "<p>hello</p>".
    fn hello_ir_module() -> IrModule {
        use forma_ir::parser::test_helpers::{build_minimal_ir, encode_open_tag, encode_close_tag, encode_text};

        let mut opcodes = Vec::new();
        opcodes.extend_from_slice(&encode_open_tag(0, &[])); // <p>
        opcodes.extend_from_slice(&encode_text(1));          // hello
        opcodes.extend_from_slice(&encode_close_tag(0));     // </p>

        let data = build_minimal_ir(&["p", "hello"], &[], &opcodes, &[]);
        IrModule::parse(&data).unwrap()
    }

    #[test]
    fn phase1_renders_empty_app_div() {
        let manifest = empty_manifest();
        let page = render_page(&PageConfig {
            title: "Test",
            route_pattern: "/test",
            manifest: &manifest,
            config_script: None,
            body_class: None,
            personality_css: None,
            body_prefix: None,
            render_mode: RenderMode::Phase1ClientMount,
            ir_module: None,
            slots: None,
        });

        assert!(page.html.contains("<div id=\"app\"></div>"));
        // Check that the #app div does NOT have the SSR attribute (runtime JS may contain the string)
        assert!(!page.html.contains("<div id=\"app\" data-forma-ssr>"));
    }

    #[test]
    fn phase2_renders_ssr_content_with_data_attr() {
        let manifest = empty_manifest();
        let ir = hello_ir_module();
        let slots = SlotData::new(0);

        let page = render_page(&PageConfig {
            title: "SSR Test",
            route_pattern: "/test",
            manifest: &manifest,
            config_script: None,
            body_class: None,
            personality_css: None,
            body_prefix: None,
            render_mode: RenderMode::Phase2SsrReconcile,
            ir_module: Some(&ir),
            slots: Some(&slots),
        });

        assert!(page.html.contains("data-forma-ssr"));
        assert!(page.html.contains("<p>hello</p>"));
        assert!(page.html.contains("<div id=\"app\" data-forma-ssr>"));
    }

    #[test]
    fn phase2_falls_back_to_phase1_when_ir_module_missing() {
        let manifest = empty_manifest();

        let page = render_page(&PageConfig {
            title: "Fallback Test",
            route_pattern: "/test",
            manifest: &manifest,
            config_script: None,
            body_class: None,
            personality_css: None,
            body_prefix: None,
            render_mode: RenderMode::Phase2SsrReconcile,
            ir_module: None,
            slots: None,
        });

        // Should fall back to Phase 1: empty #app, no data-forma-ssr
        assert!(page.html.contains("<div id=\"app\"></div>"));
        assert!(!page.html.contains("<div id=\"app\" data-forma-ssr>"));
    }

    #[test]
    fn phase2_falls_back_to_phase1_when_slots_missing() {
        let manifest = empty_manifest();
        let ir = hello_ir_module();

        let page = render_page(&PageConfig {
            title: "Fallback Test",
            route_pattern: "/test",
            manifest: &manifest,
            config_script: None,
            body_class: None,
            personality_css: None,
            body_prefix: None,
            render_mode: RenderMode::Phase2SsrReconcile,
            ir_module: Some(&ir),
            slots: None,
        });

        assert!(page.html.contains("<div id=\"app\"></div>"));
        assert!(!page.html.contains("<div id=\"app\" data-forma-ssr>"));
    }

    #[test]
    fn phase2_falls_back_on_ir_walk_error() {
        let manifest = empty_manifest();
        // Build an IR module with deliberately corrupt opcodes
        let data = forma_ir::parser::test_helpers::build_minimal_ir(
            &["x"],
            &[],
            &[0xFF], // invalid opcode byte
            &[],
        );
        let ir = IrModule::parse(&data).unwrap();
        let slots = SlotData::new(0);

        let page = render_page(&PageConfig {
            title: "Walk Error Test",
            route_pattern: "/test",
            manifest: &manifest,
            config_script: None,
            body_class: None,
            personality_css: None,
            body_prefix: None,
            render_mode: RenderMode::Phase2SsrReconcile,
            ir_module: Some(&ir),
            slots: Some(&slots),
        });

        // Should fall back to Phase 1
        assert!(page.html.contains("<div id=\"app\"></div>"));
        assert!(!page.html.contains("<div id=\"app\" data-forma-ssr>"));
    }

    #[test]
    fn phase2_ssr_preserves_head_content() {
        let manifest = empty_manifest();
        let ir = hello_ir_module();
        let slots = SlotData::new(0);

        let page = render_page(&PageConfig {
            title: "Head Test",
            route_pattern: "/test",
            manifest: &manifest,
            config_script: Some("window.__TEST__=true;"),
            body_class: Some("dark"),
            personality_css: Some(":root{--c:red}"),
            body_prefix: Some("<nav>nav</nav>"),
            render_mode: RenderMode::Phase2SsrReconcile,
            ir_module: Some(&ir),
            slots: Some(&slots),
        });

        assert!(page.html.contains("<title>Head Test</title>"));
        assert!(page.html.contains("window.__TEST__=true;"));
        assert!(page.html.contains("class=\"dark\""));
        assert!(page.html.contains(":root{--c:red}"));
        assert!(page.html.contains("<nav>nav</nav>"));
        assert!(page.html.contains("<p>hello</p>"));
        assert!(page.html.contains("data-forma-ssr"));
    }

    #[test]
    fn wasm_script_injected_when_manifest_has_wasm_and_route_has_ir() {
        let mut routes = HashMap::new();
        routes.insert("/test".to_string(), RouteAssets {
            js: vec!["test.abc123.js".to_string()],
            css: vec![],
            fonts: vec![],
            total_size_br: 0,
            budget_warn_threshold: 204800,
            ir: Some("test.def456.ir".to_string()),
        });

        let manifest = AssetManifest {
            version: 1,
            build_hash: "test".to_string(),
            assets: HashMap::new(),
            routes,
            wasm: Some(WasmAssets {
                loader: "forma_ir.abc.js".to_string(),
                binary: "forma_ir_bg.def.wasm".to_string(),
            }),
        };

        let page = render_page(&PageConfig {
            title: "WASM Test",
            route_pattern: "/test",
            manifest: &manifest,
            config_script: None,
            body_class: None,
            personality_css: None,
            body_prefix: None,
            render_mode: RenderMode::Phase1ClientMount,
            ir_module: None,
            slots: None,
        });

        assert!(page.html.contains("__FORMA_WASM__"));
        assert!(page.html.contains("/_assets/forma_ir.abc.js"));
        assert!(page.html.contains("/_assets/forma_ir_bg.def.wasm"));
        assert!(page.html.contains("/_assets/test.def456.ir"));
    }

    #[test]
    fn wasm_script_not_injected_when_no_wasm_in_manifest() {
        let manifest = empty_manifest();
        let page = render_page(&PageConfig {
            title: "No WASM",
            route_pattern: "/test",
            manifest: &manifest,
            config_script: None,
            body_class: None,
            personality_css: None,
            body_prefix: None,
            render_mode: RenderMode::Phase1ClientMount,
            ir_module: None,
            slots: None,
        });

        assert!(!page.html.contains("__FORMA_WASM__"));
    }

    #[test]
    fn wasm_script_not_injected_when_route_has_no_ir() {
        let mut routes = HashMap::new();
        routes.insert("/test".to_string(), RouteAssets {
            js: vec!["test.abc123.js".to_string()],
            css: vec![],
            fonts: vec![],
            total_size_br: 0,
            budget_warn_threshold: 204800,
            ir: None, // no IR
        });

        let manifest = AssetManifest {
            version: 1,
            build_hash: "test".to_string(),
            assets: HashMap::new(),
            routes,
            wasm: Some(WasmAssets {
                loader: "forma_ir.abc.js".to_string(),
                binary: "forma_ir_bg.def.wasm".to_string(),
            }),
        };

        let page = render_page(&PageConfig {
            title: "No IR",
            route_pattern: "/test",
            manifest: &manifest,
            config_script: None,
            body_class: None,
            personality_css: None,
            body_prefix: None,
            render_mode: RenderMode::Phase1ClientMount,
            ir_module: None,
            slots: None,
        });

        assert!(!page.html.contains("__FORMA_WASM__"));
    }

    #[test]
    fn phase2_with_slot_data() {
        use forma_ir::parser::test_helpers::{build_minimal_ir, encode_open_tag, encode_close_tag};

        // Build IR with DYN_TEXT referencing slot 0
        let mut opcodes = Vec::new();
        opcodes.extend_from_slice(&encode_open_tag(0, &[])); // <span>
        // DYN_TEXT opcode: 0x05 + slot_id(u16) + marker_id(u16)
        opcodes.push(0x05);
        opcodes.extend_from_slice(&0u16.to_le_bytes()); // slot_id = 0
        opcodes.extend_from_slice(&0u16.to_le_bytes()); // marker_id = 0
        opcodes.extend_from_slice(&encode_close_tag(0)); // </span>

        let data = build_minimal_ir(
            &["span", "name"],           // strings
            &[(0, 1, 0x01, 0x00, &[])], // slot: id=0, name_str_idx=1, type=Text, source=Server
            &opcodes,
            &[],
        );
        let ir = IrModule::parse(&data).unwrap();

        let mut slots = SlotData::new(1);
        slots.set(0, SlotValue::Text("World".to_string()));

        let manifest = empty_manifest();
        let page = render_page(&PageConfig {
            title: "Slot Test",
            route_pattern: "/test",
            manifest: &manifest,
            config_script: None,
            body_class: None,
            personality_css: None,
            body_prefix: None,
            render_mode: RenderMode::Phase2SsrReconcile,
            ir_module: Some(&ir),
            slots: Some(&slots),
        });

        assert!(page.html.contains("data-forma-ssr"));
        // DYN_TEXT wraps content in marker comments for client reconciliation
        assert!(page.html.contains("<span><!--f:t0-->World<!--/f:t0--></span>"));
    }
}
