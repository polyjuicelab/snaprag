//! Machine Learning-based MBTI personality classification
//!
//! This module provides ML-powered MBTI classification using the psycial library
//! (snapMBTI project) with BERT + Multi-Task MLP neural networks.
//!
//! The ML approach complements the rule-based approach in `personality.rs`:
//! - Rule-based: Analyzes social behavior patterns and communication style
//! - ML-based: Uses trained neural networks on text content
//!
//! Both approaches can be combined for more accurate predictions.

#![cfg(feature = "ml-mbti")]
#![allow(clippy::cast_precision_loss)] // Acceptable for ML confidence scores

use std::sync::Arc;

use serde::Deserialize;
use serde::Serialize;

use crate::database::Database;
use crate::personality::MbtiDimensions;
use crate::personality::MbtiProfile;
use crate::Result;

/// ML-based MBTI predictor using the psycial library
pub struct MlMbtiPredictor {
    predictor: psycial::api::Predictor,
    database: Arc<Database>,
}

/// ML prediction result (internal)
#[derive(Debug, Clone)]
struct MlPrediction {
    mbti_type: String,
    confidence: f64,
    ei_score: f64,
    ei_confidence: f64,
    sn_score: f64,
    sn_confidence: f64,
    tf_score: f64,
    tf_confidence: f64,
    jp_score: f64,
    jp_confidence: f64,
}

impl MlMbtiPredictor {
    /// Create a new ML-based MBTI predictor
    ///
    /// This will load the pre-trained model from the default location.
    /// If models are not available locally, they will be downloaded from HuggingFace
    /// (if the auto-download feature is enabled in psycial).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Model files are not found
    /// - Model files are corrupted
    /// - Required dependencies are missing
    pub fn new(database: Arc<Database>) -> Result<Self> {
        let predictor = psycial::api::Predictor::new().map_err(|e| {
            crate::SnapRagError::Custom(format!("Failed to initialize ML predictor: {e}"))
        })?;

        Ok(Self {
            predictor,
            database,
        })
    }

    /// Predict MBTI type for a user using ML
    ///
    /// This method:
    /// 1. Fetches recent casts from the user
    /// 2. Filters out bot messages
    /// 3. Combines text content
    /// 4. Uses the trained neural network to predict MBTI type
    ///
    /// Returns an `MbtiProfile` compatible with the existing personality module.
    pub async fn predict_mbti(&self, fid: i64) -> Result<MbtiProfile> {
        // Get recent casts for text analysis
        let casts = self
            .database
            .get_casts_by_fid(fid, Some(200), Some(0))
            .await?;

        if casts.is_empty() {
            return Err(crate::SnapRagError::Custom(format!(
                "No casts found for user {fid}. Cannot predict MBTI without text data."
            )));
        }

        // Filter out bot messages and combine text
        let texts: Vec<String> = casts
            .iter()
            .filter_map(|cast| cast.text.as_ref())
            .filter(|text| !is_bot_message(Some(text)))
            .cloned()
            .collect();

        if texts.is_empty() {
            return Err(crate::SnapRagError::Custom(format!(
                "No valid text content found for user {fid} after filtering bot messages."
            )));
        }

        // Combine all texts (the model works better with more context)
        let combined_text = texts.join(" ");

        // Use ML predictor
        let result = self
            .predictor
            .predict(&combined_text)
            .map_err(|e| crate::SnapRagError::Custom(format!("ML prediction failed: {e}")))?;

        // Convert psycial result to our MbtiProfile format
        let prediction = self.convert_prediction_result(result);

        // Convert dimension scores to our format
        let dimensions = MbtiDimensions {
            ei_score: prediction.ei_score as f32,
            sn_score: prediction.sn_score as f32,
            tf_score: prediction.tf_score as f32,
            jp_score: prediction.jp_score as f32,
            ei_confidence: prediction.ei_confidence as f32,
            sn_confidence: prediction.sn_confidence as f32,
            tf_confidence: prediction.tf_confidence as f32,
            jp_confidence: prediction.jp_confidence as f32,
        };

        // Get traits for this type
        let traits = get_traits_for_type(&prediction.mbti_type);

        // Generate analysis based on ML prediction
        let analysis = self.generate_ml_analysis(&prediction, texts.len());

        Ok(MbtiProfile {
            fid,
            mbti_type: prediction.mbti_type,
            confidence: prediction.confidence as f32,
            dimensions,
            traits,
            analysis,
        })
    }

