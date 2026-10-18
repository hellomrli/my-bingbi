use crate::KbProvider;
use crate::types::{
    ChangeEvent, ChangeKind, Frontmatter, KbError, KbHit, KbNote, KbQuery, MAX_NOTE_BYTES,
    SearchMode, contains_invalid_note_control, normalize_relative_path, path_matches_prefix,
    validate_relative_path,
};
use async_trait::async_trait;
use regex::Regex;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use tokio::sync::broadcast;
use walkdir::WalkDir;

const MAX_SCAN_FILES: usize = 20_000;
const MAX_SNIPPET_CHARS: usize = 480;

/// A read/write provider for a local Obsidian vault.
#[derive(Clone)]
pub struct LocalVaultProvider {
    root: Arc<PathBuf>,
    vault: String,
    events: broadcast::Sender<ChangeEvent>,
}

impl LocalVaultProvider {
    /// Opens an existing vault directory without following a symlinked root.
    ///
    /// # Errors
    ///
    /// Returns [`KbError::InvalidConfiguration`] when the root is not a regular directory, or
    /// [`KbError::Io`] when the directory metadata cannot be read.
    pub fn open(root: impl AsRef<Path>, vault: impl Into<String>) -> Result<Self, KbError> {
        let root = root.as_ref();
        let metadata = fs::symlink_metadata(root).map_err(KbError::Io)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(KbError::InvalidConfiguration);
        }
        let (events, _) = broadcast::channel(64);
        Ok(Self {
            root: Arc::new(root.to_path_buf()),
            vault: vault.into(),
            events,
        })
    }

    pub fn root(&self) -> &Path {
        self.root.as_ref()
    }

    fn note_path_on(root: &Path, path: &str) -> Result<PathBuf, KbError> {
        let normalized = normalize_relative_path(path)?;
        let candidate = root.join(&normalized);
        let metadata = fs::symlink_metadata(&candidate).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                KbError::NotFound
            } else {
                KbError::Io(error)
            }
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(KbError::InvalidPath);
        }
        let canonical_root = fs::canonicalize(root).map_err(KbError::Io)?;
        let canonical_candidate = fs::canonicalize(&candidate).map_err(KbError::Io)?;
        if !canonical_candidate.starts_with(&canonical_root) {
            return Err(KbError::InvalidPath);
        }
        Ok(candidate)
    }

    fn writable_note_path_on(root: &Path, path: &str) -> Result<PathBuf, KbError> {
        let normalized = normalize_relative_path(path)?;
        let relative = Path::new(&normalized);
        if relative.extension().and_then(|value| value.to_str()) != Some("md") {
            return Err(KbError::InvalidPath);
        }

        let parent = relative.parent().ok_or(KbError::InvalidPath)?;
        let mut current = root.to_path_buf();
        for component in parent.components() {
            let std::path::Component::Normal(component) = component else {
                return Err(KbError::InvalidPath);
            };
            current.push(component);
            match fs::symlink_metadata(&current) {
                Ok(metadata) => {
                    if metadata.file_type().is_symlink() || !metadata.is_dir() {
                        return Err(KbError::InvalidPath);
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    match fs::create_dir(&current) {
                        Ok(()) => {}
                        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                        Err(error) => return Err(KbError::Io(error)),
                    }
                    let metadata = fs::symlink_metadata(&current).map_err(KbError::Io)?;
                    if metadata.file_type().is_symlink() || !metadata.is_dir() {
                        return Err(KbError::InvalidPath);
                    }
                }
                Err(error) => return Err(KbError::Io(error)),
            }
        }

        let canonical_root = fs::canonicalize(root).map_err(KbError::Io)?;
        let canonical_parent = fs::canonicalize(&current).map_err(KbError::Io)?;
        if !canonical_parent.starts_with(&canonical_root) {
            return Err(KbError::InvalidPath);
        }

        let target = root.join(relative);
        match fs::symlink_metadata(&target) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_file() {
                    return Err(KbError::InvalidPath);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(KbError::Io(error)),
        }
        Ok(target)
    }

    /// The synchronous vault scan; always run through [`offload`] from async contexts.
    fn scan_on(root: &Path, vault: &str, query: &KbQuery) -> Result<Vec<KbHit>, KbError> {
        query.validate()?;
        let prefix = query
            .path_prefix
            .as_deref()
            .map(normalize_relative_path)
            .transpose()?;
        let matcher = if query.search_mode == SearchMode::Regex && !query.keyword.is_empty() {
            Some(Regex::new(&query.keyword).map_err(|_| KbError::InvalidQuery)?)
        } else {
            None
        };
        let keyword = query.keyword.to_lowercase();
        let mut hits = Vec::new();
        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .take(MAX_SCAN_FILES)
        {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("md") {
                continue;
            }
            let relative = match path.strip_prefix(root) {
                Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            if !path_matches_prefix(&relative, prefix.as_deref()) {
                continue;
            }
            // One unreadable or malformed note (open frontmatter, non-UTF-8 bytes, a file
            // deleted mid-walk) must not take down the whole vault search.
            let Ok(bytes) = fs::read(path) else {
                continue;
            };
            if bytes.len() > MAX_NOTE_BYTES {
                continue;
            }
            let Ok(text) = String::from_utf8(bytes) else {
                continue;
            };
            let Ok(note) = parse_markdown_note(vault, &relative, &text) else {
                continue;
            };
            if !filters_match(&note.frontmatter, &query.filters) {
                continue;
            }
            let searchable_path = relative.to_lowercase();
            // The title lives in frontmatter, so body search must include it explicitly; otherwise
            // a note whose title is "防汛工作要点" disappears when the body says "汛前排查".
            let searchable_content = format!("{}\n{}", note.title, note.content).to_lowercase();
            let is_match = match query.search_mode {
                SearchMode::Path => keyword.is_empty() || searchable_path.contains(&keyword),
                SearchMode::Content => {
                    if query.search_content {
                        keyword.is_empty() || searchable_content.contains(&keyword)
                    } else {
                        keyword.is_empty() || searchable_path.contains(&keyword)
                    }
                }
                SearchMode::Regex => matcher
                    .as_ref()
                    .is_none_or(|regex| regex.is_match(&note.content)),
            };
            if !is_match {
                continue;
            }
            let score = score_note(&note, &keyword);
            hits.push(KbHit {
                vault: note.vault,
                path: note.path,
                title: note.title,
                snippet: make_snippet(&note.content, &query.keyword),
                score,
                frontmatter: note.frontmatter,
            });
        }
        hits.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.path.cmp(&right.path))
        });
        hits.truncate(query.normalized_limit() as usize);
        Ok(hits)
    }
    fn get_on(root: &Path, vault: &str, path: &str) -> Result<KbNote, KbError> {
        let path = Self::note_path_on(root, path)?;
        let bytes = fs::read(&path).map_err(KbError::Io)?;
        if bytes.len() > MAX_NOTE_BYTES {
            return Err(KbError::TooLarge);
        }
        let content = String::from_utf8(bytes).map_err(|_| KbError::InvalidNote)?;
        let relative = path
            .strip_prefix(root)
            .map_err(|_| KbError::InvalidPath)?
            .to_string_lossy()
            .replace('\\', "/");
        parse_markdown_note(vault, &relative, &content)
    }

    fn backlinks_on(root: &Path, path: &str) -> Result<Vec<String>, KbError> {
        let path = normalize_relative_path(path)?;
        let needle = link_target(&path);
        let mut links = Vec::new();
        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            if entry.path().extension().and_then(|value| value.to_str()) != Some("md") {
                continue;
            }
            let relative = match entry.path().strip_prefix(root) {
                Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            // read_to_string also rejects non-UTF-8 notes; a single bad file must not fail
            // the whole backlink scan.
            let Ok(text) = fs::read_to_string(entry.path()) else {
                continue;
            };
            if extract_wikilinks(&text)
                .iter()
                .any(|value| value == &needle)
            {
                links.push(relative);
            }
            if links.len() >= MAX_SCAN_FILES {
                break;
            }
        }
        links.sort();
        links.dedup();
        Ok(links)
    }

    fn write_on(root: &Path, path: &str, content: &str) -> Result<(), KbError> {
        if content.len() > MAX_NOTE_BYTES || contains_invalid_note_control(content) {
            return Err(KbError::InvalidNote);
        }
        let target = Self::writable_note_path_on(root, path)?;
        fs::write(&target, content).map_err(KbError::Io)
    }

    fn list_notes_on(root: &Path, vault: &str, prefix: Option<&str>, limit: usize) -> Vec<KbNote> {
        let mut paths = WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .filter_map(|entry| {
                let path = entry.path();
                (path.extension().and_then(|value| value.to_str()) == Some("md"))
                    .then(|| path.to_path_buf())
            })
            .collect::<Vec<_>>();
        paths.sort();
        let mut notes = Vec::new();
        for path in paths.into_iter().take(MAX_SCAN_FILES) {
            let relative = match path.strip_prefix(root) {
                Ok(relative) => relative.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };
            if !path_matches_prefix(&relative, prefix) {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            if bytes.len() > MAX_NOTE_BYTES {
                continue;
            }
            let Ok(source) = String::from_utf8(bytes) else {
                continue;
            };
            let Ok(mut note) = parse_markdown_note(vault, &relative, &source) else {
                continue;
            };
            note.modified_at = fs::metadata(&path)
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|value| i64::try_from(value.as_millis()).ok());
            notes.push(note);
            if notes.len() >= limit {
                break;
            }
        }
        notes
    }
}

