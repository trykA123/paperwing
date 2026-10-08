use crate::settings::FinderMatching;
use nucleo::pattern::{Atom, AtomKind, CaseMatching, Normalization, Pattern};
use nucleo::{Config, Matcher, Utf32Str};
use serde::{Deserialize, Serialize};

pub const DEFAULT_RESULTS: usize = 100;
const MAX_RESULTS: usize = 500;

#[derive(Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FinderRequest {
    pub repos: Vec<String>,
    pub query: String,
    pub max_results: Option<usize>,
}

pub fn validate(request: &FinderRequest) -> Result<usize, String> {
    if request.repos.is_empty() || request.repos.len() > crate::search::MAX_REPOS {
        return Err(format!(
            "Choose between 1 and {} repositories",
            crate::search::MAX_REPOS
        ));
    }
    if request.query.len() > 4096 || request.query.chars().any(char::is_control) {
        return Err("Finder query must be at most 4096 bytes without control characters".into());
    }
    let limit = request.max_results.unwrap_or(DEFAULT_RESULTS);
    if !(1..=MAX_RESULTS).contains(&limit) {
        return Err("Finder result limit is out of range".into());
    }
    Ok(limit)
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FileMatch {
    pub repo: String,
    pub path: String,
    pub score: u32,
    pub positions: Vec<u32>,
}

enum Needle {
    Fuzzy(Pattern),
    Exact(Atom),
}

pub struct FileMatcher {
    needle: Needle,
    matcher: Matcher,
    chars: Vec<char>,
    positions: Vec<u32>,
}

impl FileMatcher {
    pub fn new(query: &str, matching: FinderMatching) -> Self {
        let needle = match matching {
            FinderMatching::Fuzzy => Needle::Fuzzy(Pattern::new(
                query,
                CaseMatching::Ignore,
                Normalization::Never,
                AtomKind::Fuzzy,
            )),
            FinderMatching::ExactSubstring => Needle::Exact(Atom::new(
                query,
                CaseMatching::Ignore,
                Normalization::Never,
                AtomKind::Substring,
                false,
            )),
        };
        Self {
            needle,
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
            chars: Vec::new(),
            positions: Vec::new(),
        }
    }

    pub fn find(&mut self, repo: &str, path: String) -> Option<FileMatch> {
        self.chars.clear();
        self.chars.extend(path.chars());
        self.positions.clear();
        let haystack = Utf32Str::Unicode(&self.chars);
        let score = match &self.needle {
            Needle::Fuzzy(pattern) => {
                pattern.indices(haystack, &mut self.matcher, &mut self.positions)?
            }
            Needle::Exact(atom) => {
                u32::from(atom.indices(haystack, &mut self.matcher, &mut self.positions)?)
            }
        };
        self.positions.sort_unstable();
        self.positions.dedup();
        let positions = utf16_positions(&self.chars, &self.positions);
        Some(FileMatch {
            repo: repo.into(),
            path,
            score,
            positions,
        })
    }
}

fn utf16_positions(chars: &[char], matched: &[u32]) -> Vec<u32> {
    let mut offset = 0;
    let mut positions = Vec::new();
    for (index, c) in chars.iter().enumerate() {
        let length = c.len_utf16() as u32;
        if matched.binary_search(&(index as u32)).is_ok() {
            positions.extend(offset..offset + length);
        }
        offset += length;
    }
    positions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_matches_terms_and_exact_matching_keeps_whitespace_literal() {
        let mut fuzzy = FileMatcher::new("sc mf", FinderMatching::Fuzzy);
        assert!(fuzzy.find("repo", "src/my-file.rs".into()).is_some());
        let mut exact = FileMatcher::new("sc mf", FinderMatching::ExactSubstring);
        assert!(exact.find("repo", "src/my-file.rs".into()).is_none());
        let mut literal = FileMatcher::new("my-file", FinderMatching::ExactSubstring);
        assert_eq!(
            literal
                .find("repo", "src/my-file.rs".into())
                .unwrap()
                .positions,
            (4..11).collect::<Vec<_>>()
        );
    }

    #[test]
    fn matched_positions_cover_utf16_surrogates_and_unicode_case() {
        let mut matcher = FileMatcher::new("Ș", FinderMatching::Fuzzy);
        assert_eq!(
            matcher.find("repo", "😀ș.txt".into()).unwrap().positions,
            [2]
        );
        let mut matcher = FileMatcher::new("😀", FinderMatching::ExactSubstring);
        assert_eq!(
            matcher.find("repo", "😀ș.txt".into()).unwrap().positions,
            [0, 1]
        );
    }

    #[test]
    fn empty_queries_match_and_invalid_requests_are_rejected() {
        let mut matcher = FileMatcher::new("", FinderMatching::Fuzzy);
        assert!(matcher
            .find("repo", "a.txt".into())
            .unwrap()
            .positions
            .is_empty());
        let mut exact = FileMatcher::new("", FinderMatching::ExactSubstring);
        assert!(exact
            .find("repo", "a.txt".into())
            .unwrap()
            .positions
            .is_empty());
        let mut request = FinderRequest {
            repos: vec!["repo".into()],
            ..Default::default()
        };
        assert_eq!(validate(&request).unwrap(), 100);
        request.max_results = Some(0);
        assert!(validate(&request).is_err());
        request.max_results = None;
        request.query = "x\n".into();
        assert!(validate(&request).is_err());
        request.query.clear();
        request.repos.clear();
        assert!(validate(&request).is_err());
    }
}