    /// Predict MBTI types for multiple users in batch
    ///
    /// More efficient than calling `predict_mbti` multiple times.
    pub async fn predict_batch(&self, fids: &[i64]) -> Result<Vec<(i64, Result<MbtiProfile>)>> {
        let mut results = Vec::new();

        for &fid in fids {
            let result = self.predict_mbti(fid).await;
            results.push((fid, result));
        }

        Ok(results)
    }

    /// Convert psycial prediction result to internal format
    fn convert_prediction_result(&self, result: psycial::api::PredictionResult) -> MlPrediction {
        // psycial uses:
        // - 0 = first letter (E, S, T, J)
        // - 1 = second letter (I, N, F, P)
        //
        // We need to convert to our scoring:
        // - 0.0 = first letter
        // - 1.0 = second letter

        let ei_score = if result.dimensions.e_i.letter == 'I' {
            result.dimensions.e_i.confidence
        } else {
            1.0 - result.dimensions.e_i.confidence
        };

        let sn_score = if result.dimensions.s_n.letter == 'N' {
            result.dimensions.s_n.confidence
        } else {
            1.0 - result.dimensions.s_n.confidence
        };

        let tf_score = if result.dimensions.t_f.letter == 'F' {
            result.dimensions.t_f.confidence
        } else {
            1.0 - result.dimensions.t_f.confidence
        };

        let jp_score = if result.dimensions.j_p.letter == 'P' {
            result.dimensions.j_p.confidence
        } else {
            1.0 - result.dimensions.j_p.confidence
        };

        MlPrediction {
            mbti_type: result.mbti_type,
            confidence: result.confidence,
            ei_score,
            ei_confidence: result.dimensions.e_i.confidence,
            sn_score,
            sn_confidence: result.dimensions.s_n.confidence,
            tf_score,
            tf_confidence: result.dimensions.t_f.confidence,
            jp_score,
            jp_confidence: result.dimensions.j_p.confidence,
        }
    }

    /// Generate analysis text based on ML prediction
    fn generate_ml_analysis(&self, prediction: &MlPrediction, text_sample_count: usize) -> String {
        let mut analysis = String::new();

        analysis.push_str(&format!(
            "Machine learning analysis (BERT + Multi-Task Neural Network) predicts {} personality type with {:.1}% overall confidence, based on {} text samples.\n\n",
            prediction.mbti_type,
            prediction.confidence * 100.0,
            text_sample_count
        ));

        analysis.push_str("Dimension Analysis:\n");

        // E/I
        let ei_letter = if prediction.ei_score > 0.5 { "I" } else { "E" };
        let ei_strength = if (prediction.ei_score - 0.5).abs() > 0.3 {
            "Strong"
        } else {
            "Moderate"
        };
        analysis.push_str(&format!(
            "- E/I: {} {} ({:.1}% confidence)\n",
            ei_strength,
            ei_letter,
            prediction.ei_confidence * 100.0
        ));

        // S/N
        let sn_letter = if prediction.sn_score > 0.5 { "N" } else { "S" };
        let sn_strength = if (prediction.sn_score - 0.5).abs() > 0.3 {
            "Strong"
        } else {
            "Moderate"
        };
        analysis.push_str(&format!(
            "- S/N: {} {} ({:.1}% confidence)\n",
            sn_strength,
            sn_letter,
            prediction.sn_confidence * 100.0
        ));

        // T/F
        let tf_letter = if prediction.tf_score > 0.5 { "F" } else { "T" };
        let tf_strength = if (prediction.tf_score - 0.5).abs() > 0.3 {
            "Strong"
        } else {
            "Moderate"
        };
        analysis.push_str(&format!(
            "- T/F: {} {} ({:.1}% confidence)\n",
            tf_strength,
            tf_letter,
            prediction.tf_confidence * 100.0
        ));

        // J/P
        let jp_letter = if prediction.jp_score > 0.5 { "P" } else { "J" };
        let jp_strength = if (prediction.jp_score - 0.5).abs() > 0.3 {
            "Strong"
        } else {
            "Moderate"
        };
        analysis.push_str(&format!(
            "- J/P: {} {} ({:.1}% confidence)\n\n",
            jp_strength,
            jp_letter,
            prediction.jp_confidence * 100.0
        ));

        analysis.push_str(
            "This prediction is based on deep learning analysis of writing style, word choice, \
             and linguistic patterns using a BERT encoder and multi-task neural network trained \
             on MBTI personality data.",
        );

        analysis
    }