#[async_trait]
impl KbProvider for LocalVaultProvider {
    async fn search(&self, query: &KbQuery) -> Result<Vec<KbHit>, KbError> {
        let root = Arc::clone(&self.root);
        let vault = self.vault.clone();
        let query = query.clone();
        offload(move || Self::scan_on(root.as_path(), &vault, &query)).await
    }

    async fn get(&self, path: &str) -> Result<KbNote, KbError> {
        let root = Arc::clone(&self.root);
        let vault = self.vault.clone();
        let path = path.to_owned();
        offload(move || Self::get_on(root.as_path(), &vault, &path)).await
    }

    async fn backlinks(&self, path: &str) -> Result<Vec<String>, KbError> {
        let root = Arc::clone(&self.root);
        let path = path.to_owned();
        offload(move || Self::backlinks_on(root.as_path(), &path)).await
    }

    async fn outlinks(&self, path: &str) -> Result<Vec<String>, KbError> {
        let note = self.get(path).await?;
        Ok(note.links)
    }

    async fn write(&self, path: &str, content: &str) -> Result<(), KbError> {
        let root = Arc::clone(&self.root);
        let path_owned = path.to_owned();
        let content_owned = content.to_owned();
        offload(move || Self::write_on(root.as_path(), &path_owned, &content_owned)).await?;
        let _ = self.events.send(ChangeEvent {
            kind: ChangeKind::Modified,
            vault: self.vault.clone(),
            path: path.replace('\\', "/"),
            old_path: None,
            cursor: None,
        });
        Ok(())
    }

