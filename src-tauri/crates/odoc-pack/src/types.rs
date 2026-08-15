use odoc_model::{DocumentType, HeadingLevel};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub const PACK_SCHEMA_VERSION: &str = "0.1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackManifest {
    pub schema_version: String,
    pub pack: PackMetadata,
    pub capabilities: PackCapabilities,
    #[serde(default)]
    pub bindings: Vec<StageBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackMetadata {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub license: String,
    #[serde(default)]
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackCapabilities {
    #[serde(default)]
    pub doc_types: Vec<DocumentType>,
    #[serde(default)]
    pub provides: Vec<PackCapability>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum PackCapability {
    FormatRules,
    LanguageRules,
    Templates,
    References,
    Checklists,
    Style,
    Tests,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct PackSummary {
    pub schema_version: String,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub license: String,
    pub source: String,
    pub doc_types: Vec<DocumentType>,
    pub capabilities: Vec<PackCapability>,
}

impl From<&PackManifest> for PackSummary {
    fn from(manifest: &PackManifest) -> Self {
        Self {
            schema_version: manifest.schema_version.clone(),
            id: manifest.pack.id.clone(),
            name: manifest.pack.name.clone(),
            version: manifest.pack.version.clone(),
            description: manifest.pack.description.clone(),
            license: manifest.pack.license.clone(),
            source: manifest.pack.source.clone(),
            doc_types: manifest.capabilities.doc_types.clone(),
            capabilities: manifest.capabilities.provides.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
/// Canonical stage identity shared by every pipeline kind.
///
/// The declaration order is the canonical execution order: pipelines must list their stages in
/// ascending order, so a new variant has to be inserted where the stage actually runs.
pub enum StageId {
    Doctype,
    Facts,
    Retrieve,
    Outline,
    Draft,
    Deai,
    Lint,
    Factcheck,
    Review,
    Score,
    Export,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageBinding {
    pub stage: StageId,
    #[serde(default)]
    pub inject: Vec<String>,
    #[serde(default)]
    pub rules: Vec<String>,
    #[serde(default)]
    pub styles: Vec<String>,
    #[serde(default)]
    pub retrieval: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceRole {
    Inject,
    Rule,
    Style,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedResource {
    pub pack_id: String,
    pub relative_path: String,
    pub role: ResourceRole,
    pub retrieval: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentTemplate {
    pub doc_type: DocumentType,
    #[serde(default)]
    pub required: TemplateRequirements,
    #[serde(default)]
    pub blocks: Vec<TemplateBlock>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemplateRequirements {
    #[serde(default)]
    pub main_recipients: bool,
    pub closing: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TemplateBlock {
    Paragraph {
        role: String,
        guidance: Option<String>,
    },
    Heading {
        level: HeadingLevel,
        title: String,
        role: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RuleDefinition {
    pub id: String,
    pub name: String,
    pub severity: RuleSeverity,
    #[serde(rename = "type")]
    pub rule_type: RuleType,
    #[serde(default)]
    pub scope: String,
    pub message: String,
    #[serde(default)]
    pub suggestion: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default, rename = "match")]
    pub match_mode: Option<RegexMatchMode>,
    #[serde(default)]
    pub when_present: bool,
    #[serde(default)]
    pub lexicon: Vec<String>,
    #[serde(default)]
    pub max_per_100_chars: Option<f64>,
    #[serde(default)]
    pub detect: Option<StructureDetector>,
    #[serde(default)]
    pub min_run: Option<u32>,
    #[serde(default)]
    pub required: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSeverity {
    Error,
    Warn,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleType {
    Regex,
    LexiconDensity,
    Structure,
    Format,
    FactCheck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegexMatchMode {
    Required,
    Forbidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StructureDetector {
    ParallelPhrases,
}