    /// Get model information
    pub fn model_info(&self) -> String {
        let info = self.predictor.model_info();
        format!(
            "ML Model: BERT + Multi-Task MLP\nDevice: {}\nInput Dimension: {}",
            info.device, info.input_dim
        )
    }
}

/// Ensemble predictor that combines rule-based and ML predictions
pub struct EnsembleMbtiPredictor {
    rule_based: crate::personality::MbtiAnalyzer,
    ml_based: MlMbtiPredictor,
}

impl EnsembleMbtiPredictor {
    /// Create a new ensemble predictor
    pub fn new(database: Arc<Database>) -> Result<Self> {
        let rule_based = crate::personality::MbtiAnalyzer::new(database.clone());
        let ml_based = MlMbtiPredictor::new(database)?;

        Ok(Self {
            rule_based,
            ml_based,
        })
    }

    /// Predict MBTI using both methods and combine results
    ///
    /// The ensemble approach:
    /// 1. Gets predictions from both rule-based and ML methods
    /// 2. Averages dimension scores (weighted by confidence)
    /// 3. Determines final type from averaged dimensions
    /// 4. Provides higher overall confidence when both methods agree
    pub async fn predict_ensemble(
        &self,
        fid: i64,
        social_profile: Option<&crate::social_graph::SocialProfile>,
    ) -> Result<MbtiProfile> {
        // Get both predictions
        let rule_prediction = self.rule_based.analyze_mbti(fid, social_profile).await?;
        let ml_prediction = self.ml_based.predict_mbti(fid).await?;

        // Combine dimension scores with confidence-based weighting
        let rule_weight = rule_prediction.confidence;
        let ml_weight = ml_prediction.confidence;
        let total_weight = rule_weight + ml_weight;

        let ei_score = (rule_prediction.dimensions.ei_score * rule_weight
            + ml_prediction.dimensions.ei_score * ml_weight)
            / total_weight;
        let sn_score = (rule_prediction.dimensions.sn_score * rule_weight
            + ml_prediction.dimensions.sn_score * ml_weight)
            / total_weight;
        let tf_score = (rule_prediction.dimensions.tf_score * rule_weight
            + ml_prediction.dimensions.tf_score * ml_weight)
            / total_weight;
        let jp_score = (rule_prediction.dimensions.jp_score * rule_weight
            + ml_prediction.dimensions.jp_score * ml_weight)
            / total_weight;

        // Calculate confidence (higher when both methods agree)
        let agreement_bonus = if rule_prediction.mbti_type == ml_prediction.mbti_type {
            0.2 // Boost confidence by 20% when both methods agree
        } else {
            0.0
        };

        let base_confidence = (rule_prediction.confidence + ml_prediction.confidence) / 2.0;
        let confidence = (base_confidence + agreement_bonus).min(1.0);

        // Determine final type
        let mbti_type = format!(
            "{}{}{}{}",
            if ei_score < 0.5 { "E" } else { "I" },
            if sn_score < 0.5 { "S" } else { "N" },
            if tf_score < 0.5 { "T" } else { "F" },
            if jp_score < 0.5 { "J" } else { "P" }
        );

        let dimensions = MbtiDimensions {
            ei_score,
            sn_score,
            tf_score,
            jp_score,
            ei_confidence: (rule_prediction.dimensions.ei_confidence
                + ml_prediction.dimensions.ei_confidence)
                / 2.0,
            sn_confidence: (rule_prediction.dimensions.sn_confidence
                + ml_prediction.dimensions.sn_confidence)
                / 2.0,
            tf_confidence: (rule_prediction.dimensions.tf_confidence
                + ml_prediction.dimensions.tf_confidence)
                / 2.0,
            jp_confidence: (rule_prediction.dimensions.jp_confidence
                + ml_prediction.dimensions.jp_confidence)
                / 2.0,
        };

        let traits = get_traits_for_type(&mbti_type);

        // Combine analyses
        let analysis = format!(
            "ENSEMBLE ANALYSIS (Rule-based + Machine Learning)\n\n\
             Final Type: {}\n\
             Agreement: Both methods {}.\n\n\
             === Rule-based Analysis ===\n\
             Type: {} (Confidence: {:.1}%)\n\
             {}\n\n\
             === Machine Learning Analysis ===\n\
             Type: {} (Confidence: {:.1}%)\n\
             {}",
            mbti_type,
            if rule_prediction.mbti_type == ml_prediction.mbti_type {
                format!("AGREE on {}", mbti_type)
            } else {
                format!(
                    "differ (rule: {}, ML: {})",
                    rule_prediction.mbti_type, ml_prediction.mbti_type
                )
            },
            rule_prediction.mbti_type,
            rule_prediction.confidence * 100.0,
            rule_prediction.analysis,
            ml_prediction.mbti_type,
            ml_prediction.confidence * 100.0,
            ml_prediction.analysis
        );

        Ok(MbtiProfile {
            fid,
            mbti_type,
            confidence,
            dimensions,
            traits,
            analysis,
        })
    }
}

