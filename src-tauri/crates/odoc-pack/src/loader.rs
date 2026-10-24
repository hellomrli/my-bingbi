use crate::{
    DocumentTemplate, PACK_SCHEMA_VERSION, PackCapability, PackError, PackManifest, RegexMatchMode,
    ResolvedResource, ResourceRole, RuleDefinition, RuleType, StageBinding, StageId,
    StructureDetector, TemplateSummary,
};
use odoc_model::DocumentType;
use regex::Regex;
use semver::Version;
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

const MANIFEST_FILE: &str = "pack.toml";
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_PACK_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PACK_FILES: usize = 512;

#[derive(Debug, Clone)]
pub struct LoadedPack {
    root: PathBuf,
    canonical_root: PathBuf,
    manifest: PackManifest,
    declared_resources: BTreeSet<String>,
}

impl LoadedPack {
    /// Loads and validates one declarative knowledge pack.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed manifests, unsafe paths or links, size-limit violations,
    /// invalid declared resources, duplicate rule identifiers, or filesystem failures.
    pub fn load(root: impl AsRef<Path>) -> Result<Self, PackError> {
        let root = root.as_ref();
        validate_directory(root)?;
        inspect_pack_tree(root)?;
        let canonical_root = fs::canonicalize(root)?;
        let manifest_path = root.join(MANIFEST_FILE);
        let manifest_text = read_limited_utf8(&manifest_path)?;
        let manifest: PackManifest = toml::from_str(&manifest_text)?;
        validate_manifest(root, &manifest)?;
        let mut pack = Self {
            root: root.to_path_buf(),
            canonical_root,
            manifest,
            declared_resources: BTreeSet::new(),
        };
        pack.validate_declared_resources()?;
        Ok(pack)
    }