    async fn patch_frontmatter(
        &self,
        path: &str,
        fields: &BTreeMap<String, String>,
    ) -> Result<(), KbError> {
        let note = self.get(path).await?;
        for key in fields.keys() {
            if key.is_empty()
                || key
                    .chars()
                    .any(|value| value.is_control() || matches!(value, ':' | '#'))
            {
                return Err(KbError::InvalidQuery);
            }
        }
        let mut frontmatter = note.frontmatter;
        frontmatter.extend(fields.clone());
        let mut output = String::new();
        if !frontmatter.is_empty() {
            output.push_str("---\n");
            for (key, value) in &frontmatter {
                if value.contains('\n') || value.contains('\r') {
                    return Err(KbError::InvalidNote);
                }
                output.push_str(key);
                output.push_str(": ");
                output.push_str(&yaml_scalar(value));
                output.push('\n');
            }
            output.push_str("---\n");
        }
        output.push_str(&note.content);
        self.write(path, &output).await
    }

    async fn list_notes(
        &self,
        path_prefix: Option<&str>,
        limit: u32,
    ) -> Result<Vec<KbNote>, KbError> {
        let prefix = path_prefix.map(normalize_relative_path).transpose()?;
        let scan_ceiling = u32::try_from(MAX_SCAN_FILES).unwrap_or(u32::MAX);
        let limit = usize::try_from(limit.clamp(1, scan_ceiling)).unwrap_or(MAX_SCAN_FILES);
        let root = Arc::clone(&self.root);
        let vault = self.vault.clone();
        offload(move || {
            Ok(Self::list_notes_on(
                root.as_path(),
                &vault,
                prefix.as_deref(),
                limit,
            ))
        })
        .await
    }

