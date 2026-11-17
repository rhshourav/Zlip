use crate::{Category, ItemId};
use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};

/// Only the first part of long items is searched, which bounds per-keystroke cost.
const SEARCH_PREFIX_CHARS: usize = 2048;

/// One searchable history entry, borrowed from the caller's index.
#[derive(Debug, Clone, Copy)]
pub struct Candidate<'a> {
    pub id: ItemId,
    pub text: &'a str,
    pub category: Category,
    /// Larger is newer. Used as the tie-breaker and for the empty query.
    pub recency: i64,
}

/// Reusable fuzzy searcher (`nucleo`, the matcher behind Helix).
pub struct Searcher {
    matcher: Matcher,
    buf: Vec<char>,
}

impl std::fmt::Debug for Searcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Searcher")
    }
}

impl Default for Searcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Searcher {
    #[must_use]
    pub fn new() -> Self {
        Self {
            matcher: Matcher::new(Config::DEFAULT),
            buf: Vec::new(),
        }
    }

    /// Rank candidates for `query`, best first, at most `limit` ids.
    ///
    /// * Empty query: newest first.
    /// * Several words must *all* match; smart-case (an upper-case letter makes
    ///   the match case-sensitive); accents are normalised.
    /// * `filter` scopes the search to one category without a second pass.
    pub fn search<'a>(
        &mut self,
        query: &str,
        candidates: impl IntoIterator<Item = Candidate<'a>>,
        filter: Option<Category>,
        limit: usize,
    ) -> Vec<ItemId> {
        let scoped = candidates
            .into_iter()
            .filter(|c| filter.is_none_or(|f| f == c.category));
        let query = query.trim();
        if query.is_empty() {
            let mut all: Vec<_> = scoped.collect();
            all.sort_by_key(|c| std::cmp::Reverse(c.recency));
            return all.into_iter().take(limit).map(|c| c.id).collect();
        }
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut scored: Vec<(u32, i64, ItemId)> = Vec::new();
        for c in scoped {
            let hay = prefix(c.text, SEARCH_PREFIX_CHARS);
            let hay = Utf32Str::new(hay, &mut self.buf);
            if let Some(score) = pattern.score(hay, &mut self.matcher) {
                scored.push((score, c.recency, c.id));
            }
        }
        scored.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
        scored
            .into_iter()
            .take(limit)
            .map(|(_, _, id)| id)
            .collect()
    }
}

fn prefix(s: &str, max_chars: usize) -> &str {
    s.char_indices().nth(max_chars).map_or(s, |(i, _)| &s[..i])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<(ItemId, &'static str, Category, i64)> {
        vec![
            (ItemId(1), "docker system prune -af", Category::Text, 10),
            (
                ItemId(2),
                "https://github.com/rhshourav/zlip",
                Category::Link,
                30,
            ),
            (ItemId(3), "SELECT * FROM users", Category::Code, 20),
            (ItemId(4), "Grocery list: milk, eggs", Category::Text, 40),
        ]
    }
    fn run(q: &str, f: Option<Category>) -> Vec<u64> {
        let data = items();
        let cands = data.iter().map(|(id, t, c, r)| Candidate {
            id: *id,
            text: t,
            category: *c,
            recency: *r,
        });
        Searcher::new()
            .search(q, cands, f, 50)
            .into_iter()
            .map(|i| i.0)
            .collect()
    }

    #[test]
    fn empty_query_is_newest_first() {
        assert_eq!(run("", None), vec![4, 2, 3, 1]);
    }

    #[test]
    fn fuzzy_subsequence_matches() {
        assert_eq!(run("dkr prn", None), vec![1]);
        assert_eq!(run("ghzlip", None), vec![2]);
    }

    #[test]
    fn all_words_must_match() {
        assert_eq!(run("select users", None), vec![3]);
        assert!(run("select milk", None).is_empty());
    }

    #[test]
    fn smart_case() {
        assert_eq!(
            run("select", None),
            vec![3],
            "lower-case query ignores case"
        );
        assert_eq!(run("SELECT", None), vec![3]);
        assert!(
            run("SELECt", None).is_empty(),
            "no lower-case t exists, and capitals force case-sensitivity"
        );
        assert!(
            run("Docker", None).is_empty(),
            "a capital makes it case-sensitive"
        );
    }

    #[test]
    fn category_filter_scopes_results() {
        assert_eq!(run("", Some(Category::Text)), vec![4, 1]);
        assert!(run("github", Some(Category::Code)).is_empty());
    }

    #[test]
    fn limit_is_respected() {
        let data = items();
        let cands = data.iter().map(|(id, t, c, r)| Candidate {
            id: *id,
            text: t,
            category: *c,
            recency: *r,
        });
        assert_eq!(Searcher::new().search("", cands, None, 2).len(), 2);
    }
}