    pub const fn manifest(&self) -> &PackManifest {
        &self.manifest
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn bindings_for(&self, stage: StageId) -> impl Iterator<Item = &StageBinding> {
        self.manifest
            .bindings
            .iter()
            .filter(move |binding| binding.stage == stage)
    }

    /// Resolves declared resources for a stage and document type without reading their contents.
    ///
    /// # Errors
    ///
    /// Returns an error when a declaration contains an unsafe path, unresolved placeholder, or a
    /// resource that changed after the pack was loaded.
    pub fn resources_for_stage(
        &self,
        stage: StageId,
        doc_type: Option<DocumentType>,
    ) -> Result<Vec<ResolvedResource>, PackError> {
        let mut resources = Vec::new();
        for binding in self.bindings_for(stage) {
            for reference in &binding.inject {
                resources.push(self.resolve_resource(
                    reference,
                    ResourceRole::Inject,
                    binding.retrieval,
                    doc_type,
                )?);
            }
            for reference in &binding.rules {
                resources.push(self.resolve_resource(
                    reference,
                    ResourceRole::Rule,
                    false,
                    doc_type,
                )?);
            }
            for reference in &binding.styles {
                resources.push(self.resolve_resource(
                    reference,
                    ResourceRole::Style,
                    false,
                    doc_type,
                )?);
            }
        }
        Ok(resources)
    }

    /// Reads a previously resolved text resource after repeating path and metadata checks.
    ///
    /// # Errors
    ///
    /// Returns an error when the path is unsafe, the file is missing, linked, oversized, outside
    /// the pack root, or not valid UTF-8.
    pub fn read_text(&self, relative_path: &str) -> Result<String, PackError> {
        let relative = validate_relative_path(relative_path, false)?;
        let normalized = path_to_slashes(&relative);
        if !self.declared_resources.contains(&normalized) {
            return Err(PackError::UndeclaredResource(normalized));
        }
        let path = self.checked_resource_path(&relative)?;
        read_limited_utf8(&path)
    }

    /// Parses a YAML resource into a caller-owned declarative type.
    ///
    /// # Errors
    ///
    /// Returns an error from safe resource reading or YAML deserialization.
    pub fn read_yaml<T: DeserializeOwned>(&self, relative_path: &str) -> Result<T, PackError> {
        let text = self.read_text(relative_path)?;
        serde_yaml::from_str(&text).map_err(|_| PackError::InvalidYaml(relative_path.to_owned()))
    }

    /// Reads and validates a structured document template.
    ///
    /// # Errors
    ///
    /// Returns an error from safe resource reading or template YAML deserialization.
    pub fn read_template(&self, relative_path: &str) -> Result<DocumentTemplate, PackError> {
        self.read_yaml(relative_path)
    }

    /// Lists every declared template, including scenario variants beyond the per-type default.
    ///
    /// Invalid template files are skipped instead of failing the whole list: a user pack may be
    /// mid-edit, and the drafting UI should still show the templates that do load.
    #[must_use]
    pub fn list_templates(&self) -> Vec<TemplateSummary> {
        let mut templates = self
            .declared_resources
            .iter()
            .filter(|path| {
                path.starts_with("templates/") && matches!(extension(path), Some("yaml" | "yml"))
            })
            .filter_map(|path| {
                let template = self.read_template(path).ok()?;
                let file_name = path.rsplit('/').next()?;
                let id = file_name.rsplit_once('.')?.0.to_owned();
                let default_path = format!("templates/{}.yaml", template.doc_type.as_str());
                Some(TemplateSummary {
                    name: template
                        .name
                        .unwrap_or_else(|| format!("{}（标准）", template.doc_type.label_zh())),
                    id,
                    doc_type: template.doc_type,
                    scenario: template.scenario,
                    description: template.description,
                    is_default: path == &default_path,
                })
            })
            .collect::<Vec<_>>();
        templates.sort_by(|left, right| {
            left.doc_type
                .as_str()
                .cmp(right.doc_type.as_str())
                .then_with(|| right.is_default.cmp(&left.is_default))
                .then_with(|| left.name.cmp(&right.name))
        });
        templates
    }

    fn validate_declared_resources(&mut self) -> Result<(), PackError> {
        let mut rule_ids = BTreeSet::new();
        let mut declared_resources = BTreeSet::new();
        for binding in &self.manifest.bindings {
            validate_binding(binding)?;
            for (reference, role) in binding
                .inject
                .iter()
                .map(|path| (path, ResourceRole::Inject))
                .chain(binding.rules.iter().map(|path| (path, ResourceRole::Rule)))
                .chain(
                    binding
                        .styles
                        .iter()
                        .map(|path| (path, ResourceRole::Style)),
                )
            {
                let doc_types = if reference.contains("{doc_type}") {
                    self.manifest.capabilities.doc_types.clone()
                } else {
                    Vec::new()
                };
                if reference.contains("{doc_type}") && doc_types.is_empty() {
                    return Err(PackError::InvalidResourcePath(reference.clone()));
                }
                if doc_types.is_empty() {
                    self.validate_one_declared_resource(
                        reference,
                        role,
                        None,
                        &mut rule_ids,
                        &mut declared_resources,
                    )?;
                } else {
                    for doc_type in doc_types {
                        self.validate_one_declared_resource(
                            reference,
                            role,
                            Some(doc_type),
                            &mut rule_ids,
                            &mut declared_resources,
                        )?;
                    }
                }
            }
        }
        self.declared_resources = declared_resources;
        Ok(())
    }

    fn validate_one_declared_resource(
        &self,
        reference: &str,
        role: ResourceRole,
        doc_type: Option<DocumentType>,
        rule_ids: &mut BTreeSet<String>,
        declared_resources: &mut BTreeSet<String>,
    ) -> Result<(), PackError> {
        let resource = self.resolve_resource(reference, role, false, doc_type)?;
        declared_resources.insert(resource.relative_path.clone());
        let text = read_limited_utf8(
            &self
                .checked_resource_path(&validate_relative_path(&resource.relative_path, false)?)?,
        )?;
        match role {
            ResourceRole::Rule => {
                // An empty or comment-only rule file is a valid empty rule set, so user packs
                // can start from a template and fill rules in incrementally.
                let rules: Vec<RuleDefinition> = serde_yaml::from_str(&text)
                    .map_err(|_| PackError::InvalidYaml(resource.relative_path.clone()))?;
                for rule in rules {
                    validate_rule(&rule)?;
                    if !rule_ids.insert(rule.id.clone()) {
                        return Err(PackError::DuplicateRuleId(rule.id));
                    }
                }
            }
            ResourceRole::Style => {
                toml::from_str::<toml::Value>(&text)
                    .map_err(|_| PackError::InvalidToml(resource.relative_path))?;
            }
            ResourceRole::Inject => match extension(&resource.relative_path) {
                Some("yaml" | "yml") => {
                    if resource.relative_path.starts_with("templates/") {
                        let template: DocumentTemplate = serde_yaml::from_str(&text)
                            .map_err(|_| PackError::InvalidYaml(resource.relative_path.clone()))?;
                        if doc_type.is_some_and(|expected| template.doc_type != expected) {
                            return Err(PackError::InvalidYaml(resource.relative_path));
                        }
                    } else {
                        serde_yaml::from_str::<serde_yaml::Value>(&text)
                            .map_err(|_| PackError::InvalidYaml(resource.relative_path))?;
                    }
                }
                Some("toml") => {
                    toml::from_str::<toml::Value>(&text)
                        .map_err(|_| PackError::InvalidToml(resource.relative_path))?;
                }
                Some("md") => {}
                _ => return Err(PackError::UnsupportedResource(resource.relative_path)),
            },
        }
        Ok(())
    }

    fn resolve_resource(
        &self,
        reference: &str,
        role: ResourceRole,
        retrieval: bool,
        doc_type: Option<DocumentType>,
    ) -> Result<ResolvedResource, PackError> {
        let expanded = expand_reference(reference, doc_type)?;
        let relative = validate_relative_path(&expanded, false)?;
        validate_extension(&expanded, role)?;
        let path = self.checked_resource_path(&relative)?;
        let size_bytes = fs::metadata(path)?.len();
        Ok(ResolvedResource {
            pack_id: self.manifest.pack.id.clone(),
            relative_path: path_to_slashes(&relative),
            role,
            retrieval,
            size_bytes,
        })
    }

    fn checked_resource_path(&self, relative: &Path) -> Result<PathBuf, PackError> {
        let path = self.root.join(relative);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| PackError::ResourceNotFound(path_to_slashes(relative)))?;
        if metadata.file_type().is_symlink() {
            return Err(PackError::SymlinkNotAllowed(path));
        }
        if !metadata.is_file() {
            return Err(PackError::UnsupportedResource(path_to_slashes(relative)));
        }
        if metadata.len() > MAX_FILE_BYTES {
            return Err(PackError::FileTooLarge {
                path,
                limit: MAX_FILE_BYTES,
            });
        }
        let canonical = fs::canonicalize(&path)?;
        if !canonical.starts_with(&self.canonical_root) {
            return Err(PackError::InvalidResourcePath(path_to_slashes(relative)));
        }
        Ok(path)
    }
}

