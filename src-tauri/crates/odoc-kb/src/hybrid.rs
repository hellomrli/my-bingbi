use crate::{Frontmatter, KbError, KbHit, KbProvider, KbQuery};
use std::collections::{BTreeMap, BTreeSet};

const PRIMARY_LIMIT: u32 = 80;
const GRAPH_SEEDS: usize = 20;
const GRAPH_LIMIT: usize = 80;
const RRF_K: f32 = 60.0;

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
    result.truncate(query.normalized_limit() as usize);
    Ok(result)
}

fn matches_path_prefix(path: &str, prefix: Option<&str>) -> bool {
    prefix.is_none_or(|prefix| path.replace('\\', "/").starts_with(prefix))
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
