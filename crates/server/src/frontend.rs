use std::collections::HashMap;

use crate::{assets, ir, AppState, AssetManifest, RenderMode};

#[derive(Debug, Clone)]
pub struct FrontendSnapshot {
    manifest: AssetManifest,
    render_modes: HashMap<String, RenderMode>,
    ir_modules: HashMap<String, ir::parser::IrModule>,
}

impl FrontendSnapshot {
    pub fn capture(state: &AppState) -> Self {
        if assets::use_filesystem_assets() {
            let manifest = assets::load_manifest();
            let (render_modes, ir_modules) = ir::load_ir_modules(&manifest);
            return Self {
                manifest,
                render_modes,
                ir_modules,
            };
        }

        Self {
            manifest: state.asset_manifest.clone(),
            render_modes: state.render_modes.clone(),
            ir_modules: state.ir_modules.clone(),
        }
    }

    pub fn manifest(&self) -> &AssetManifest {
        &self.manifest
    }

    pub fn render_mode(&self, route_pattern: &str) -> RenderMode {
        self.render_modes
            .get(route_pattern)
            .copied()
            .unwrap_or(RenderMode::Phase1ClientMount)
    }

    pub fn ir_module(&self, route_pattern: &str) -> Option<&ir::parser::IrModule> {
        self.ir_modules.get(route_pattern)
    }
}
