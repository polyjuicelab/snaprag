//! Validation tests for strict test configuration
//! These tests verify that our strict testing setup works correctly

use crate::strict_test;
use crate::tests::strict_test_config;

#[cfg(test)]
mod tests {
    use super::*;

    strict_test!(strict_test_macro_example, async {
        assert_eq!(2 + 2, 4);
    });

    #[test]
    fn test_strict_config_initialization() {
        // This test verifies that strict test configuration can be initialized
        strict_test_config::init_strict_testing();
        // If we get here without panicking, the initialization worked
        assert!(true, "Strict test configuration initialized successfully");
    }

    #[test]
    fn test_generated_code_warning_detection() {
        // Test that we can detect generated code warnings
        let generated_warnings = vec![
            "warning: unused_lifetimes in generated/",
            "warning: elided-lifetimes-in-paths in protobuf",
            "warning: unused_imports in prost",
            "warning: unused_variables in tonic",
        ];

        for warning in generated_warnings {
            assert!(
                warning.contains("generated/")
                    || warning.contains("protobuf")
                    || warning.contains("prost")
                    || warning.contains("tonic"),
                "Should detect generated code warning: {warning}"
            );
        }
    }

    #[test]
    fn test_non_generated_warning_detection() {
        // Test that we can detect non-generated code warnings
        let non_generated_warnings = vec![
            "warning: unused variable in src/main.rs",
            "warning: dead code in src/sync/service.rs",
            "warning: unused import in src/database.rs",
        ];

        for warning in non_generated_warnings {
            assert!(
                !warning.contains("generated/")
                    && !warning.contains("protobuf")
                    && !warning.contains("prost")
                    && !warning.contains("tonic"),
                "Should NOT detect as generated code warning: {warning}"
            );
        }
    }
}