fn validate_rule(rule: &RuleDefinition) -> Result<(), PackError> {
    validate_slug(&rule.id, "rule id")?;
    validate_text_field(&rule.name, "rule name", 160, &rule.id)?;
    validate_text_field(&rule.scope, "rule scope", 256, &rule.id)?;
    validate_text_field(&rule.message, "rule message", 1024, &rule.id)?;
    if let Some(suggestion) = &rule.suggestion {
        validate_text_field(suggestion, "rule suggestion", 1024, &rule.id)?;
    }
    match rule.rule_type {
        RuleType::Regex => validate_regex_rule(rule)?,
        RuleType::LexiconDensity => validate_lexicon_density_rule(rule)?,
        RuleType::Structure => validate_structure_rule(rule)?,
        RuleType::Format => validate_format_rule(rule)?,
        RuleType::FactCheck => validate_fact_check_rule(rule)?,
    }
    if rule.rule_type != RuleType::Regex && rule.when_present {
        return Err(invalid_rule(
            &rule.id,
            "when_present is only valid for regex rules",
        ));
    }
    if rule.rule_type == RuleType::Regex
        && rule.match_mode == Some(RegexMatchMode::Forbidden)
        && rule.when_present
    {
        return Err(invalid_rule(
            &rule.id,
            "forbidden regex rules cannot require when_present",
        ));
    }
    if rule.rule_type == RuleType::Structure
        && rule.detect != Some(StructureDetector::ParallelPhrases)
    {
        return Err(invalid_rule(&rule.id, "unsupported structure detector"));
    }
    Ok(())
}

