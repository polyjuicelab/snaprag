use super::Database;
use crate::Result;

impl Database {
    /// Get FID by Ethereum address from verifications table
    /// Returns the most recent active verification (event_type='add')
    ///
    /// # Errors
    /// - Database query errors (connection failures, SQL execution errors)
    /// - Invalid address format
    pub async fn get_fid_by_ethereum_address(&self, address: &str) -> Result<Option<i64>> {
        // Decode hex address to bytes
        let address_bytes = hex::decode(address.trim_start_matches("0x").trim_start_matches("0X"))
            .map_err(|e| {
                crate::SnapRagError::Custom(format!("Invalid Ethereum address format: {e}"))
            })?;

        // Query for the most recent active verification
        // Get the latest verification for this address (only active ones)
        let result: Option<(i64,)> = sqlx::query_as(
            r"
            SELECT fid
            FROM verifications
            WHERE address = $1 AND event_type = 'add'
            ORDER BY timestamp DESC
            LIMIT 1
            ",
        )
        .bind(&address_bytes)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(|(fid,)| fid))
    }

    /// Get FID by Solana address from verifications table
    /// Returns the most recent active verification (event_type='add')
    ///
    /// # Errors
    /// - Database query errors (connection failures, SQL execution errors)
    /// - Invalid address format
    pub async fn get_fid_by_solana_address(&self, address: &str) -> Result<Option<i64>> {
        // Solana addresses are stored as base58 string bytes (as_bytes().to_vec())
        let address_bytes = address.as_bytes().to_vec();

        // Query for the most recent active verification
        let result: Option<(i64,)> = sqlx::query_as(
            r"
            SELECT fid
            FROM verifications
            WHERE address = $1 AND event_type = 'add'
            ORDER BY timestamp DESC
            LIMIT 1
            ",
        )
        .bind(&address_bytes)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(|(fid,)| fid))
    }

    /// Get FID by address (auto-detect Ethereum or Solana)
    /// Tries Ethereum hex format first, then falls back to Solana
    ///
    /// # Errors
    /// - Database query errors (connection failures, SQL execution errors)
    /// - Invalid address format
    pub async fn get_fid_by_address(&self, address: &str) -> Result<Option<i64>> {
        let trimmed = address.trim();

        // Try Ethereum address first (hex format, with or without 0x prefix)
        if trimmed.starts_with("0x") || trimmed.starts_with("0X") || trimmed.len() == 42 {
            if let Ok(fid) = self.get_fid_by_ethereum_address(trimmed).await {
                return Ok(fid);
            }
        }

        // Fallback to Solana address (base58, typically 32-44 characters)
        if trimmed.len() >= 32 && trimmed.len() <= 44 {
            if let Ok(fid) = self.get_fid_by_solana_address(trimmed).await {
                return Ok(fid);
            }
        }

        // Try Ethereum without 0x prefix
        if trimmed.len() == 40 {
            if let Ok(fid) = self.get_fid_by_ethereum_address(trimmed).await {
                return Ok(fid);
            }
        }

        // Last attempt: try as Solana
        self.get_fid_by_solana_address(trimmed).await
    }

    /// Get all FIDs associated with an address (including removed verifications)
    /// Useful for finding all historical associations
    ///
    /// # Errors
    /// - Database query errors (connection failures, SQL execution errors)
    pub async fn get_all_fids_by_address(&self, address: &str) -> Result<Vec<i64>> {
        let trimmed = address.trim();

        // Try to decode as Ethereum address
        let address_bytes = if trimmed.starts_with("0x")
            || trimmed.starts_with("0X")
            || trimmed.len() == 42
            || trimmed.len() == 40
        {
            hex::decode(trimmed.trim_start_matches("0x").trim_start_matches("0X"))
                .unwrap_or_else(|_| trimmed.as_bytes().to_vec())
        } else {
            trimmed.as_bytes().to_vec()
        };

        let results: Vec<(i64,)> = sqlx::query_as(
            r"
            SELECT DISTINCT fid
            FROM verifications
            WHERE address = $1
            ORDER BY fid
            ",
        )
        .bind(&address_bytes)
        .fetch_all(&self.pool)
        .await?;

        Ok(results.into_iter().map(|(fid,)| fid).collect())
    }
}