    fn watch(&self) -> Option<broadcast::Receiver<ChangeEvent>> {
        Some(self.events.subscribe())
    }
}

/// Parses the bounded frontmatter and body of one Markdown note.
///
/// # Errors
///
/// Returns [`KbError::InvalidPath`] for an unsafe relative path, or [`KbError::InvalidNote`]
/// when the Markdown/frontmatter exceeds limits or contains unsupported values.
pub fn parse_markdown_note(vault: &str, path: &str, source: &str) -> Result<KbNote, KbError> {
    validate_relative_path(path)?;
    if source.len() > MAX_NOTE_BYTES || contains_invalid_note_control(source) {
        return Err(KbError::InvalidNote);
    }
    let (frontmatter, content) = split_frontmatter(source)?;
    let title = frontmatter
        .get("title")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            Path::new(path)
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or(path)
                .to_owned()
        });
    let links = extract_wikilinks(&content);
    let tags = extract_tags(&content, &frontmatter);
    Ok(KbNote {
        vault: vault.to_owned(),
        path: path.replace('\\', "/"),
        title,
        content,
        frontmatter,
        tags,
        links,
        modified_at: None,
    })
}

fn split_frontmatter(source: &str) -> Result<(Frontmatter, String), KbError> {
    let Some(rest) = source.strip_prefix("---\n") else {
        return Ok((Frontmatter::new(), source.to_owned()));
    };
    let Some(end) = rest.find("\n---\n") else {
        return Err(KbError::InvalidNote);
    };
    let yaml = &rest[..end];
    let parsed: serde_yaml::Value = serde_yaml::from_str(yaml).map_err(|_| KbError::InvalidNote)?;
    let mapping = parsed.as_mapping().ok_or(KbError::InvalidNote)?;
    let mut fields = Frontmatter::new();
    for (key, value) in mapping {
        let Some(key) = key.as_str() else {
            return Err(KbError::InvalidNote);
        };
        let value = match value {
            serde_yaml::Value::String(value) => value.clone(),
            serde_yaml::Value::Number(value) => value.to_string(),
            serde_yaml::Value::Bool(value) => value.to_string(),
            serde_yaml::Value::Null => String::new(),
            serde_yaml::Value::Sequence(values) => values
                .iter()
                .map(yaml_value_text)
                .collect::<Result<Vec<_>, _>>()?
                .join(","),
            _ => return Err(KbError::InvalidNote),
        };
        fields.insert(key.to_owned(), value);
    }
    Ok((fields, rest[end + "\n---\n".len()..].to_owned()))
}

fn yaml_value_text(value: &serde_yaml::Value) -> Result<String, KbError> {
    match value {
        serde_yaml::Value::String(value) => Ok(value.clone()),
        serde_yaml::Value::Number(value) => Ok(value.to_string()),
        serde_yaml::Value::Bool(value) => Ok(value.to_string()),
        serde_yaml::Value::Null => Ok(String::new()),
        _ => Err(KbError::InvalidNote),
    }
}

/// Runs blocking vault IO on the blocking thread pool so large scans never stall a tokio
/// worker. A panicking task is surfaced as an IO error instead of aborting the runtime.
async fn offload<T, F>(task: F) -> Result<T, KbError>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, KbError> + Send + 'static,
{
    tokio::task::spawn_blocking(task)
        .await
        .map_err(|error| KbError::Io(std::io::Error::other(error.to_string())))?
}

fn extract_wikilinks(content: &str) -> Vec<String> {
    static WIKILINK: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\[\[([^\]|#]+)(?:#[^\]|]+)?(?:\|[^\]]+)?\]\]").expect("static wikilink regex")
    });
    let mut links = WIKILINK
        .captures_iter(content)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str().trim().to_owned()))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    links.sort();
    links.dedup();
    links
}

fn extract_tags(content: &str, frontmatter: &Frontmatter) -> Vec<String> {
    static TAG: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?m)(?:^|\s)#([\p{L}\p{N}_/-]+)").expect("static tag regex"));
    let mut tags = TAG
        .captures_iter(content)
        .filter_map(|capture| capture.get(1).map(|value| value.as_str().to_owned()))
        .collect::<Vec<_>>();
    if let Some(value) = frontmatter.get("tags") {
        tags.extend(
            value
                .split([',', '、', ' '])
                .filter(|tag| !tag.is_empty())
                .map(str::to_owned),
        );
    }
    tags.sort();
    tags.dedup();
    tags
}

