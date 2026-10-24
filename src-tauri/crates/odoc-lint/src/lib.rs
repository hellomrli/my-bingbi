use odoc_model::{Block, Fact, ODoc};
use odoc_pack::{
    LoadedPack, PackRegistry, RegexMatchMode, ResourceRole, RuleDefinition, RuleSeverity, RuleType,
    StageId, StructureDetector,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;
use ts_rs::TS;

const MAX_RULES: usize = 512;

#[derive(Debug, Error)]
pub enum LintError {
    #[error("knowledge-pack rule resource is invalid")]
    InvalidRuleResource,
    #[error("rule pattern is invalid")]
    InvalidPattern,
    #[error("rule set is too large")]
    TooManyRules,
    #[error("fact check pattern is invalid")]
    InvalidFactPattern,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warn,
    Info,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub severity: Severity,
    pub node_id: String,
    pub message: String,
    pub location: String,
    pub suggestion: Option<String>,
    /// The exact text span this finding targets, when it is a single inline token
    /// (set by the fact check so the frontend can red-mark the unverified value).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct LintReport {
    pub findings: Vec<Finding>,
    pub error_count: u32,
    pub warn_count: u32,
    pub info_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct FactCheckReport {
    pub findings: Vec<Finding>,
    pub checked_tokens: u32,
    pub unresolved_tokens: u32,
}

#[derive(Debug, Clone)]
struct Target {
    node_id: String,
    location: String,
    text: String,
}

/// Runs all deterministic rules declared by the knowledge packs for the lint stage.
///
/// # Errors
///
/// Returns [`LintError`] when a declared rule resource cannot be parsed or exceeds limits.
pub fn lint_document(document: &ODoc, packs: &PackRegistry) -> Result<LintReport, LintError> {
    let mut rules = Vec::new();
    for pack in packs.iter() {
        rules.extend(read_pack_rules(pack)?);
        if rules.len() > MAX_RULES {
            return Err(LintError::TooManyRules);
        }
    }
    let mut findings = Vec::new();
    for (pack_id, rule) in rules {
        apply_rule(document, &pack_id, &rule, &mut findings)?;
    }
    apply_builtin_rules(document, &mut findings);
    Ok(report(findings))
}

/// Checks numbers, dates and document numbers against verified facts in the `ODoc` ledger.
///
/// # Errors
///
/// Returns [`LintError`] when one of the bounded fact patterns cannot be compiled.
pub fn fact_check_document(document: &ODoc) -> Result<FactCheckReport, LintError> {
    let date =
        Regex::new(r"\d{4}年\d{1,2}月\d{1,2}日").map_err(|_| LintError::InvalidFactPattern)?;
    let document_number =
        Regex::new(r"[\p{Han}A-Za-z][\p{Han}A-Za-z0-9]*〔[0-9]{4}〕[0-9]+号|〔[0-9]{4}〕[0-9]+号").map_err(|_| LintError::InvalidFactPattern)?;
    let number_with_unit = Regex::new(r"[+\-−]?[0-9]+(?:\.[0-9]+)?(?:万|亿)?(?:平方千米|平方公里|平方米|平方厘米|立方米|千瓦时|百分点|人次|千米|公里|厘米|毫米|公顷|千克|公斤|毫克|千瓦|小时|分钟|秒钟|个月|万元|亿元|海里|吨|米|亩|升|克|秒|个|份|项|户|家|辆|套|台|件|人|次|元|年|月|日|%|％|号|万|亿)")
        .map_err(|_| LintError::InvalidFactPattern)?;
    let bare_number = Regex::new(r"(?:^|[^0-9A-Za-z_.+\-−])([+\-−]?[0-9]+(?:\.[0-9]+)?)")
        .map_err(|_| LintError::InvalidFactPattern)?;
    let verified = verified_fact_terms(&document.facts.facts);
    let mut findings = Vec::new();
    let mut checked_tokens = 0_u32;
    for target in fact_check_targets(document) {
        let tokens = extract_fact_tokens(
            &target.text,
            &date,
            &document_number,
            &number_with_unit,
            &bare_number,
        );
        checked_tokens = checked_tokens.saturating_add(tokens.len().try_into().unwrap_or(u32::MAX));
        for token in tokens {
            if verified.iter().any(|term| fact_covers_token(&token, term)) {
                continue;
            }
            findings.push(Finding {
                id: if findings.is_empty() {
                    "FACT-001".to_owned()
                } else {
                    format!("FACT-001-{}", findings.len() + 1)
                },
                severity: Severity::Error,
                node_id: target.node_id.clone(),
                message: format!("正文中的事实“{token}”未在已核实事实台账中找到。"),
                location: target.location.clone(),
                suggestion: Some("补充材料或在事实台账中核实该数据后再导出。".to_owned()),
                token: Some(token),
            });
        }
    }
    let unresolved_tokens = findings.len().try_into().unwrap_or(u32::MAX);
    Ok(FactCheckReport {
        findings,
        checked_tokens,
        unresolved_tokens,
    })
}

pub fn typescript_bindings() -> String {
    let declarations = [
        Severity::decl(),
        Finding::decl(),
        LintReport::decl(),
        FactCheckReport::decl(),
    ];
    format!(
        "// This file is generated from src-tauri/crates/odoc-lint. Do not edit by hand.\n\nexport {}\n",
        declarations.join("\n\nexport ")
    )
}

fn read_pack_rules(pack: &LoadedPack) -> Result<Vec<(String, RuleDefinition)>, LintError> {
    let resources = pack
        .resources_for_stage(StageId::Lint, None)
        .map_err(|_| LintError::InvalidRuleResource)?;
    let mut rules = Vec::new();
    let pack_id = pack.manifest().pack.id.clone();
    for resource in resources {
        if resource.role != ResourceRole::Rule {
            continue;
        }
        let parsed: Vec<RuleDefinition> = pack
            .read_yaml(&resource.relative_path)
            .map_err(|_| LintError::InvalidRuleResource)?;
        rules.extend(parsed.into_iter().map(|rule| (pack_id.clone(), rule)));
    }
    Ok(rules)
}

fn apply_rule(
    document: &ODoc,
    pack_id: &str,
    rule: &RuleDefinition,
    findings: &mut Vec<Finding>,
) -> Result<(), LintError> {
    let targets = targets_for_scope(document, &rule.scope);
    match rule.rule_type {
        RuleType::Regex => apply_regex_rule(pack_id, rule, &targets, findings),
        RuleType::LexiconDensity => apply_density_rule(pack_id, rule, &targets, findings),
        RuleType::Structure => apply_structure_rule(pack_id, rule, &targets, findings),
        RuleType::Format => apply_format_rule(document, pack_id, rule, &targets, findings),
        RuleType::FactCheck => Ok(()),
    }
}

fn apply_regex_rule(
    pack_id: &str,
    rule: &RuleDefinition,
    targets: &[Target],
    findings: &mut Vec<Finding>,
) -> Result<(), LintError> {
    let pattern = rule.pattern.as_deref().ok_or(LintError::InvalidPattern)?;
    let regex = Regex::new(pattern).map_err(|_| LintError::InvalidPattern)?;
    let present = targets.iter().any(|target| !target.text.trim().is_empty());
    if rule.when_present && !present {
        return Ok(());
    }
    match rule.match_mode {
        Some(RegexMatchMode::Required) => {
            if !targets.iter().any(|target| regex.is_match(&target.text)) {
                let target = targets.first().cloned().unwrap_or_else(|| Target {
                    node_id: document_fallback_id(),
                    location: rule.scope.clone(),
                    text: String::new(),
                });
                push_finding(findings, pack_id, rule, &target, 1);
            }
        }
        Some(RegexMatchMode::Forbidden) => {
            for (occurrence, target) in targets
                .iter()
                .filter(|target| regex.is_match(&target.text))
                .enumerate()
            {
                push_finding(findings, pack_id, rule, target, occurrence + 1);
            }
        }
        None => return Err(LintError::InvalidPattern),
    }
    Ok(())
}

fn apply_density_rule(
    pack_id: &str,
    rule: &RuleDefinition,
    targets: &[Target],
    findings: &mut Vec<Finding>,
) -> Result<(), LintError> {
    let threshold = rule.max_per_100_chars.ok_or(LintError::InvalidPattern)?;
    let count = targets
        .iter()
        .map(|target| {
            rule.lexicon
                .iter()
                .map(|term| target.text.matches(term).count())
                .sum::<usize>()
        })
        .sum::<usize>();
    let chars = targets
        .iter()
        .map(|target| target.text.chars().count())
        .sum::<usize>();
    let count_f = f64::from(u32::try_from(count).unwrap_or(u32::MAX));
    let chars_f = f64::from(u32::try_from(chars).unwrap_or(u32::MAX));
    if chars > 0 && count_f > threshold * (chars_f / 100.0) {
        let target = targets.first().cloned().unwrap_or_else(|| Target {
            node_id: document_fallback_id(),
            location: rule.scope.clone(),
            text: String::new(),
        });
        push_finding(findings, pack_id, rule, &target, 1);
    }
    Ok(())
}

fn apply_structure_rule(
    pack_id: &str,
    rule: &RuleDefinition,
    targets: &[Target],
    findings: &mut Vec<Finding>,
) -> Result<(), LintError> {
    if rule.detect != Some(StructureDetector::ParallelPhrases) {
        return Err(LintError::InvalidPattern);
    }
    let min_run = rule.min_run.unwrap_or(3) as usize;
    let mut occurrence = 0_usize;
    for target in targets {
        let max_run = target
            .text
            .split('、')
            .map(str::trim)
            .filter(|part| !part.is_empty() && part.chars().count() <= 24)
            .fold((0_usize, 0_usize), |(run, max), part| {
                let next = if part.contains(['，', '。', '；', '：']) {
                    0
                } else {
                    run + 1
                };
                (next, max.max(next))
            })
            .1;
        if max_run >= min_run {
            occurrence += 1;
            push_finding(findings, pack_id, rule, target, occurrence);
        }
    }
    Ok(())
}

fn apply_format_rule(
    document: &ODoc,
    pack_id: &str,
    rule: &RuleDefinition,
    targets: &[Target],
    findings: &mut Vec<Finding>,
) -> Result<(), LintError> {
    if rule.required != Some(true) {
        return Err(LintError::InvalidPattern);
    }
    let present = match rule.scope.as_str() {
        "body.mainRecipients" => document
            .body
            .main_recipients
            .iter()
            .any(|value| !value.trim().is_empty()),
        _ => targets.iter().any(|target| !target.text.trim().is_empty()),
    };
    if !present {
        let target = targets.first().cloned().unwrap_or_else(|| Target {
            node_id: document_fallback_id(),
            location: rule.scope.clone(),
            text: String::new(),
        });
        push_finding(findings, pack_id, rule, &target, 1);
    }
    Ok(())
}

fn push_finding(
    findings: &mut Vec<Finding>,
    pack_id: &str,
    rule: &RuleDefinition,
    target: &Target,
    occurrence: usize,
) {
    let base_id = format!("{pack_id}/{}", rule.id);
    // Every rule runs exactly once per lint pass, so the occurrence within that pass decides
    // the suffix. This keeps ID generation O(1) instead of rescanning all findings per push.
    let id = if occurrence <= 1 {
        base_id
    } else {
        format!("{base_id}-{occurrence}")
    };
    findings.push(Finding {
        id,
        severity: severity(rule.severity),
        node_id: target.node_id.clone(),
        message: rule.message.clone(),
        location: target.location.clone(),
        suggestion: rule.suggestion.clone(),
        token: None,
    });
}

fn apply_builtin_rules(document: &ODoc, findings: &mut Vec<Finding>) {
    let needs_recipient = document.meta.doc_type.requires_main_recipient();
    if needs_recipient
        && document
            .body
            .main_recipients
            .iter()
            .all(|value| value.trim().is_empty())
    {
        findings.push(Finding {
            id: "FMT-004".to_owned(),
            severity: Severity::Error,
            node_id: document.body.id.clone(),
            message: "公文缺少主送机关。".to_owned(),
            location: "主体 · 主送机关".to_owned(),
            suggestion: None,
            token: None,
        });
    }
    if document
        .body
        .blocks
        .iter()
        .all(|block| block.plain_text().trim().is_empty())
    {
        findings.push(Finding {
            id: "CNT-001".to_owned(),
            severity: Severity::Info,
            node_id: document.body.id.clone(),
            message: "正文尚未填写。".to_owned(),
            location: "主体 · 正文".to_owned(),
            suggestion: None,
            token: None,
        });
    }
    apply_typo_rules(document, findings);
    apply_chinese_date_rule(document, findings);
}

/// High-frequency official-document typos with their corrections, checked as exact word matches.
const TYPO_PAIRS: &[(&str, &str)] = &[
    ("布署", "部署"),
    ("部暑", "部署"),
    ("再接再励", "再接再厉"),
    ("一如继往", "一如既往"),
    ("幅射", "辐射"),
    ("帐蓬", "帐篷"),
    ("松驰", "松弛"),
    ("亲睐", "青睐"),
    ("编篡", "编纂"),
    ("急待", "亟待"),
    ("划拔", "划拨"),
    ("迫不急待", "迫不及待"),
    ("穿流不息", "川流不息"),
    ("名符其实", "名副其实"),
    ("相辅相承", "相辅相成"),
];

fn apply_typo_rules(document: &ODoc, findings: &mut Vec<Finding>) {
    let targets = fact_check_targets(document);
    let mut counter = 0_u32;
    for target in targets {
        for (wrong, right) in TYPO_PAIRS {
            let mut start = 0;
            while let Some(offset) = target.text[start..].find(wrong) {
                let index = start + offset;
                counter += 1;
                findings.push(Finding {
                    id: if counter == 1 {
                        "SPL-001".to_owned()
                    } else {
                        format!("SPL-001-{counter}")
                    },
                    severity: Severity::Warn,
                    node_id: target.node_id.clone(),
                    message: format!("疑似错别字“{wrong}”，应为“{right}”。"),
                    location: target.location.clone(),
                    suggestion: Some(right.to_string()),
                    token: Some(wrong.to_string()),
                });
                start = index + wrong.len();
            }
        }
    }
}

/// Maps Chinese numerals to digits for date conversion (e.g. 二〇二五 → 2025).
fn chinese_to_digits(value: &str) -> String {
    let digit = |character: char| match character {
        '〇' | '零' => '0',
        '一' => '1',
        '二' | '两' => '2',
        '三' => '3',
        '四' => '4',
        '五' => '5',
        '六' => '6',
        '七' => '7',
        '八' => '8',
        '九' => '9',
        _ => character,
    };
    value.chars().map(digit).collect()
}

fn apply_chinese_date_rule(document: &ODoc, findings: &mut Vec<Finding>) {
    let Ok(date_pattern) = Regex::new(
        r"[一二两三四五六七八九十〇零]+年[一二三四五六七八九十〇零]+月(?:[一二三四五六七八九十]+日)?",
    ) else {
        return;
    };
    let targets = fact_check_targets(document);
    let mut counter = 0_u32;
    for target in targets {
        let mut start = 0;
        while let Some(found) = date_pattern.find(&target.text[start..]) {
            let matched = found.as_str();
            let converted = chinese_to_digits(matched);
            counter += 1;
            findings.push(Finding {
                id: if counter == 1 {
                    "SPL-002".to_owned()
                } else {
                    format!("SPL-002-{counter}")
                },
                severity: Severity::Warn,
                node_id: target.node_id.clone(),
                message: format!("中文数字日期建议改为阿拉伯数字：{converted}。"),
                location: target.location.clone(),
                suggestion: Some(converted),
                token: Some(matched.to_owned()),
            });
            start += found.end();
        }
    }
}

fn report(findings: Vec<Finding>) -> LintReport {
    let error_count = findings
        .iter()
        .filter(|finding| finding.severity == Severity::Error)
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    let warn_count = findings
        .iter()
        .filter(|finding| finding.severity == Severity::Warn)
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    let info_count = findings
        .iter()
        .filter(|finding| finding.severity == Severity::Info)
        .count()
        .try_into()
        .unwrap_or(u32::MAX);
    LintReport {
        findings,
        error_count,
        warn_count,
        info_count,
    }
}

fn severity(value: RuleSeverity) -> Severity {
    match value {
        RuleSeverity::Error => Severity::Error,
        RuleSeverity::Warn => Severity::Warn,
        RuleSeverity::Info => Severity::Info,
    }
}

fn targets_for_scope(document: &ODoc, scope: &str) -> Vec<Target> {
    match scope {
        "body.title.text" => vec![Target {
            node_id: document.body.title.id.clone(),
            location: "主体 · 标题".to_owned(),
            text: document.body.title.text.clone(),
        }],
        "header.documentNumber" => document
            .header
            .as_ref()
            .map(|header| {
                vec![Target {
                    node_id: header.id.clone(),
                    location: "版头 · 发文字号".to_owned(),
                    text: header.document_number.clone().unwrap_or_default(),
                }]
            })
            .unwrap_or_default(),
        "body.date.value" => document
            .body
            .date
            .as_ref()
            .map(|date| {
                vec![Target {
                    node_id: date.id.clone(),
                    location: "落款 · 成文日期".to_owned(),
                    text: date.value.clone(),
                }]
            })
            .unwrap_or_default(),
        "body.mainRecipients" => vec![Target {
            node_id: document.body.id.clone(),
            location: "主体 · 主送机关".to_owned(),
            text: document.body.main_recipients.join("、"),
        }],
        "body.blocks" => body_targets(document),
        _ => vec![Target {
            node_id: document.body.id.clone(),
            location: scope.to_owned(),
            text: document.to_plain_text(),
        }],
    }
}

fn body_targets(document: &ODoc) -> Vec<Target> {
    document
        .body
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| Target {
            node_id: block.id().to_owned(),
            location: if matches!(block, Block::Heading { .. }) {
                format!("正文 · 第 {} 节", index + 1)
            } else {
                format!("正文 · 第 {} 段", index + 1)
            },
            text: block.plain_text(),
        })
        .collect()
}

fn fact_check_targets(document: &ODoc) -> Vec<Target> {
    let mut targets = Vec::new();
    if let Some(header) = &document.header {
        if let Some(number) = &header.document_number {
            targets.push(Target {
                node_id: header.id.clone(),
                location: "版头 · 发文字号".to_owned(),
                text: number.clone(),
            });
        }
    }
    targets.push(Target {
        node_id: document.body.title.id.clone(),
        location: "主体 · 标题".to_owned(),
        text: document.body.title.text.clone(),
    });
    targets.extend(body_targets(document));
    if let Some(date) = &document.body.date {
        targets.push(Target {
            node_id: date.id.clone(),
            location: "落款 · 成文日期".to_owned(),
            text: date.value.clone(),
        });
    }
    targets
}

fn extract_fact_tokens(
    text: &str,
    date: &Regex,
    document_number: &Regex,
    number_with_unit: &Regex,
    bare_number: &Regex,
) -> Vec<String> {
    let mut spans = date
        .find_iter(text)
        .map(|m| (m.start(), m.end(), m.as_str().to_owned()))
        .chain(
            document_number
                .find_iter(text)
                .map(|m| (m.start(), m.end(), document_number_token(m.as_str()).to_owned())),
        )
        .chain(
            number_with_unit
                .find_iter(text)
                .map(|m| (m.start(), m.end(), m.as_str().to_owned())),
        )
        .collect::<Vec<_>>();
    for capture in bare_number.captures_iter(text) {
        let Some(matched) = capture.get(1) else {
            continue;
        };
        if spans.iter().any(|(start, end, _)| *start <= matched.start() && *end >= matched.end()) {
            continue;
        }
        // Unknown units must not degrade into a bare number that an unrelated fact can clear.
        let suffix_len = text[matched.end()..].chars()
            .take_while(|character| matches!(*character, '\u{3400}'..='\u{9fff}'))
            .map(char::len_utf8).sum::<usize>();
        let end = matched.end() + suffix_len;
        spans.push((matched.start(), end, text[matched.start()..end].to_owned()));
    }
    spans.sort_by_key(|(start, end, _)| (*start, std::cmp::Reverse(*end)));
    let mut result = Vec::new();
    let mut covered_until = 0;
    for (start, end, token) in spans {
        if start < covered_until {
            continue;
        }
        covered_until = end;
        result.push(token);
    }
    result
}

fn verified_fact_terms(facts: &[Fact]) -> BTreeSet<String> {
    facts.iter().filter(|fact| fact.verified).map(|fact| {
        let value = fact.value.trim();
        match fact.unit.as_deref().map(str::trim).filter(|unit| !unit.is_empty()) {
            Some(unit) if !value.ends_with(unit) => format!("{value}{unit}"),
            _ => value.to_owned(),
        }
    }).filter(|term| !term.is_empty()).collect()
}

fn document_number_token(value: &str) -> &str {
    // Remove prose introducers, never the issuing authority itself. Unknown prose remains
    // unmatched rather than allowing two different authorities to share a verified number.
    let mut token = value;
    for prefix in ["依据", "根据", "按照", "参照", "文号为", "文号是", "文号", "编号为", "编号", "执行"] {
        if let Some((_, tail)) = token.rsplit_once(prefix) {
            token = tail;
        }
    }
    token
}

/// Document identifiers and dates require exact identity; quantities keep their sign and unit.
fn fact_covers_token(token: &str, term: &str) -> bool {
    if token == term {
        return true;
    }
    if token.contains('〔') || term.contains('〔')
        || (token.contains('年') && token.contains('月'))
        || (term.contains('年') && term.contains('月')) {
        return false;
    }
    match (fact_numeric_part(token), fact_numeric_part(term)) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

/// Decimal strings avoid rounding large identifiers/amounts through f64. A narrative prefix
/// is allowed for quantities ("已完成80%"), but never for document identifiers or dates.
fn fact_numeric_part(value: &str) -> Option<(bool, &str, &str, &str)> {
    let start = value.find(|character: char| character.is_ascii_digit())?;
    let negative = value[..start].ends_with(['-', '−']);
    let bytes = value.as_bytes();
    let mut end = start;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    let integer = value[start..end].trim_start_matches('0');
    let mut fraction = "";
    if bytes.get(end) == Some(&b'.') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
        let fraction_start = end + 1;
        end += 1;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
        fraction = value[fraction_start..end].trim_end_matches('0');
    }
    Some((negative, integer, fraction, &value[end..]))
}

fn document_fallback_id() -> String {
    "document".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use odoc_model::{Block, FactKind, ODoc};
    use odoc_pack::PackRegistry;
    use std::path::Path;

    fn packs() -> PackRegistry {
        PackRegistry::load(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../packs"))
            .expect("packs should load")
    }

    #[test]
    fn runs_pack_rules_against_an_empty_document() {
        let document = ODoc::empty();
        let report = lint_document(&document, &packs()).expect("rules should run");
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.id == "gb9704-format/title-required")
        );
        assert!(
            !report
                .findings
                .iter()
                .any(|finding| finding.id == "gb9704-format/wenhao-bracket")
        );
    }

    #[test]
    fn detects_forbidden_language_and_density() {
        let mut document = ODoc::empty();
        document.body.title.text = "测试".to_owned();
        document.body.blocks = vec![Block::paragraph(
            "请相关部门走深走实、凝心聚力、开拓创新。".to_owned(),
        )];
        let report = lint_document(&document, &packs()).expect("rules should run");
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.id.starts_with("renmin-style/vague-actor"))
        );
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.id.starts_with("renmin-style/slogan-density"))
        );
    }

    #[test]
    fn flags_meta_commentary_from_the_skill_pack() {
        let mut document = ODoc::empty();
        document.body.title.text = "测试".to_owned();
        document.body.blocks = vec![Block::paragraph(
            "下面从三个方面展开：一是提高认识，二是强化担当。".to_owned(),
        )];
        let report = lint_document(&document, &packs()).expect("rules should run");
        assert!(
            report
                .findings
                .iter()
                .any(|finding| finding.id.starts_with("renmin-style/meta-commentary"))
        );
    }

    #[test]
    fn flags_unverified_numbers() {
        let mut document = ODoc::empty();
        document.body.blocks = vec![Block::paragraph("购置15万元设备。".to_owned())];
        let report = fact_check_document(&document).expect("fact check should run");
        assert_eq!(report.unresolved_tokens, 1);
        document.facts.facts.push(Fact::from_user_input(
            FactKind::Data,
            "15".to_owned(),
            Some("万元".to_owned()),
            None,
            None,
        ));
        let verified = fact_check_document(&document).expect("fact check should run");
        assert_eq!(verified.unresolved_tokens, 0);
    }

    #[test]
    fn fact_check_preserves_units_signs_and_issuing_authorities() {
        for (body, value, unit, unresolved) in [
            ("修建15公里道路。", "15", Some("平方米"), 1),
            ("修建15公里道路。", "15", Some("公里"), 0),
            ("增长-15%。", "15", Some("%"), 1),
            ("增长−15%。", "-15", Some("%"), 0),
            ("增长-15%。", "-15", Some("%"), 0),
            ("依据乙发〔2026〕27号执行。", "甲发〔2026〕27号", None, 1),
            ("依据甲发〔2026〕27号执行。", "甲发〔2026〕27号", None, 0),
            ("修建15海里。", "15", None, 1),
            ("购置15桶。", "15", None, 1),
            ("投资15万元。", "15万元", Some("万元"), 0),
        ] {
            let mut document = ODoc::empty();
            document.body.blocks = vec![Block::paragraph(body.to_owned())];
            document.facts.facts.push(Fact::from_user_input(FactKind::Data, value.to_owned(), unit.map(str::to_owned), None, None));
            let report = fact_check_document(&document).expect("check");
            assert_eq!(report.unresolved_tokens, unresolved, "{body} / {value} {unit:?}");
        }
        assert!(!fact_covers_token("9007199254740993元", "9007199254740992元"));
        assert!(!fact_covers_token("-15%", "15%"));
        assert!(!fact_covers_token("〔2026〕27号", "甲发〔2026〕27号"));
    }

    #[test]
    fn does_not_clear_tokens_through_substring_matches() {
        // A verified "15" must not clear "150万元", and a full date must not clear bare years.
        assert!(!fact_covers_token("150万元", "15"));
        assert!(!fact_covers_token("2026年", "2026年12月31日"));
        // Different units are different facts even when the number matches.
        assert!(!fact_covers_token("15亿", "15万元"));
        assert!(!fact_covers_token("15万元", "15"));
        // Equal numbers with the same unit tail match wherever they appear in the value.
        assert!(fact_covers_token("80%", "已完成80%"));
        assert!(fact_covers_token("15万元", "15万元"));
        assert!(fact_covers_token("15", "15"));
        assert!(fact_covers_token("15.5万元", "15.50万元"));
    }
}