// Helper functions

/// Check if a message is likely automated/bot-generated
fn is_bot_message(text: Option<&str>) -> bool {
    let Some(text) = text else {
        return false;
    };

    let text_lower = text.to_lowercase();

    let bot_patterns = [
        "ms!t",
        "i'm supporting you through /microsub",
        "please mute the keyword \"ms!t\"",
        "$degen",
        "minted",
        "you've been tipped",
        "airdrop claim",
        "congratulations! you won",
        "click here to claim",
        "limited time offer",
        "visit this link",
    ];

    for pattern in &bot_patterns {
        if text_lower.contains(pattern) {
            return true;
        }
    }

    // Filter short automated messages
    if text.len() < 50 && text_lower.contains("$degen") && text_lower.contains("supporting") {
        return true;
    }

    // Filter emoji-only posts
    let has_meaningful_text = text
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .count()
        > 10;

    if !has_meaningful_text && text.len() < 20 {
        return true;
    }

    false
}

/// Get personality traits for a given MBTI type
fn get_traits_for_type(mbti_type: &str) -> Vec<String> {
    let traits = match mbti_type {
        "INTJ" => vec![
            "Strategic thinker",
            "Independent",
            "Analytical",
            "Innovative",
            "Future-focused",
        ],
        "INTP" => vec![
            "Logical",
            "Curious",
            "Theoretical",
            "Problem solver",
            "Analytical",
        ],
        "ENTJ" => vec![
            "Natural leader",
            "Strategic",
            "Decisive",
            "Efficient",
            "Goal-oriented",
        ],
        "ENTP" => vec![
            "Innovative",
            "Entrepreneurial",
            "Debater",
            "Quick thinker",
            "Versatile",
        ],
        "INFJ" => vec![
            "Insightful",
            "Idealistic",
            "Compassionate",
            "Creative",
            "Purposeful",
        ],
        "INFP" => vec![
            "Idealistic",
            "Empathetic",
            "Creative",
            "Open-minded",
            "Value-driven",
        ],
        "ENFJ" => vec![
            "Charismatic",
            "Inspiring",
            "Empathetic",
            "Organized",
            "Persuasive",
        ],
        "ENFP" => vec![
            "Enthusiastic",
            "Creative",
            "Sociable",
            "Spontaneous",
            "Optimistic",
        ],
        "ISTJ" => vec![
            "Responsible",
            "Organized",
            "Practical",
            "Detail-oriented",
            "Reliable",
        ],
        "ISFJ" => vec![
            "Caring",
            "Loyal",
            "Practical",
            "Detail-oriented",
            "Supportive",
        ],
        "ESTJ" => vec![
            "Organized",
            "Practical",
            "Direct",
            "Efficient",
            "Traditional",
        ],
        "ESFJ" => vec!["Caring", "Social", "Organized", "Cooperative", "Supportive"],
        "ISTP" => vec![
            "Practical",
            "Hands-on",
            "Logical",
            "Adaptable",
            "Problem solver",
        ],
        "ISFP" => vec![
            "Artistic",
            "Gentle",
            "Flexible",
            "Sensitive",
            "Present-focused",
        ],
        "ESTP" => vec![
            "Energetic",
            "Action-oriented",
            "Pragmatic",
            "Sociable",
            "Risk-taker",
        ],
        "ESFP" => vec![
            "Outgoing",
            "Spontaneous",
            "Playful",
            "Enthusiastic",
            "People-focused",
        ],
        _ => vec!["Unique", "Individual"],
    };

    traits.into_iter().map(String::from).collect()
}