fn filters_match(frontmatter: &Frontmatter, filters: &Frontmatter) -> bool {
    filters
        .iter()
        .all(|(key, value)| frontmatter.get(key) == Some(value))
}

fn score_note(note: &KbNote, keyword: &str) -> f32 {
    if keyword.is_empty() {
        return 0.0;
    }
    let mut score = 0.0;
    let lower_title = note.title.to_lowercase();
    let lower_path = note.path.to_lowercase();
    if lower_title.contains(keyword) {
        score += 4.0;
    }
    if lower_path.contains(keyword) {
        score += 2.0;
    }
    let occurrences =
        u32::try_from(note.content.to_lowercase().matches(keyword).count()).unwrap_or(u32::MAX);
    score + occurrences.to_string().parse::<f32>().unwrap_or(0.0)
}

fn make_snippet(content: &str, keyword: &str) -> String {
    let content = content.trim();
    if content.chars().count() <= MAX_SNIPPET_CHARS || keyword.is_empty() {
        return content.chars().take(MAX_SNIPPET_CHARS).collect();
    }
    let lower = content.to_lowercase();
    let match_start = lower.find(&keyword.to_lowercase()).unwrap_or(0);
    let start_chars = lower[..match_start]
        .chars()
        .count()
        .saturating_sub(MAX_SNIPPET_CHARS / 3);
    let snippet: String = content
        .chars()
        .skip(start_chars)
        .take(MAX_SNIPPET_CHARS)
        .collect();
    if start_chars > 0 {
        format!("…{snippet}")
    } else {
        snippet
    }
}

fn link_target(path: &str) -> String {
    Path::new(path)
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/")
}

