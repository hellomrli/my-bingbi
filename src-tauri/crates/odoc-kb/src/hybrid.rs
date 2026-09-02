use crate::{Frontmatter, KbError, KbHit, KbProvider, KbQuery};
use odoc_llm::{LlmProvider, SecurityLevel};
use std::collections::{BTreeMap, BTreeSet};

const PRIMARY_LIMIT: u32 = 80;
const GRAPH_SEEDS: usize = 20;
const GRAPH_LIMIT: usize = 80;
const RRF_K: f32 = 60.0;
/// Weight of the cosine similarity between a hit and the query in the fused score.
const SEMANTIC_WEIGHT: f32 = 0.25;
const SEMANTIC_TEXT_CHARS: usize = 480;

/// Performs deterministic three-level retrieval: provider full-text search, one-hop link
/// expansion, then reciprocal-rank fusion. The provider remains responsible for security and
/// frontmatter filtering; this function never sends a new query outside the original vault.
///
/// # Errors
///
/// Returns an error when the query is invalid or the provider's primary search fails. Link
/// expansion failures are treated as "no extra hits" rather than aborting the search.
pub async fn hybrid_search(
    provider: &dyn KbProvider,
    query: &KbQuery,
) -> Result<Vec<KbHit>, KbError> {
    // No semantic re-ranking, so the security level is irrelevant; a placeholder keeps the
    // call site honest about the fact that nothing is sent outside the vault.
    hybrid_search_with_semantic(
        provider,
        query,
        SecurityLevel::Public,
        None::<&dyn LlmProvider>,
    )
    .await
}

/// [`hybrid_search`] with an optional semantic re-ranking pass.
///
/// When `semantic` is supplied and supports embeddings, every fused hit is embedded together
/// with the query and its cosine similarity is folded into the score. Embedding failures are
/// silently ignored — semantic re-ranking is an enhancement, never a failure mode. The
/// `security_level` is forwarded to the embedding provider so restricted content is never
/// sent to a remote endpoint.
///
/// # Errors
///
/// Returns an error when the query is invalid or the provider's primary search fails. Link
/// expansion failures are treated as "no extra hits" rather than aborting the search.
pub async fn hybrid_search_with_semantic<P: LlmProvider + ?Sized>(
    provider: &dyn KbProvider,
    query: &KbQuery,
    security_level: SecurityLevel,
    semantic: Option<&P>,
) -> Result<Vec<KbHit>, KbError> {
    query.validate()?;
    let mut primary_query = query.clone();
    primary_query.limit = query.normalized_limit().max(PRIMARY_LIMIT);
    let primary = provider.search(&primary_query).await?;
    let mut hits: BTreeMap<(String, String), KbHit> = BTreeMap::new();
    let mut scores: BTreeMap<(String, String), f32> = BTreeMap::new();
    for (rank, hit) in primary.iter().enumerate() {
        let key = (hit.vault.clone(), hit.path.clone());
        scores.insert(
            key.clone(),
            1.0 / (RRF_K + rank_score(rank) + 1.0) + hit.score.max(0.0) * 0.01,
        );
        hits.insert(key, hit.clone());
    }

    let mut graph_paths = BTreeSet::new();
    for seed in primary.iter().take(GRAPH_SEEDS) {
        for path in provider.outlinks(&seed.path).await.unwrap_or_default() {
            graph_paths.insert((seed.vault.clone(), path));
        }
        for path in provider.backlinks(&seed.path).await.unwrap_or_default() {
            graph_paths.insert((seed.vault.clone(), path));
        }
        if graph_paths.len() >= GRAPH_LIMIT {
            break;
        }
    }

    for (rank, (vault, path)) in graph_paths.into_iter().take(GRAPH_LIMIT).enumerate() {
        if !matches_path_prefix(&path, query.path_prefix.as_deref()) {
            continue;
        }
        let key = (vault.clone(), path.clone());
        let graph_score = 0.65 / (RRF_K + rank_score(rank) + 1.0);
        *scores.entry(key.clone()).or_insert(0.0) += graph_score;
        if hits.contains_key(&key) {
            continue;
        }
        if let Ok(note) = provider.get(&path).await {
            if !matches_frontmatter(&note.frontmatter, &query.filters) {
                continue;
            }
            hits.insert(
                key,
                KbHit {
                    vault: note.vault,
                    path: note.path,
                    title: note.title,
                    snippet: note.content.chars().take(480).collect(),
                    score: 0.0,
                    frontmatter: note.frontmatter,
                },
            );
        }
    }

    let mut result = hits
        .into_iter()
        .filter_map(|(key, mut hit)| {
            if !matches_path_prefix(&hit.path, query.path_prefix.as_deref())
                || !matches_frontmatter(&hit.frontmatter, &query.filters)
            {
                return None;
            }
            hit.score = scores.get(&key).copied().unwrap_or(0.0);
            Some(hit)
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.vault.cmp(&right.vault))
    });
    if let Some(semantic) = semantic {
        fold_semantic_scores(semantic, query, security_level, &mut result).await;
    }
    result.truncate(query.normalized_limit() as usize);
    Ok(result)
}

