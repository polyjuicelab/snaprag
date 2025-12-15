//! Authentication token management handlers

use std::fs;
use std::path::Path;

use rand::Rng;
use tracing::info;
use uuid::Uuid;

use crate::config::AppConfig;
use crate::Result;

/// Generate a new authentication token/secret pair
///
/// # Errors
/// - File I/O errors when reading or writing config file
/// - TOML parsing/serialization errors
pub fn handle_auth_generate(config: &AppConfig, name: Option<String>) -> Result<()> {
    let token_name = name.unwrap_or_else(|| {
        // Generate a default name using UUID
        format!("token_{}", Uuid::new_v4().simple())
    });

    // Generate a random 32-byte secret (256 bits)
    let mut rng = rand::thread_rng();
    let secret_bytes: [u8; 32] = rng.gen();
    let secret = hex::encode(secret_bytes);

    info!("Generated new token: {}", token_name);
    info!("Secret: {}", secret);
    info!("");
    info!("Add this to your config.toml:");
    info!("[auth]");
    info!("enabled = true");
    info!("{} = \"{}\"", token_name, secret);
    info!("");
    info!("⚠️  Keep the secret secure! It cannot be recovered if lost.");

    // Try to update config.toml if it exists
    let config_path = if Path::new("config.toml").exists() {
        "config.toml"
    } else if Path::new("config.example.toml").exists() {
        "config.example.toml"
    } else {
        info!("⚠️  No config file found. Please manually add the token to your config.");
        return Ok(());
    };

    // Read existing config
    let config_content = fs::read_to_string(config_path)?;

    // Parse TOML
    let mut toml_value: toml::Value = toml::from_str(&config_content)?;

    // Get or create auth section
    let auth_section = toml_value
        .as_table_mut()
        .and_then(|t| t.get_mut("auth"))
        .and_then(|v| v.as_table_mut());

    if let Some(auth_table) = auth_section {
        // Add token to existing auth section
        auth_table.insert(token_name.clone(), toml::Value::String(secret.clone()));
        if !auth_table.contains_key("enabled") {
            auth_table.insert("enabled".to_string(), toml::Value::Boolean(true));
        }
    } else {
        // Create new auth section
        let mut auth_table = toml::map::Map::new();
        auth_table.insert("enabled".to_string(), toml::Value::Boolean(true));
        auth_table.insert(token_name.clone(), toml::Value::String(secret.clone()));

        toml_value
            .as_table_mut()
            .unwrap()
            .insert("auth".to_string(), toml::Value::Table(auth_table));
    }

    // Write back to file
    let updated_content = toml::to_string_pretty(&toml_value)?;
    fs::write(config_path, updated_content)?;

    info!("✅ Token added to {}", config_path);

    Ok(())
}

/// List all configured authentication tokens
///
/// # Errors
/// - File I/O errors when reading config file
/// - TOML parsing errors
pub fn handle_auth_list(config: &AppConfig) -> Result<()> {
    if !config.auth.enabled {
        info!("⚠️  Authentication is disabled in configuration.");
        return Ok(());
    }

    if config.auth.tokens.is_empty() {
        info!("No authentication tokens configured.");
        return Ok(());
    }

    info!("Configured authentication tokens:");
    info!("");
    for (name, secret) in &config.auth.tokens {
        // Only show first 8 characters of secret for security
        let secret_preview = if secret.len() > 8 {
            format!("{}...", &secret[..8])
        } else {
            "***".to_string()
        };
        info!("  {}: {}", name, secret_preview);
    }
    info!("");
    info!("Total: {} token(s)", config.auth.tokens.len());

    Ok(())
}

/// Revoke (delete) an authentication token
///
/// # Errors
/// - File I/O errors when reading or writing config file
/// - TOML parsing/serialization errors
/// - Token not found
pub fn handle_auth_revoke(config: &AppConfig, name: String) -> Result<()> {
    // Check if token exists
    if !config.auth.tokens.contains_key(&name) {
        return Err(crate::SnapRagError::Custom(format!(
            "Token '{}' not found in configuration",
            name
        )));
    }

    // Find config file
    let config_path = if Path::new("config.toml").exists() {
        "config.toml"
    } else if Path::new("config.example.toml").exists() {
        "config.example.toml"
    } else {
        return Err(crate::SnapRagError::Custom(
            "No config file found. Cannot revoke token.".to_string(),
        ));
    };

    // Read existing config
    let config_content = fs::read_to_string(config_path)?;

    // Parse TOML
    let mut toml_value: toml::Value = toml::from_str(&config_content)?;

    // Get auth section
    if let Some(auth_table) = toml_value
        .as_table_mut()
        .and_then(|t| t.get_mut("auth"))
        .and_then(|v| v.as_table_mut())
    {
        // Remove token
        if auth_table.remove(&name).is_some() {
            // Write back to file
            let updated_content = toml::to_string_pretty(&toml_value)?;
            fs::write(config_path, updated_content)?;

            info!("✅ Token '{}' revoked from {}", name, config_path);
        } else {
            return Err(crate::SnapRagError::Custom(format!(
                "Token '{}' not found in config file",
                name
            )));
        }
    } else {
        return Err(crate::SnapRagError::Custom(
            "Auth section not found in config file".to_string(),
        ));
    }

    Ok(())
}