fn validate_regex_rule(rule: &RuleDefinition) -> Result<(), PackError> {
    let pattern = rule
        .pattern
        .as_deref()
        .ok_or_else(|| invalid_rule(&rule.id, "regex rule requires pattern"))?;
    Regex::new(pattern).map_err(|_| invalid_rule(&rule.id, "regex pattern does not compile"))?;
    if rule.match_mode.is_none() {
        return Err(invalid_rule(&rule.id, "regex rule requires match"));
    }
    if !rule.lexicon.is_empty()
        || rule.max_per_100_chars.is_some()
        || rule.detect.is_some()
        || rule.min_run.is_some()
        || rule.required.is_some()
    {
        return Err(invalid_rule(&rule.id, "regex rule has unsupported fields"));
    }
    Ok(())
}

fn validate_lexicon_density_rule(rule: &RuleDefinition) -> Result<(), PackError> {
    if rule.lexicon.is_empty() {
        return Err(invalid_rule(&rule.id, "lexicon_density requires lexicon"));
    }
    for item in &rule.lexicon {
        validate_text_field(item, "lexicon item", 160, &rule.id)?;
    }
    let threshold = rule
        .max_per_100_chars
        .ok_or_else(|| invalid_rule(&rule.id, "lexicon_density requires max_per_100_chars"))?;
    if !threshold.is_finite() || threshold < 0.0 {
        return Err(invalid_rule(
            &rule.id,
            "max_per_100_chars must be finite and non-negative",
        ));
    }
    if rule.pattern.is_some()
        || rule.match_mode.is_some()
        || rule.detect.is_some()
        || rule.min_run.is_some()
        || rule.required.is_some()
    {
        return Err(invalid_rule(
            &rule.id,
            "lexicon_density has unsupported fields",
        ));
    }
    Ok(())
}

fn validate_structure_rule(rule: &RuleDefinition) -> Result<(), PackError> {
    if rule.detect.is_none() {
        return Err(invalid_rule(&rule.id, "structure rule requires detect"));
    }
    let min_run = rule
        .min_run
        .ok_or_else(|| invalid_rule(&rule.id, "structure rule requires min_run"))?;
    if min_run < 2 {
        return Err(invalid_rule(&rule.id, "min_run must be at least 2"));
    }
    if rule.pattern.is_some()
        || rule.match_mode.is_some()
        || !rule.lexicon.is_empty()
        || rule.max_per_100_chars.is_some()
        || rule.required.is_some()
    {
        return Err(invalid_rule(
            &rule.id,
            "structure rule has unsupported fields",
        ));
    }
    Ok(())
}

fn validate_format_rule(rule: &RuleDefinition) -> Result<(), PackError> {
    if rule.required.is_none() {
        return Err(invalid_rule(&rule.id, "format rule requires required"));
    }
    if rule.pattern.is_some()
        || rule.match_mode.is_some()
        || !rule.lexicon.is_empty()
        || rule.max_per_100_chars.is_some()
        || rule.detect.is_some()
        || rule.min_run.is_some()
    {
        return Err(invalid_rule(&rule.id, "format rule has unsupported fields"));
    }
    Ok(())
}

fn validate_fact_check_rule(rule: &RuleDefinition) -> Result<(), PackError> {
    if rule.pattern.is_some()
        || rule.match_mode.is_some()
        || !rule.lexicon.is_empty()
        || rule.max_per_100_chars.is_some()
        || rule.detect.is_some()
        || rule.min_run.is_some()
        || rule.required.is_some()
    {
        return Err(invalid_rule(&rule.id, "fact_check has unsupported fields"));
    }
    Ok(())
}

