// All IR types live in the forma-ir crate
pub use forma_ir::format;
pub use forma_ir::parser;
pub use forma_ir::slot;
pub use forma_ir::walker;
pub use forma_ir::dump;

// Re-export IR compatibility and loading functions from forma-server.
// `load_ir_modules` replaces the inline loop in main.rs.
pub use forma_server::ir_compat::{check_ir_compatibility, load_ir_modules};

use forma_ir::parser::IrModule;
use std::collections::HashMap;

/// Validate all IR modules against the current runtime version.
/// Returns a map of route -> render mode decisions.
/// Routes with valid IR get `true` (SSR capable), invalid get `false` (fall back to Phase 1).
///
/// NOTE: This is a GateWASM-specific helper not present in forma-server.
/// It provides a simpler bool-based API used by some test code.
pub fn validate_ir_modules(ir_modules: &HashMap<String, IrModule>) -> HashMap<String, bool> {
    let mut results = HashMap::new();
    for (route, module) in ir_modules {
        match check_ir_compatibility(module) {
            Ok(()) => {
                results.insert(route.clone(), true);
            }
            Err(msg) => {
                tracing::warn!(route = %route, error = %msg, "IR compatibility check failed");
                results.insert(route.clone(), false);
            }
        }
    }
    results
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;
    use forma_ir::format::{IrHeader, IR_VERSION};
    use forma_ir::parser::test_helpers::build_minimal_ir;
    use forma_ir::parser::IrModule;

    /// Parse a valid minimal IR, then override the header version.
    fn make_ir_module_with_version(version: u16) -> IrModule {
        // Build a minimal valid IR binary with a TEXT opcode referencing string 0.
        let opcodes = forma_ir::parser::test_helpers::encode_text(0);
        let data = build_minimal_ir(&["hello"], &[], &opcodes, &[]);
        let mut module = IrModule::parse(&data).unwrap();
        module.header = IrHeader {
            version,
            flags: 0,
            source_hash: 0,
        };
        module
    }

    #[test]
    fn compatibility_passes_for_current_version() {
        let module = make_ir_module_with_version(IR_VERSION);
        assert!(check_ir_compatibility(&module).is_ok());
    }

    #[test]
    fn compatibility_fails_for_future_version() {
        let module = make_ir_module_with_version(99);
        let err = check_ir_compatibility(&module).unwrap_err();
        assert!(err.contains("99"));
        assert!(err.contains(&IR_VERSION.to_string()));
    }

    #[test]
    fn compatibility_fails_for_version_zero() {
        let module = make_ir_module_with_version(0);
        let err = check_ir_compatibility(&module).unwrap_err();
        assert!(err.contains("0"));
        assert!(err.contains(&IR_VERSION.to_string()));
    }

    #[test]
    fn validate_ir_modules_mixed() {
        let valid_module = make_ir_module_with_version(IR_VERSION);
        let invalid_module = make_ir_module_with_version(99);

        let mut ir_modules = HashMap::new();
        ir_modules.insert("/valid-route".to_string(), valid_module);
        ir_modules.insert("/invalid-route".to_string(), invalid_module);

        let results = validate_ir_modules(&ir_modules);

        assert_eq!(results.len(), 2);
        assert_eq!(results["/valid-route"], true);
        assert_eq!(results["/invalid-route"], false);
    }

    #[test]
    fn validate_ir_modules_empty() {
        let ir_modules: HashMap<String, IrModule> = HashMap::new();
        let results = validate_ir_modules(&ir_modules);
        assert!(results.is_empty());
    }
}