/// Embeds the query and the top hits, then adds the weighted cosine similarity to each score.
///
/// Any embedding failure drops the whole pass; the lexical fusion remains authoritative. The
/// security level travels with the texts so a restricted document cannot leak through a remote
/// embedding endpoint.
async fn fold_semantic_scores<P: LlmProvider + ?Sized>(
    semantic: &P,
    query: &KbQuery,
    security_level: SecurityLevel,
    result: &mut [KbHit],
) {
    let Some(query_vector) = semantic
        .embed(std::slice::from_ref(&query.keyword), security_level)
        .await
        .ok()
        .and_then(|vectors| vectors.into_iter().next())
    else {
        return;
    };
    let texts = result
        .iter()
        .map(|hit| {
            hit.snippet
                .chars()
                .take(SEMANTIC_TEXT_CHARS)
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    let Ok(vectors) = semantic.embed(&texts, security_level).await else {
        return;
    };
    if vectors.len() != result.len() {
        return;
    }
    for (hit, vector) in result.iter_mut().zip(vectors) {
        hit.score += cosine_similarity(&query_vector, &vector) * SEMANTIC_WEIGHT;
    }
    result.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.vault.cmp(&right.vault))
    });
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    let dot = left
        .iter()
        .zip(right)
        .fold(0.0_f32, |acc, (a, b)| acc + a * b);
    let left_norm = left
        .iter()
        .fold(0.0_f32, |acc, value| acc + value * value)
        .sqrt();
    let right_norm = right
        .iter()
        .fold(0.0_f32, |acc, value| acc + value * value)
        .sqrt();
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm * right_norm)
    }
}

fn matches_path_prefix(path: &str, prefix: Option<&str>) -> bool {
    crate::types::path_matches_prefix(path, prefix)
}

/// Converts a zero-based rank into its fusion weight. Ranks are bounded by the retrieval limits,
/// so the saturating conversion never loses a rank that can actually occur.
fn rank_score(rank: usize) -> f32 {
    f32::from(u16::try_from(rank).unwrap_or(u16::MAX))
}

