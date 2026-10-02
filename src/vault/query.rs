//! The retrieval queries: by tag, by term, and by wikilink-graph proximity.
//!
//! Each function is a pure read of the indices built in [`super::index`] and
//! returns ranked [`Hit`]s; rendering is [`super::render`]'s job.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use super::index::Vault;
use super::note::{Direction, Hit};
use super::text::{collect_terms, normalize_tag, score_terms};

impl Vault {
    /// Search notes by tag. A query tag also matches nested tags
    /// (`project` matches `project/alpha`).
    pub fn search_by_tag(&self, tags: &[String], match_all: bool, limit: usize) -> Vec<Hit> {
        let queries = clean_queries(tags, normalize_tag);
        if queries.is_empty() {
            return Vec::new();
        }

        let per_tag: Vec<BTreeSet<usize>> =
            queries.iter().map(|query| self.notes_with_tag(query)).collect();
        let scores = combine_tag_scores(&per_tag, match_all);

        let reason = format!("tag match: {}", queries.join(", "));
        let hits = scores
            .into_iter()
            .map(|(index, mut score)| {
                if queries
                    .iter()
                    .any(|query| self.note_at(index).title.to_lowercase().contains(query.as_str()))
                {
                    score += 2;
                }
                self.hit(index, score, reason.clone())
            });
        self.rank(hits, limit)
    }

    /// Search notes by free-text term across title, tags, headings, frontmatter
    /// and body. With `match_all`, every term must match the note.
    pub fn search_by_term(&self, terms: &[String], match_all: bool, limit: usize) -> Vec<Hit> {
        let queries = clean_queries(terms, |term| term.trim().to_lowercase());
        if queries.is_empty() {
            return Vec::new();
        }

        let mut scored: Vec<Hit> = Vec::new();
        for index in self.term_candidates(&queries) {
            let note = self.note_at(index);
            let mut total = 0u32;
            let mut matched_terms = 0usize;
            for query in &queries {
                let score = score_terms(note, query);
                if score > 0 {
                    matched_terms += 1;
                    total += score;
                }
            }
            if matched_terms == 0 || (match_all && matched_terms != queries.len()) {
                continue;
            }
            scored.push(self.hit(index, total, term_reason(&queries, matched_terms)));
        }
        self.rank(scored, limit)
    }

    /// Traverse the wikilink graph outwards from `name` up to `depth` hops.
    pub fn get_linked_notes(
        &self,
        name: &str,
        depth: usize,
        direction: Direction,
        limit: usize,
    ) -> Result<Vec<Hit>> {
        let start = self.resolve_one(name)?;
        let mut visited: BTreeSet<usize> = BTreeSet::from([start]);
        let mut frontier: Vec<usize> = vec![start];
        let mut scores: BTreeMap<usize, (u32, String)> = BTreeMap::new();

        for hop in 1..=depth.max(1) {
            let mut next: BTreeSet<usize> = BTreeSet::new();
            for index in &frontier {
                for (neighbour, kind) in self.neighbours(*index, direction) {
                    if neighbour == start || visited.contains(&neighbour) {
                        continue;
                    }
                    next.insert(neighbour);
                    let entry = scores.entry(neighbour).or_insert((0, String::new()));
                    entry.0 += (10 / hop as u32).max(1);
                    if entry.1.is_empty() {
                        entry.1 =
                            format!("{kind} of '{}' at hop {hop}", self.note_at(*index).rel_path);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            visited.extend(next.iter().copied());
            frontier = next.into_iter().collect();
        }

        let hits = scores
            .into_iter()
            .map(|(index, (score, reason))| self.hit(index, score, reason));
        Ok(self.rank(hits, limit))
    }

    /// Notes that contain at least one of the query words, from the inverted
    /// index, so a term search never scans every note body.
    fn term_candidates(&self, queries: &[String]) -> BTreeSet<usize> {
        let mut candidates = BTreeSet::new();
        for query in queries {
            let mut words = BTreeSet::new();
            collect_terms(query, &mut words);
            for word in words {
                candidates.extend(self.notes_with_word(&word).iter().copied());
            }
        }
        candidates
    }
}

/// Normalise query strings, dropping the ones that end up empty.
fn clean_queries(queries: &[String], normalize: impl Fn(&str) -> String) -> Vec<String> {
    queries
        .iter()
        .map(|query| normalize(query))
        .filter(|query| !query.is_empty())
        .collect()
}

/// Score a tag query: every tag must match (`match_all`) or any tag may match.
///
/// A note that satisfies the whole query is worth `10 × tags`, while the
/// any-of case accumulates `10` per matched tag, so a note matching more of the
/// requested tags always ranks higher.
fn combine_tag_scores(per_tag: &[BTreeSet<usize>], match_all: bool) -> BTreeMap<usize, u32> {
    let mut scores: BTreeMap<usize, u32> = BTreeMap::new();
    if match_all {
        let mut intersection = per_tag.first().cloned().unwrap_or_default();
        for set in per_tag.iter().skip(1) {
            intersection = intersection.intersection(set).copied().collect();
        }
        let score = 10 * per_tag.len() as u32;
        for index in intersection {
            scores.insert(index, score);
        }
    } else {
        for set in per_tag {
            for index in set {
                *scores.entry(*index).or_insert(0) += 10;
            }
        }
    }
    scores
}

/// Human-readable reason for a term hit.
///
/// Multi-word queries report how many terms matched rather than listing the
/// matched terms, which keeps the reason short inside a rendered result.
fn term_reason(queries: &[String], matched_terms: usize) -> String {
    if matched_terms == 1 {
        return format!("term match: {}", queries[0]);
    }
    format!("term match ({matched_terms}/{} terms)", queries.len())
}
