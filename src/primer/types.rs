//! @acp:module "Primer Types"
//! @acp:summary "Type definitions matching primer.schema.json"
//! @acp:domain cli
//! @acp:layer types

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Main primer configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimerConfig {
    pub version: String,

    #[serde(default)]
    pub extends: Option<String>,

    #[serde(default)]
    pub metadata: Option<PrimerMetadata>,

    #[serde(default)]
    pub capabilities: HashMap<String, Capability>,

    #[serde(default)]
    pub categories: Vec<Category>,

    pub sections: Vec<Section>,

    #[serde(default)]
    pub additional_sections: Vec<Section>,

    #[serde(default)]
    pub disabled_sections: Vec<String>,

    #[serde(default)]
    pub section_overrides: HashMap<String, SectionOverride>,

    #[serde(default)]
    pub selection_strategy: SelectionStrategy,

    #[serde(default)]
    pub output_formats: Option<OutputFormatsConfig>,

    #[serde(default)]
    pub knowledge_store: Option<KnowledgeStoreConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimerMetadata {
    pub name: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub license: Option<String>,
    pub min_acp_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub detect_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub priority: Option<u32>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub budget_constraints: Option<BudgetConstraints>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetConstraints {
    #[serde(default)]
    pub minimum: Option<u32>,
    #[serde(default)]
    pub maximum: Option<u32>,
    #[serde(default)]
    pub minimum_percent: Option<f32>,
    #[serde(default)]
    pub maximum_percent: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub id: String,

    pub category: String,

    pub tokens: TokenCount,

    pub value: SectionValue,

    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub description: Option<String>,

    #[serde(default)]
    pub priority: Option<u32>,

    #[serde(default)]
    pub required: bool,

    #[serde(default)]
    pub required_if: Option<String>,

    #[serde(default)]
    pub capabilities: Vec<String>,

    #[serde(default)]
    pub capabilities_all: Vec<String>,

    #[serde(default)]
    pub depends_on: Vec<String>,

    #[serde(default)]
    pub conflicts_with: Vec<String>,

    #[serde(default)]
    pub replaces: Vec<String>,

    #[serde(default)]
    pub tags: Vec<String>,

    pub formats: SectionFormats,

    #[serde(default)]
    pub data: Option<SectionData>,
}

/// Token count - either fixed or dynamic
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TokenCount {
    Fixed(u32),
    Dynamic(String), // "dynamic"
}

impl TokenCount {
    pub fn estimate(&self) -> u32 {
        match self {
            TokenCount::Fixed(n) => *n,
            TokenCount::Dynamic(_) => 50, // Default estimate for dynamic sections
        }
    }
}

/// Multi-dimensional value scoring
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SectionValue {
    #[serde(default)]
    pub safety: u8,

    #[serde(default)]
    pub efficiency: u8,

    #[serde(default)]
    pub accuracy: u8,

    #[serde(default = "default_base")]
    pub base: u8,

    #[serde(default)]
    pub modifiers: Vec<ValueModifier>,
}

fn default_base() -> u8 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueModifier {
    pub condition: String,

    #[serde(default)]
    pub add: Option<i32>,

    #[serde(default)]
    pub multiply: Option<f64>,

    #[serde(default)]
    pub set: Option<i32>,

    #[serde(default)]
    pub dimension: Option<String>,

    #[serde(default)]
    pub reason: Option<String>,
}

/// Section override for project customization
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SectionOverride {
    #[serde(default)]
    pub value: Option<SectionValue>,

    #[serde(default)]
    pub tokens: Option<u32>,

    #[serde(default)]
    pub required: Option<bool>,

    #[serde(default)]
    pub required_if: Option<String>,

    #[serde(default)]
    pub formats: Option<SectionFormats>,
}

/// Output format templates per section
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SectionFormats {
    #[serde(default)]
    pub markdown: Option<FormatTemplate>,

    #[serde(default)]
    pub compact: Option<FormatTemplate>,

    #[serde(default)]
    pub json: Option<serde_json::Value>, // Can be null

    #[serde(default)]
    pub text: Option<FormatTemplate>,
}

impl SectionFormats {
    /// Get template for format with fallback to markdown
    pub fn get(&self, format: super::renderer::OutputFormat) -> Option<&FormatTemplate> {
        match format {
            super::renderer::OutputFormat::Markdown => self.markdown.as_ref(),
            super::renderer::OutputFormat::Compact => {
                self.compact.as_ref().or(self.markdown.as_ref())
            }
            super::renderer::OutputFormat::Json => None, // JSON uses raw data
            super::renderer::OutputFormat::Text => {
                self.text.as_ref().or(self.markdown.as_ref())
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FormatTemplate {
    #[serde(default)]
    pub template: Option<String>,

    #[serde(default)]
    pub header: Option<String>,

    #[serde(default)]
    pub footer: Option<String>,

    #[serde(default)]
    pub item_template: Option<String>,

    #[serde(default)]
    pub separator: Option<String>,

    #[serde(default)]
    pub empty_template: Option<String>,
}

/// Dynamic data source configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionData {
    pub source: String,

    #[serde(default)]
    pub fields: Vec<String>,

    #[serde(default)]
    pub filter: Option<DataFilter>,

    #[serde(default)]
    pub sort_by: Option<String>,

    #[serde(default)]
    pub sort_order: Option<String>,

    #[serde(default)]
    pub max_items: Option<usize>,

    #[serde(default)]
    pub item_tokens: Option<u32>,

    #[serde(default)]
    pub empty_behavior: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DataFilter {
    Array(Vec<String>),
    Object(HashMap<String, serde_json::Value>),
}

/// Selection strategy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionStrategy {
    #[serde(default = "default_algorithm")]
    pub algorithm: String,

    #[serde(default)]
    pub weights: DimensionWeights,

    #[serde(default)]
    pub presets: HashMap<String, DimensionWeights>,

    #[serde(default)]
    pub phases: Vec<SelectionPhase>,

    #[serde(default = "default_min_budget")]
    pub minimum_budget: u32,

    #[serde(default = "default_dynamic_enabled")]
    pub dynamic_modifiers_enabled: bool,
}

impl Default for SelectionStrategy {
    fn default() -> Self {
        Self {
            algorithm: default_algorithm(),
            weights: DimensionWeights::default(),
            presets: HashMap::new(),
            phases: Vec::new(),
            minimum_budget: default_min_budget(),
            dynamic_modifiers_enabled: default_dynamic_enabled(),
        }
    }
}

fn default_algorithm() -> String {
    "value-optimized".to_string()
}

fn default_min_budget() -> u32 {
    80
}

fn default_dynamic_enabled() -> bool {
    true
}

/// Weights for multi-dimensional value calculation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionWeights {
    #[serde(default = "default_safety_weight")]
    pub safety: f64,

    #[serde(default = "default_weight")]
    pub efficiency: f64,

    #[serde(default = "default_weight")]
    pub accuracy: f64,

    #[serde(default = "default_weight")]
    pub base: f64,
}

impl Default for DimensionWeights {
    fn default() -> Self {
        Self {
            safety: default_safety_weight(),
            efficiency: default_weight(),
            accuracy: default_weight(),
            base: default_weight(),
        }
    }
}

fn default_safety_weight() -> f64 {
    1.5
}

fn default_weight() -> f64 {
    1.0
}

/// A phase in the selection process
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionPhase {
    pub name: String,

    #[serde(default)]
    pub filter: PhaseFilter,

    #[serde(default = "default_sort")]
    pub sort: String,

    #[serde(default)]
    pub budget_percent: Option<f32>,
}

fn default_sort() -> String {
    "value-per-token".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PhaseFilter {
    #[serde(default)]
    pub required: Option<bool>,

    #[serde(default)]
    pub required_if: Option<bool>,

    #[serde(default)]
    pub safety_minimum: Option<u8>,

    #[serde(default)]
    pub categories: Option<Vec<String>>,

    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

/// Output format configurations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutputFormatsConfig {
    #[serde(default)]
    pub markdown: Option<MarkdownConfig>,

    #[serde(default)]
    pub compact: Option<CompactConfig>,

    #[serde(default)]
    pub json: Option<JsonConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownConfig {
    #[serde(default)]
    pub section_separator: Option<String>,
    #[serde(default)]
    pub header_level: Option<u8>,
    #[serde(default)]
    pub list_style: Option<String>,
    #[serde(default)]
    pub code_block_style: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompactConfig {
    #[serde(default)]
    pub section_separator: Option<String>,
    #[serde(default)]
    pub max_line_length: Option<usize>,
    #[serde(default)]
    pub abbreviate: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonConfig {
    #[serde(default)]
    pub pretty: Option<bool>,
    #[serde(default)]
    pub include_metadata: Option<bool>,
    #[serde(default)]
    pub include_token_counts: Option<bool>,
}

/// Knowledge store configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeStoreConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub index_path: Option<String>,
    #[serde(default)]
    pub semantic_db_path: Option<String>,
    #[serde(default)]
    pub fallback_to_index: Option<bool>,
}

/// A section that has been selected for output
#[derive(Debug, Clone)]
pub struct SelectedSection {
    pub id: String,
    pub priority: u32,
    pub tokens: u32,
    pub value: f64,
    pub section: Section,
}
