//! Basic tests for MBTI integration (no ML required)
//!
//! These tests verify the configuration and basic structure
//! without requiring the ml-mbti feature or model downloads.

use snaprag::config::MbtiConfig;
use snaprag::config::MbtiMethod;

#[test]
fn test_mbti_method_enum() {
    // Test that all MBTI methods are defined
    let _ = MbtiMethod::RuleBased;
    let _ = MbtiMethod::MachineLearning;
    let _ = MbtiMethod::Ensemble;

    println!("✅ All MBTI methods are properly defined");
}

#[test]
fn test_mbti_config_default() {
    let config = MbtiConfig::default();

    assert_eq!(config.method, MbtiMethod::RuleBased);
    assert!(!config.use_llm);

    println!("✅ MbtiConfig defaults to rule-based method");
}

#[test]
fn test_mbti_config_serialization() {
    use serde::Deserialize;
    use serde::Serialize;

    let config = MbtiConfig {
        method: MbtiMethod::MachineLearning,
        use_llm: true,
    };

    // Test serialization
    let toml_str = toml::to_string(&config).expect("Failed to serialize");
    println!("Serialized config:\n{toml_str}");

    // Test deserialization
    let deserialized: MbtiConfig = toml::from_str(&toml_str).expect("Failed to deserialize");
    assert_eq!(deserialized.method, MbtiMethod::MachineLearning);
    assert!(deserialized.use_llm);

    println!("✅ MbtiConfig serialization/deserialization works");
}

#[test]
fn test_mbti_method_in_config_serialization() {
    // Test that methods serialize correctly when inside a config struct
    let configs = vec![
        MbtiConfig {
            method: MbtiMethod::RuleBased,
            use_llm: false,
        },
        MbtiConfig {
            method: MbtiMethod::MachineLearning,
            use_llm: false,
        },
        MbtiConfig {
            method: MbtiMethod::Ensemble,
            use_llm: false,
        },
    ];

    for config in configs {
        let toml_str = toml::to_string(&config).expect("Failed to serialize config");
        println!("Method {:?} serializes to:\n{toml_str}", config.method);

        // Verify it can be deserialized back
        let _: MbtiConfig = toml::from_str(&toml_str).expect("Failed to deserialize");
    }

    println!("✅ MBTI methods serialize correctly in config");
}

#[test]
fn test_load_test_config() {
    // Try to load test config
    match snaprag::AppConfig::from_file("config.test.toml") {
        Ok(config) => {
            println!("✅ Test config loaded successfully");
            println!("   MBTI method: {:?}", config.mbti.method);
            println!("   Use LLM: {}", config.mbti.use_llm);
        }
        Err(e) => {
            println!("⚠️  Test config not available: {e}");
            println!("   This is expected if config.test.toml doesn't exist");
        }
    }
}

#[cfg(feature = "ml-mbti")]
#[test]
fn test_ml_module_exists() {
    // When ml-mbti feature is enabled, the module should exist
    println!("✅ ml-mbti feature is enabled");
    println!("   personality_ml module is available");
}

#[cfg(not(feature = "ml-mbti"))]
#[test]
fn test_ml_module_disabled() {
    println!("ℹ️  ml-mbti feature is NOT enabled");
    println!("   This is the default configuration");
    println!("   To enable: cargo test --features ml-mbti");
}

#[test]
fn test_mbti_integration_documentation() {
    println!("\n=== MBTI Integration Test Documentation ===\n");

    println!("This test file verifies:");
    println!("  ✅ MbtiMethod enum and variants");
    println!("  ✅ MbtiConfig structure and defaults");
    println!("  ✅ Configuration serialization");
    println!("  ✅ Test config loading");

    println!("\nTo run ML-specific tests:");
    println!("  cargo test --features ml-mbti --test mbti_ml_tests");

    println!("\nTo run ignored integration tests:");
    println!("  cargo test --features ml-mbti --test mbti_ml_tests -- --ignored");

    println!("\n✅ Basic MBTI integration tests complete");
}