fn yaml_scalar(value: &str) -> String {
    if value.is_empty() || value.chars().any(char::is_whitespace) || value.contains([':', '#']) {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    #[test]
    fn parses_frontmatter_links_tags_and_title() {
        let note = parse_markdown_note(
            "vault",
            "公文/2026/请示.md",
            "---\nyear: 2026\ndoc_type: request\ntags: [防汛, 预算]\n---\n# 申请\n\n参考 [[政策/防汛]] 和 [[政策/防汛#条款|条款]] #应急。",
        )
        .expect("note should parse");
        assert_eq!(note.title, "请示");
        assert_eq!(note.frontmatter.get("year"), Some(&"2026".to_owned()));
        assert_eq!(note.links, vec!["政策/防汛"]);
        assert!(note.tags.contains(&"防汛".to_owned()));
        assert!(note.tags.contains(&"应急".to_owned()));
    }

    #[tokio::test]
    async fn local_provider_searches_filters_links_and_patches() {
        let directory = tempdir().expect("temp dir should exist");
        let provider =
            LocalVaultProvider::open(directory.path(), "vault").expect("vault should open");
        provider
            .write(
                "公文/2026/a.md",
                "---\ndoc_type: request\n---\n申请购置设备，参考 [[政策/设备]]。",
            )
            .await
            .expect("note should write");
        provider
            .write("政策/设备.md", "设备采购管理办法")
            .await
            .expect("policy should write");
        let query = KbQuery {
            keyword: "设备".to_owned(),
            filters: BTreeMap::from([(String::from("doc_type"), String::from("request"))]),
            ..KbQuery::default()
        };
        let hits = provider.search(&query).await.expect("search should work");
        assert_eq!(hits.len(), 1);
        assert_eq!(
            provider.outlinks("公文/2026/a.md").await.unwrap(),
            vec!["政策/设备"]
        );
        assert_eq!(
            provider.backlinks("政策/设备.md").await.unwrap(),
            vec!["公文/2026/a.md"]
        );
        let mut fields = BTreeMap::new();
        fields.insert("year".to_owned(), "2026".to_owned());
        provider
            .patch_frontmatter("公文/2026/a.md", &fields)
            .await
            .unwrap();
        assert_eq!(
            provider
                .get("公文/2026/a.md")
                .await
                .unwrap()
                .frontmatter
                .get("year"),
            Some(&"2026".to_owned())
        );
    }

    #[tokio::test]
    async fn title_is_searchable_when_the_body_lacks_the_keyword() {
        let directory = tempdir().expect("temp dir should exist");
        let provider =
            LocalVaultProvider::open(directory.path(), "vault").expect("vault should open");
        provider
            .write(
                "防汛工作要点.md",
                "---\ntitle: 防汛工作要点\n---\n汛前完成隐患排查。",
            )
            .await
            .expect("note should write");
        let query = KbQuery {
            keyword: "防汛".to_owned(),
            ..KbQuery::default()
        };
        let hits = provider.search(&query).await.expect("search should work");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "防汛工作要点");
    }

    #[tokio::test]
    async fn rejects_symlinked_notes() {
        let directory = tempdir().expect("temp dir should exist");
        let outside = tempdir().expect("outside dir should exist");
        fs::write(outside.path().join("secret.md"), "secret").expect("outside note should write");
        let provider =
            LocalVaultProvider::open(directory.path(), "vault").expect("vault should open");
        #[cfg(unix)]
        let link = std::os::unix::fs::symlink(
            outside.path().join("secret.md"),
            directory.path().join("secret.md"),
        );
        #[cfg(windows)]
        let link = std::os::windows::fs::symlink_file(
            outside.path().join("secret.md"),
            directory.path().join("secret.md"),
        );
        if link.is_err() {
            // Windows only grants SeCreateSymbolicLinkPrivilege to elevated / developer-mode
            // accounts; the traversal guard itself is covered by the path tests below.
            eprintln!("skipping symlink test: this account cannot create symbolic links");
            return;
        }
        assert!(matches!(
            provider.get("secret.md").await,
            Err(KbError::InvalidPath)
        ));
    }

    #[cfg(any(unix, windows))]
    #[tokio::test]
    async fn rejects_symlinked_parent_directories_when_writing() {
        let directory = tempdir().expect("temp dir should exist");
        let outside = tempdir().expect("outside dir should exist");
        let provider =
            LocalVaultProvider::open(directory.path(), "vault").expect("vault should open");
        #[cfg(unix)]
        let link = std::os::unix::fs::symlink(outside.path(), directory.path().join("linked"));
        #[cfg(windows)]
        let link =
            std::os::windows::fs::symlink_dir(outside.path(), directory.path().join("linked"));
        if link.is_err() {
            eprintln!("skipping symlink test: this account cannot create symbolic links");
            return;
        }

        assert!(matches!(
            provider.write("linked/escaped.md", "secret").await,
            Err(KbError::InvalidPath)
        ));
        assert!(!outside.path().join("escaped.md").exists());
    }

    #[test]
    fn rejects_windows_style_path_traversal_on_all_platforms() {
        assert!(matches!(
            normalize_relative_path(r"..\outside.md"),
            Err(KbError::InvalidPath)
        ));
        assert_eq!(
            normalize_relative_path(r"公文\2026\请示.md").unwrap(),
            "公文/2026/请示.md"
        );
    }

    #[tokio::test]
    async fn skips_unreadable_or_malformed_notes_instead_of_failing() {
        let directory = tempdir().expect("temp dir should exist");
        let provider =
            LocalVaultProvider::open(directory.path(), "vault").expect("vault should open");
        provider
            .write("公文/完好.md", "防汛工作部署会议纪要")
            .await
            .expect("note should write");
        // An unclosed frontmatter block: a very common real-world note defect.
        std::fs::write(
            directory.path().join("公文/坏笔记.md"),
            "---\ntitle: 未闭合\n正文",
        )
        .expect("broken note should write");
        // Non-UTF-8 bytes.
        std::fs::write(directory.path().join("公文/二进制.md"), [0xff, 0xfe, 0x00])
            .expect("binary note should write");

        let query = KbQuery {
            keyword: "防汛".to_owned(),
            ..KbQuery::default()
        };
        let hits = provider.search(&query).await.expect("search should work");
        assert_eq!(hits.len(), 1);
        let notes = provider
            .list_notes(None, 10)
            .await
            .expect("list should work");
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].path, "公文/完好.md");
        let links = provider
            .backlinks("政策/防汛.md")
            .await
            .expect("backlinks should work");
        assert!(links.is_empty(), "the note has no backlinks");
    }
}