fn validate_text_field(
    value: &str,
    field: &'static str,
    max_bytes: usize,
    rule_id: &str,
) -> Result<(), PackError> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        Err(invalid_rule(rule_id, field))
    } else {
        Ok(())
    }
}

fn invalid_rule(rule_id: &str, reason: &'static str) -> PackError {
    PackError::InvalidRule {
        rule_id: rule_id.to_owned(),
        reason,
    }
}

#[derive(Debug, Default)]
pub struct PackRegistry {
    packs: BTreeMap<String, LoadedPack>,
}

impl PackRegistry {
    /// Snapshot the enabled registry for one operation. Core packs are mandatory; an empty
    /// selection (including legacy configurations) means no optional packs, not all packs.
    pub fn enabled(&self, active_ids: &[String]) -> Self {
        Self {
            packs: self.packs.iter()
                .filter(|(id, _)| matches!(id.as_str(), "gb9704-format" | "renmin-style") || active_ids.contains(id))
                .map(|(id, pack)| (id.clone(), pack.clone()))
                .collect(),
        }
    }

    /// Loads all immediate child directories containing a `pack.toml` manifest.
    ///
    /// # Errors
    ///
    /// Returns an error when the registry root is unsafe, a contained pack is invalid, or two
    /// manifests declare the same pack identifier.
    pub fn load(root: impl AsRef<Path>) -> Result<Self, PackError> {
        let root = root.as_ref();
        validate_directory(root)?;
        let mut directories = fs::read_dir(root)?.collect::<Result<Vec<_>, _>>()?;
        directories.sort_by_key(std::fs::DirEntry::file_name);
        let mut packs = BTreeMap::new();
        for entry in directories {
            let metadata = entry.path().symlink_metadata()?;
            if metadata.file_type().is_symlink() {
                return Err(PackError::SymlinkNotAllowed(entry.path()));
            }
            if !metadata.is_dir() || !entry.path().join(MANIFEST_FILE).is_file() {
                continue;
            }
            let pack = LoadedPack::load(entry.path())?;
            let id = pack.manifest.pack.id.clone();
            if packs.insert(id.clone(), pack).is_some() {
                return Err(PackError::DuplicatePackId(id));
            }
        }
        Ok(Self { packs })
    }

    /// Loads the built-in packs and additionally tolerates user packs from a second root.
    ///
    /// Invalid user packs and identifier clashes with built-ins are skipped and reported in
    /// the returned issue list; only problems inside the built-in root are fatal. A missing
    /// user root loads nothing and reports no issues.
    ///
    /// # Errors
    ///
    /// Returns an error when the built-in root is unsafe or contains an invalid pack.
    pub fn load_with_user_root(
        builtin_root: impl AsRef<Path>,
        user_root: impl AsRef<Path>,
    ) -> Result<(Self, Vec<PackError>), PackError> {
        let mut registry = Self::load(builtin_root)?;
        let mut issues = Vec::new();
        let user_root = user_root.as_ref();
        if !user_root.exists() {
            return Ok((registry, issues));
        }
        if let Err(error) = validate_directory(user_root) {
            return Ok((registry, vec![error]));
        }
        let mut directories = match fs::read_dir(user_root) {
            Ok(entries) => entries.collect::<Result<Vec<_>, _>>()?,
            Err(error) => return Ok((registry, vec![PackError::Io(error)])),
        };
        directories.sort_by_key(std::fs::DirEntry::file_name);
        for entry in directories {
            let path = entry.path();
            let Ok(metadata) = path.symlink_metadata() else {
                continue;
            };
            if !metadata.is_dir() || !path.join(MANIFEST_FILE).is_file() {
                continue;
            }
            match LoadedPack::load(&path) {
                Ok(pack) => {
                    let id = pack.manifest.pack.id.clone();
                    match registry.packs.entry(id) {
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            entry.insert(pack);
                        }
                        std::collections::btree_map::Entry::Occupied(entry) => {
                            issues.push(PackError::DuplicatePackId(entry.key().clone()));
                        }
                    }
                }
                Err(error) => issues.push(error),
            }
        }
        Ok((registry, issues))
    }

    pub fn get(&self, id: &str) -> Option<&LoadedPack> {
        self.packs.get(id)
    }

    /// Lists templates across every loaded pack, de-duplicated by template ID.
    #[must_use]
    pub fn list_templates(&self) -> Vec<TemplateSummary> {
        let mut by_id: BTreeMap<String, TemplateSummary> = BTreeMap::new();
        for pack in self.packs.values() {
            for template in pack.list_templates() {
                by_id.entry(template.id.clone()).or_insert(template);
            }
        }
        let mut templates = by_id.into_values().collect::<Vec<_>>();
        templates.sort_by(|left, right| {
            left.doc_type
                .as_str()
                .cmp(right.doc_type.as_str())
                .then_with(|| right.is_default.cmp(&left.is_default))
                .then_with(|| left.name.cmp(&right.name))
        });
        templates
    }

    pub fn iter(&self) -> impl Iterator<Item = &LoadedPack> {
        self.packs.values()
    }

    pub fn len(&self) -> usize {
        self.packs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.packs.is_empty()
    }
}

