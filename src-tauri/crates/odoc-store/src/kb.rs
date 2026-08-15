use crate::config::unix_millis;
use crate::{DocumentStore, StoreError};
use odoc_kb::{Frontmatter, KbNote};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

const MAX_CACHE_LIMIT: u32 = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct KbCacheHit {
    pub vault: String,
    pub path: String,
    pub title: String,
    pub snippet: String,
    pub score: f32,
    pub frontmatter: Frontmatter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct KbIndexState {
    pub vault: String,
    pub provider_kind: String,
    #[ts(type = "number")]
    pub last_cursor: i64,
    #[ts(type = "number")]
    pub note_count: u32,
    #[ts(type = "number")]
    pub chunk_count: u32,
    pub status: String,
    pub error_message: Option<String>,
    #[ts(type = "number")]
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct CrashEvent {
    #[ts(type = "number")]
    pub id: u64,
    pub kind: String,
    pub message: String,
    pub context_json: String,
    #[ts(type = "number")]
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct UpdateState {
    pub current_version: String,
    pub available_version: Option<String>,
    pub download_url: Option<String>,
    #[ts(type = "number | null")]
    pub checked_at: Option<i64>,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct PackInstallRecord {
    pub pack_id: String,
    pub source_path: String,
    pub enabled: bool,
    #[ts(type = "number")]
    pub installed_at: i64,
    #[ts(type = "number")]
    pub updated_at: i64,
}

// `total_money` is a float, so this type is comparable but not `Eq`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename_all = "camelCase")]
pub struct CostSummary {
    #[ts(type = "number")]
    pub total_tokens: u64,
    pub total_money: f64,
    #[ts(type = "number")]
    pub session_count: u32,
}

impl DocumentStore {
    /// Upserts a provider note and its deterministic chunks into the local cache.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the note cannot be serialized or the cache write fails.
    pub fn cache_kb_note(&self, note: &KbNote) -> Result<(), StoreError> {
        let now = unix_millis()?;
        let modified_at = note.modified_at.unwrap_or(now);
        let hash = content_hash(&note.content);
        let frontmatter = serde_json::to_string(&note.frontmatter)?;
        let tags = serde_json::to_string(&note.tags)?;
        let links = serde_json::to_string(&note.links)?;
        self.connection.execute(
            "INSERT INTO kb_notes(path, vault, title, frontmatter_json, mtime, hash, content, tags_json, links_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(vault, path) DO UPDATE SET title=excluded.title,
               frontmatter_json=excluded.frontmatter_json, mtime=excluded.mtime, hash=excluded.hash,
               content=excluded.content, tags_json=excluded.tags_json, links_json=excluded.links_json",
            params![note.path, note.vault, note.title, frontmatter, modified_at, hash, note.content, tags, links],
        )?;
        self.connection.execute(
            "DELETE FROM kb_fts WHERE vault = ?1 AND path = ?2",
            params![note.vault, note.path],
        )?;
        self.connection.execute(
            "INSERT INTO kb_fts(vault, path, title, content) VALUES (?1, ?2, ?3, ?4)",
            params![note.vault, note.path, note.title, note.content],
        )?;
        self.connection.execute(
            "DELETE FROM kb_chunks WHERE vault = ?1 AND path = ?2",
            params![note.vault, note.path],
        )?;
        for (seq, chunk) in chunk_text(&note.content).into_iter().enumerate() {
            self.connection.execute(
                "INSERT INTO kb_chunks(vault, path, seq, text) VALUES (?1, ?2, ?3, ?4)",
                params![
                    note.vault,
                    note.path,
                    i64::try_from(seq).unwrap_or(i64::MAX),
                    chunk
                ],
            )?;
        }
        Ok(())
    }

    /// Drops a note and its derived chunks and full-text rows from the cache.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when a cache delete fails.
    pub fn remove_cached_kb_note(&self, vault: &str, path: &str) -> Result<(), StoreError> {
        self.connection.execute(
            "DELETE FROM kb_notes WHERE vault = ?1 AND path = ?2",
            params![vault, path],
        )?;
        self.connection.execute(
            "DELETE FROM kb_fts WHERE vault = ?1 AND path = ?2",
            params![vault, path],
        )?;
        self.connection.execute(
            "DELETE FROM kb_chunks WHERE vault = ?1 AND path = ?2",
            params![vault, path],
        )?;
        Ok(())
    }

    /// Runs the offline full-text search over cached notes, applying path and frontmatter filters.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query fails or a cached row cannot be decoded.
    pub fn search_cached_kb(
        &self,
        vault: &str,
        keyword: &str,
        path_prefix: Option<&str>,
        filters: &Frontmatter,
        limit: u32,
    ) -> Result<Vec<KbCacheHit>, StoreError> {
        let result_limit = limit.clamp(1, MAX_CACHE_LIMIT);
        let candidate_limit = if filters.is_empty() && path_prefix.is_none() {
            result_limit
        } else {
            result_limit.saturating_mul(4).min(MAX_CACHE_LIMIT)
        };
        let candidate_limit = i64::from(candidate_limit);
        let path_prefix = path_prefix.unwrap_or_default();
        if keyword.trim().is_empty() {
            let mut statement = self.connection.prepare(
                "SELECT n.vault, n.path, n.title, substr(n.content, 1, 320), n.frontmatter_json
                 FROM kb_notes n
                 WHERE n.vault = ?1 AND (?2 = '' OR substr(n.path, 1, length(?2)) = ?2)
                 ORDER BY n.mtime DESC, n.path LIMIT ?3",
            )?;
            let rows =
                statement.query_map(params![vault, path_prefix, candidate_limit], |row| {
                    let frontmatter_json: String = row.get(4)?;
                    Ok(KbCacheHit {
                        vault: row.get(0)?,
                        path: row.get(1)?,
                        title: row.get(2)?,
                        snippet: row.get(3)?,
                        score: 0.0,
                        frontmatter: serde_json::from_str(&frontmatter_json).unwrap_or_default(),
                    })
                })?;
            let hits = rows
                .collect::<Result<Vec<_>, _>>()
                .map_err(StoreError::from)?;
            return Ok(filter_cache_hits(hits, filters, result_limit));
        }
        let query = fts_query(keyword);
        let mut statement = self.connection.prepare(
            "SELECT f.vault, f.path, f.title, snippet(kb_fts, 3, '', '', '…', 18),
                    bm25(kb_fts), n.frontmatter_json
             FROM kb_fts f LEFT JOIN kb_notes n ON n.vault = f.vault AND n.path = f.path
             WHERE f.vault = ?1 AND kb_fts MATCH ?2
               AND (?3 = '' OR substr(f.path, 1, length(?3)) = ?3)
             ORDER BY bm25(kb_fts) LIMIT ?4",
        )?;
        let rows =
            statement.query_map(params![vault, query, path_prefix, candidate_limit], |row| {
                let frontmatter_json: Option<String> = row.get(5)?;
                let frontmatter = frontmatter_json
                    .as_deref()
                    .and_then(|value| serde_json::from_str(value).ok())
                    .unwrap_or_default();
                let score: f64 = row.get(4)?;
                Ok(KbCacheHit {
                    vault: row.get(0)?,
                    path: row.get(1)?,
                    title: row.get(2)?,
                    snippet: row.get(3)?,
                    score: relevance_score(score),
                    frontmatter,
                })
            })?;
        let hits = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)?;
        Ok(filter_cache_hits(hits, filters, result_limit))
    }

    /// Reads one cached note, or `None` when the vault does not hold that path.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query fails or the cached row cannot be decoded.
    pub fn cached_kb_note(&self, vault: &str, path: &str) -> Result<Option<KbNote>, StoreError> {
        self.connection
            .query_row(
                "SELECT title, content, frontmatter_json, tags_json, links_json, mtime
                 FROM kb_notes WHERE vault = ?1 AND path = ?2",
                params![vault, path],
                |row| {
                    let frontmatter: String = row.get(2)?;
                    let tags: String = row.get(3)?;
                    let links: String = row.get(4)?;
                    Ok(KbNote {
                        vault: vault.to_owned(),
                        path: path.to_owned(),
                        title: row.get(0)?,
                        content: row.get(1)?,
                        frontmatter: serde_json::from_str(&frontmatter).unwrap_or_default(),
                        tags: serde_json::from_str(&tags).unwrap_or_default(),
                        links: serde_json::from_str(&links).unwrap_or_default(),
                        modified_at: row.get(5)?,
                    })
                },
            )
            .optional()
            .map_err(StoreError::from)
    }

    /// Returns the cached note and chunk counts for a vault, used by the index progress display.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when a count query fails.
    pub fn kb_cache_counts(&self, vault: &str) -> Result<(u32, u32), StoreError> {
        let note_count = self.connection.query_row(
            "SELECT COUNT(*) FROM kb_notes WHERE vault = ?1",
            params![vault],
            |row| row.get::<_, i64>(0),
        )?;
        let chunk_count = self.connection.query_row(
            "SELECT COUNT(*) FROM kb_chunks WHERE vault = ?1",
            params![vault],
            |row| row.get::<_, i64>(0),
        )?;
        Ok((
            note_count.try_into().unwrap_or(u32::MAX),
            chunk_count.try_into().unwrap_or(u32::MAX),
        ))
    }

    /// Persists the incremental index cursor and status for a vault.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    pub fn save_kb_index_state(&self, state: &KbIndexState) -> Result<(), StoreError> {
        self.connection.execute(
            "INSERT INTO kb_index_state(vault, provider_kind, last_cursor, note_count, chunk_count, status, error_message, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(vault) DO UPDATE SET provider_kind=excluded.provider_kind,
              last_cursor=excluded.last_cursor, note_count=excluded.note_count, chunk_count=excluded.chunk_count,
              status=excluded.status, error_message=excluded.error_message, updated_at=excluded.updated_at",
            params![state.vault, state.provider_kind, state.last_cursor, state.note_count, state.chunk_count,
                state.status, state.error_message, state.updated_at],
        )?;
        Ok(())
    }

    /// Reads the index cursor for a vault, or `None` before the first indexing run.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query fails.
    pub fn kb_index_state(&self, vault: &str) -> Result<Option<KbIndexState>, StoreError> {
        self.connection.query_row(
            "SELECT vault, provider_kind, last_cursor, note_count, chunk_count, status, error_message, updated_at
             FROM kb_index_state WHERE vault = ?1", params![vault], |row| {
                Ok(KbIndexState { vault: row.get(0)?, provider_kind: row.get(1)?, last_cursor: row.get(2)?,
                    note_count: row.get::<_, i64>(3)?.try_into().unwrap_or(0), chunk_count: row.get::<_, i64>(4)?.try_into().unwrap_or(0),
                    status: row.get(5)?, error_message: row.get(6)?, updated_at: row.get(7)? })
            }).optional().map_err(StoreError::from)
    }

    /// Removes every cached note, chunk, full-text row and index cursor for a vault.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when a delete fails.
    pub fn clear_kb_index(&self, vault: &str) -> Result<(), StoreError> {
        self.connection
            .execute("DELETE FROM kb_notes WHERE vault = ?1", params![vault])?;
        self.connection
            .execute("DELETE FROM kb_chunks WHERE vault = ?1", params![vault])?;
        self.connection
            .execute("DELETE FROM kb_fts WHERE vault = ?1", params![vault])?;
        self.connection.execute(
            "DELETE FROM kb_index_state WHERE vault = ?1",
            params![vault],
        )?;
        Ok(())
    }

    /// Appends one local crash record. Crash data stays on this machine; nothing is reported out.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the clock is unavailable or the insert fails.
    pub fn record_crash_event(
        &self,
        kind: &str,
        message: &str,
        context_json: &str,
    ) -> Result<(), StoreError> {
        self.connection.execute("INSERT INTO crash_events(kind, message, context_json, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![kind, message, context_json, unix_millis()?])?;
        Ok(())
    }

    /// Lists the most recent crash records, newest first, capped at 100 rows.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query fails or a row cannot be decoded.
    pub fn list_crash_events(&self, limit: u32) -> Result<Vec<CrashEvent>, StoreError> {
        let mut statement = self.connection.prepare("SELECT id, kind, message, context_json, created_at FROM crash_events ORDER BY id DESC LIMIT ?1")?;
        let rows = statement.query_map(params![i64::from(limit.clamp(1, 100))], |row| {
            Ok(CrashEvent {
                id: row.get::<_, i64>(0)?.try_into().unwrap_or(0),
                kind: row.get(1)?,
                message: row.get(2)?,
                context_json: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(StoreError::from)
    }

    /// Stores the result of the latest update check.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    pub fn save_update_state(&self, state: &UpdateState) -> Result<(), StoreError> {
        self.connection.execute("INSERT INTO update_state(id, current_version, available_version, download_url, checked_at, status)
            VALUES (1, ?1, ?2, ?3, ?4, ?5) ON CONFLICT(id) DO UPDATE SET current_version=excluded.current_version,
            available_version=excluded.available_version, download_url=excluded.download_url, checked_at=excluded.checked_at, status=excluded.status",
            params![state.current_version, state.available_version, state.download_url, state.checked_at, state.status])?;
        Ok(())
    }

    /// Reads the stored update state, falling back to an unknown state for a fresh install.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the query fails or the row cannot be decoded.
    pub fn update_state(&self, current_version: &str) -> Result<UpdateState, StoreError> {
        self.connection.query_row("SELECT current_version, available_version, download_url, checked_at, status FROM update_state WHERE id=1", [], |row| Ok(UpdateState {
            current_version: row.get(0)?, available_version: row.get(1)?, download_url: row.get(2)?, checked_at: row.get(3)?, status: row.get(4)?
        })).optional().map(|value| value.unwrap_or(UpdateState { current_version: current_version.to_owned(), available_version: None, download_url: None, checked_at: None, status: "unknown".to_owned() })).map_err(StoreError::from)
    }

    /// Totals token and money spend across every recorded session.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the aggregate query fails.
    pub fn cost_summary(&self) -> Result<CostSummary, StoreError> {
        self.connection.query_row("SELECT COALESCE(SUM(cost_tokens),0), COALESCE(SUM(cost_money),0), COUNT(*) FROM sessions", [], |row| Ok(CostSummary {
            total_tokens: row.get::<_, i64>(0)?.try_into().unwrap_or(0), total_money: row.get(1)?, session_count: row.get::<_, i64>(2)?.try_into().unwrap_or(0)
        })).map_err(StoreError::from)
    }

    /// Records or updates one knowledge-pack installation.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] when the write fails.
    pub fn upsert_pack_installation(&self, record: &PackInstallRecord) -> Result<(), StoreError> {
        self.connection.execute("INSERT INTO pack_installations(pack_id, source_path, enabled, installed_at, updated_at) VALUES (?1,?2,?3,?4,?5)
            ON CONFLICT(pack_id) DO UPDATE SET source_path=excluded.source_path, enabled=excluded.enabled, updated_at=excluded.updated_at",
            params![record.pack_id, record.source_path, record.enabled, record.installed_at, record.updated_at])?;
        Ok(())
    }
}

/// Converts an FTS5 bm25 score into the positive relevance value the UI ranks by.
///
/// bm25 returns smaller (more negative) numbers for better matches, so the sign is flipped. The
/// narrowing to `f32` is deliberate: only the ordering matters, not the exact magnitude.
#[allow(
    clippy::cast_possible_truncation,
    reason = "relevance ordering does not need f64 precision"
)]
fn relevance_score(bm25: f64) -> f32 {
    (-bm25).max(0.0) as f32
}

fn content_hash(content: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in content.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn chunk_text(content: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    for character in content.chars() {
        current.push(character);
        if current.chars().count() >= 800 {
            chunks.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() || chunks.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn fts_query(keyword: &str) -> String {
    keyword
        .split_whitespace()
        .map(|term| format!("\"{}\"", term.replace('"', "")))
        .filter(|term| term != "\"\"")
        .collect::<Vec<_>>()
        .join(" AND ")
}

fn filter_cache_hits(
    mut hits: Vec<KbCacheHit>,
    filters: &Frontmatter,
    limit: u32,
) -> Vec<KbCacheHit> {
    if !filters.is_empty() {
        hits.retain(|hit| {
            filters
                .iter()
                .all(|(key, value)| hit.frontmatter.get(key) == Some(value))
        });
    }
    hits.truncate(limit as usize);
    hits
}
