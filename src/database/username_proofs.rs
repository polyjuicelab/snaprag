use sha2::Digest;
use sha2::Sha256;

use super::Database;
use crate::models::UsernameProof;
use crate::models::UsernameType;
use crate::Result;

/// Generate a unique message_hash for recovered username_proofs
/// Uses a hash of (fid, username_type, username) to ensure uniqueness
fn generate_recovery_message_hash(fid: i64, username_type: i16, username: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"RECOVERED_USERNAME_PROOF");
    hasher.update(fid.to_be_bytes());
    hasher.update(username_type.to_be_bytes());
    hasher.update(username.as_bytes());
    hasher.finalize().to_vec()
}

impl Database {
    /// Create or update username proof
    pub async fn upsert_username_proof(
        &self,
        fid: i64,
        username: String,
        username_type: UsernameType,
        owner: Vec<u8>,
        signature: Vec<u8>,
        timestamp: i64,
    ) -> Result<UsernameProof> {
        // Generate unique message_hash before moving username
        let message_hash = generate_recovery_message_hash(fid, username_type as i16, &username);

        let proof = sqlx::query_as::<_, UsernameProof>(
            r"
            INSERT INTO username_proofs (fid, username, username_type, owner, signature, timestamp, message_hash)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (fid, username_type)
            DO UPDATE SET
                username = EXCLUDED.username,
                owner = EXCLUDED.owner,
                signature = EXCLUDED.signature,
                timestamp = EXCLUDED.timestamp,
                created_at = NOW()
            RETURNING *
            "
        )
        .bind(fid)
        .bind(username)
        .bind(username_type as i16)
        .bind(owner)
        .bind(signature)
        .bind(timestamp)
        .bind(message_hash)
        .fetch_one(&self.pool)
        .await?;

        Ok(proof)
    }

    /// Get username proof by FID and type
    pub async fn get_username_proof(
        &self,
        fid: i64,
        username_type: UsernameType,
    ) -> Result<Option<UsernameProof>> {
        let proof = sqlx::query_as::<_, UsernameProof>(
            "SELECT * FROM username_proofs WHERE fid = $1 AND username_type = $2",
        )
        .bind(fid)
        .bind(username_type as i32)
        .fetch_optional(&self.pool)
        .await?;

        Ok(proof)
    }

    /// Get all username proofs for a user
    pub async fn get_user_username_proofs(&self, fid: i64) -> Result<Vec<UsernameProof>> {
        let proofs = sqlx::query_as::<_, UsernameProof>(
            "SELECT * FROM username_proofs WHERE fid = $1 ORDER BY timestamp DESC",
        )
        .bind(fid)
        .fetch_all(&self.pool)
        .await?;

        Ok(proofs)
    }
}