fn validate_directory(path: &Path) -> Result<(), PackError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(PackError::SymlinkNotAllowed(path.to_path_buf()));
    }
    if metadata.is_dir() {
        Ok(())
    } else {
        Err(PackError::InvalidManifest(
            "pack root is not a directory".to_owned(),
        ))
    }
}

fn inspect_pack_tree(root: &Path) -> Result<(), PackError> {
    let mut total_bytes = 0_u64;
    let mut file_count = 0_usize;
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(PackError::SymlinkNotAllowed(entry.path().to_path_buf()));
        }
        if metadata.is_file() {
            file_count = file_count.saturating_add(1);
            if file_count > MAX_PACK_FILES {
                return Err(PackError::TooManyFiles(MAX_PACK_FILES));
            }
            if metadata.len() > MAX_FILE_BYTES {
                return Err(PackError::FileTooLarge {
                    path: entry.path().to_path_buf(),
                    limit: MAX_FILE_BYTES,
                });
            }
            total_bytes = total_bytes.saturating_add(metadata.len());
            if total_bytes > MAX_PACK_BYTES {
                return Err(PackError::PackTooLarge(MAX_PACK_BYTES));
            }
        } else if !metadata.is_dir() {
            return Err(PackError::UnsupportedResource(
                entry.path().display().to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_manifest(root: &Path, manifest: &PackManifest) -> Result<(), PackError> {
    if manifest.schema_version != PACK_SCHEMA_VERSION {
        return Err(PackError::UnsupportedSchema(
            manifest.schema_version.clone(),
        ));
    }
    validate_slug(&manifest.pack.id, "pack id")?;
    validate_text(&manifest.pack.name, "pack name", 160)?;
    validate_text(&manifest.pack.license, "pack license", 128)?;
    if manifest.pack.description.len() > 1024 || manifest.pack.source.len() > 2048 {
        return Err(PackError::InvalidManifest(
            "pack metadata exceeds length limits".to_owned(),
        ));
    }
    Version::parse(&manifest.pack.version)
        .map_err(|_| PackError::InvalidManifest("pack version is not SemVer".to_owned()))?;
    let directory_id = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| PackError::DirectoryIdMismatch(manifest.pack.id.clone()))?;
    if directory_id != manifest.pack.id {
        return Err(PackError::DirectoryIdMismatch(manifest.pack.id.clone()));
    }
    let capabilities = manifest
        .capabilities
        .provides
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if capabilities.len() != manifest.capabilities.provides.len() {
        return Err(PackError::InvalidManifest(
            "capabilities contain duplicates".to_owned(),
        ));
    }
    let mut doc_types = manifest.capabilities.doc_types.clone();
    doc_types.sort_by_key(|doc_type| doc_type.as_str());
    doc_types.dedup();
    if doc_types.len() != manifest.capabilities.doc_types.len() {
        return Err(PackError::InvalidManifest(
            "document types contain duplicates".to_owned(),
        ));
    }
    for binding in &manifest.bindings {
        validate_binding(binding)?;
        if !binding.rules.is_empty()
            && !capabilities.contains(&PackCapability::FormatRules)
            && !capabilities.contains(&PackCapability::LanguageRules)
        {
            return Err(PackError::InvalidManifest(
                "rule bindings require a rules capability".to_owned(),
            ));
        }
        if !binding.styles.is_empty() && !capabilities.contains(&PackCapability::Style) {
            return Err(PackError::InvalidManifest(
                "style bindings require the style capability".to_owned(),
            ));
        }
    }
    Ok(())
}

fn validate_binding(binding: &StageBinding) -> Result<(), PackError> {
    if binding.inject.is_empty() && binding.rules.is_empty() && binding.styles.is_empty() {
        return Err(PackError::InvalidManifest(
            "stage binding has no resources".to_owned(),
        ));
    }
    if binding.retrieval && binding.inject.is_empty() {
        return Err(PackError::InvalidManifest(
            "retrieval binding has no injected resources".to_owned(),
        ));
    }
    Ok(())
}

fn validate_slug(value: &str, field: &str) -> Result<(), PackError> {
    let bytes = value.as_bytes();
    let valid = !bytes.is_empty()
        && bytes.len() <= 96
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-');
    if valid {
        Ok(())
    } else {
        Err(PackError::InvalidManifest(format!("invalid {field}")))
    }
}

fn validate_text(value: &str, field: &str, max_bytes: usize) -> Result<(), PackError> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        Err(PackError::InvalidManifest(format!("invalid {field}")))
    } else {
        Ok(())
    }
}