fn matches_frontmatter(frontmatter: &Frontmatter, filters: &Frontmatter) -> bool {
    filters
        .iter()
        .all(|(key, value)| frontmatter.get(key) == Some(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChangeEvent, KbNote};
    use async_trait::async_trait;
    use odoc_llm::{Capabilities, ChatDelta, ChatRequest, ChatResponse, LlmError, LlmProvider};
    use tokio::sync::mpsc::Sender;

    struct FakeKb;

    #[async_trait]
    impl KbProvider for FakeKb {
        async fn search(&self, _query: &KbQuery) -> Result<Vec<KbHit>, KbError> {
            Ok(vec![
                KbHit {
                    vault: "v".into(),
                    path: "公文/a.md".into(),
                    title: "甲".into(),
                    snippet: "防汛演练方案".into(),
                    score: 0.9,
                    frontmatter: Frontmatter::new(),
                },
                KbHit {
                    vault: "v".into(),
                    path: "公文/b.md".into(),
                    title: "乙".into(),
                    snippet: "食堂菜谱".into(),
                    score: 0.8,
                    frontmatter: Frontmatter::new(),
                },
            ])
        }
        async fn get(&self, _path: &str) -> Result<KbNote, KbError> {
            unreachable!()
        }
        async fn backlinks(&self, _path: &str) -> Result<Vec<String>, KbError> {
            Ok(Vec::new())
        }
        async fn outlinks(&self, _path: &str) -> Result<Vec<String>, KbError> {
            Ok(Vec::new())
        }
        async fn write(&self, _path: &str, _content: &str) -> Result<(), KbError> {
            Ok(())
        }
        async fn patch_frontmatter(
            &self,
            _path: &str,
            _fields: &Frontmatter,
        ) -> Result<(), KbError> {
            Ok(())
        }
        fn watch(&self) -> Option<tokio::sync::broadcast::Receiver<ChangeEvent>> {
            None
        }
    }

    /// Embeds the query and the first text identically, everything else orthogonally.
    struct SemanticProvider;

    #[async_trait]
    impl LlmProvider for SemanticProvider {
        fn id(&self) -> &'static str {
            "semantic"
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                streaming: false,
                json_mode: false,
                tool_calling: false,
                embeddings: true,
                context_window: None,
                is_local: true,
            }
        }
        async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, LlmError> {
            Ok(ChatResponse {
                content: String::new(),
                finish_reason: None,
                usage: None,
            })
        }
        async fn chat_stream(
            &self,
            _request: ChatRequest,
            _sender: Sender<ChatDelta>,
        ) -> Result<(), LlmError> {
            Ok(())
        }
        async fn embed(
            &self,
            texts: &[String],
            _security_level: SecurityLevel,
        ) -> Result<Vec<Vec<f32>>, LlmError> {
            Ok(texts
                .iter()
                .map(|text| {
                    if text.contains("防汛") {
                        vec![1.0, 0.0]
                    } else {
                        vec![0.0, 1.0]
                    }
                })
                .collect())
        }
    }

    /// A provider whose embedding endpoint is missing.
    struct NoEmbedProvider;

    #[async_trait]
    impl LlmProvider for NoEmbedProvider {
        fn id(&self) -> &'static str {
            "no-embed"
        }
        fn capabilities(&self) -> Capabilities {
            Capabilities {
                streaming: false,
                json_mode: false,
                tool_calling: false,
                embeddings: false,
                context_window: None,
                is_local: true,
            }
        }
        async fn chat(&self, _request: ChatRequest) -> Result<ChatResponse, LlmError> {
            Ok(ChatResponse {
                content: String::new(),
                finish_reason: None,
                usage: None,
            })
        }
        async fn chat_stream(
            &self,
            _request: ChatRequest,
            _sender: Sender<ChatDelta>,
        ) -> Result<(), LlmError> {
            Ok(())
        }
    }

    fn query() -> KbQuery {
        KbQuery {
            keyword: "防汛".into(),
            ..KbQuery::default()
        }
    }

    #[tokio::test]
    async fn semantic_rerank_promotes_the_most_similar_hit() {
        let lexical = hybrid_search(&FakeKb, &query())
            .await
            .expect("search should work");
        assert_eq!(lexical[0].path, "公文/a.md");

        let reranked = hybrid_search_with_semantic(
            &FakeKb,
            &query(),
            SecurityLevel::Internal,
            Some(&SemanticProvider),
        )
        .await
        .expect("search should work");
        assert_eq!(reranked[0].path, "公文/a.md");
        assert!(
            reranked[0].score > lexical[0].score,
            "semantic similarity should lift the matching hit"
        );
    }

    #[tokio::test]
    async fn missing_embeddings_fall_back_to_lexical_fusion() {
        let with = hybrid_search_with_semantic(
            &FakeKb,
            &query(),
            SecurityLevel::Internal,
            Some(&NoEmbedProvider),
        )
        .await
        .expect("search should work");
        let without = hybrid_search(&FakeKb, &query())
            .await
            .expect("search should work");
        assert_eq!(with, without);
    }
}
