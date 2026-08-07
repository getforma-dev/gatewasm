// All IR types live in the forma-ir crate
pub use forma_ir::format;
pub use forma_ir::parser;
pub use forma_ir::slot;
pub use forma_ir::walker;
pub use forma_ir::dump;

use forma_ir::parser::IrModule;
use std::collections::HashMap;

use crate::{assets, AssetManifest, RenderMode};

/// Load and parse all IR modules from the active asset source.
///
/// In development we read `.ir` files from `admin/dist` on the filesystem so
/// the SSR walker, hydration manifest, and emitted JS/CSS stay in sync.
/// In production the same helper reads from embedded assets through
/// `crate::assets::asset_bytes`, preserving single-binary delivery. This is
/// why we do not call `forma_server::ir_compat::load_ir_modules`: that reads
/// embedded assets only, and would break the dev filesystem path.
///
/// Version compatibility is no longer checked separately — since forma-ir
/// 0.2.0 `IrModule::parse` rejects an incompatible header itself, so a
/// successful parse *is* the compatibility guarantee.
pub fn load_ir_modules(
    manifest: &AssetManifest,
) -> (HashMap<String, RenderMode>, HashMap<String, IrModule>) {
    let mut render_modes = HashMap::new();
    let mut ir_modules = HashMap::new();

    for (route_pattern, route_assets) in &manifest.routes {
        let Some(ref ir_filename) = route_assets.ir else {
            continue;
        };
        let source = if assets::use_filesystem_assets() {
            "filesystem"
        } else {
            "embedded"
        };

        let Some(ir_bytes) = assets::asset_bytes(ir_filename) else {
            // The manifest named an .ir file that is not present. Previously
            // this fell through with no log and no render_modes entry, which
            // was indistinguishable from a route that legitimately has no IR.
            tracing::warn!(
                route = %route_pattern,
                ir_file = %ir_filename,
                source,
                "Manifest declares an IR file that is not available — Phase 1 fallback"
            );
            render_modes.insert(route_pattern.clone(), RenderMode::Phase1ClientMount);
            continue;
        };

        match IrModule::parse(&ir_bytes) {
            Ok(module) => {
                tracing::info!(
                    route = %route_pattern,
                    ir_file = %ir_filename,
                    source,
                    "Loaded IR module for SSR"
                );
                render_modes.insert(route_pattern.clone(), RenderMode::Phase2SsrReconcile);
                ir_modules.insert(route_pattern.clone(), module);
            }
            Err(e) => {
                tracing::warn!(
                    route = %route_pattern,
                    error = %e,
                    "Failed to parse IR module — Phase 1 fallback"
                );
                render_modes.insert(route_pattern.clone(), RenderMode::Phase1ClientMount);
            }
        }
    }

    if !ir_modules.is_empty() {
        tracing::info!(
            ssr_routes = ir_modules.len(),
            "Phase 2 SSR enabled for {} route(s)",
            ir_modules.len()
        );
    }

    (render_modes, ir_modules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use forma_ir::parser::test_helpers::{build_minimal_ir, encode_text};

    #[test]
    fn parse_accepts_a_current_version_module() {
        let data = build_minimal_ir(&["hello"], &[], &encode_text(0), &[]);
        assert!(IrModule::parse(&data).is_ok());
    }

    #[test]
    fn parse_rejects_an_incompatible_header_version() {
        // The version lives in the header, immediately after the 4-byte magic.
        // Corrupting it is the only way to exercise this now that the check is
        // inside `parse` rather than a separate pass over an already-parsed
        // module. `validate_ir_modules` and `check_ir_compatibility` went away
        // with that change; both were only ever reachable from these tests.
        let mut data = build_minimal_ir(&["hello"], &[], &encode_text(0), &[]);
        data[4] = 0xFF;
        data[5] = 0xFF;
        assert!(IrModule::parse(&data).is_err());
    }
}