fn validate_relative_path(value: &str, allow_placeholder: bool) -> Result<PathBuf, PackError> {
    if value.is_empty()
        || value.len() > 512
        || value.contains('\\')
        || value.chars().any(char::is_control)
        || (!allow_placeholder && value.contains('{'))
    {
        return Err(PackError::InvalidResourcePath(value.to_owned()));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(PackError::InvalidResourcePath(value.to_owned()));
    }
    Ok(path.to_path_buf())
}

fn expand_reference(reference: &str, doc_type: Option<DocumentType>) -> Result<String, PackError> {
    validate_reference_placeholders(reference)?;
    if reference.contains("{doc_type}") {
        let doc_type =
            doc_type.ok_or_else(|| PackError::InvalidResourcePath(reference.to_owned()))?;
        Ok(reference.replace("{doc_type}", doc_type.as_str()))
    } else {
        Ok(reference.to_owned())
    }
}

fn validate_reference_placeholders(reference: &str) -> Result<(), PackError> {
    validate_relative_path(reference, true)?;
    let without_allowed = reference.replace("{doc_type}", "");
    if without_allowed.contains(['{', '}']) || reference.matches("{doc_type}").count() > 1 {
        Err(PackError::InvalidResourcePath(reference.to_owned()))
    } else {
        Ok(())
    }
}

fn validate_extension(path: &str, role: ResourceRole) -> Result<(), PackError> {
    let extension = extension(path);
    let valid = match role {
        ResourceRole::Inject => matches!(extension, Some("md" | "yaml" | "yml" | "toml")),
        ResourceRole::Rule => matches!(extension, Some("yaml" | "yml")),
        ResourceRole::Style => extension == Some("toml"),
    };
    if valid {
        Ok(())
    } else {
        Err(PackError::UnsupportedResource(path.to_owned()))
    }
}

fn extension(path: &str) -> Option<&str> {
    Path::new(path).extension().and_then(|value| value.to_str())
}

fn read_limited_utf8(path: &Path) -> Result<String, PackError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(PackError::SymlinkNotAllowed(path.to_path_buf()));
    }
    if !metadata.is_file() {
        return Err(PackError::UnsupportedResource(path.display().to_string()));
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(PackError::FileTooLarge {
            path: path.to_path_buf(),
            limit: MAX_FILE_BYTES,
        });
    }
    String::from_utf8(fs::read(path)?)
        .map_err(|_| PackError::InvalidUtf8(path.display().to_string()))
}

fn path_to_slashes(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}
