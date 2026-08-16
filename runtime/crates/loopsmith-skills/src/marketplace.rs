//! The `claudemarketplaces.com` index.
//!
//! The index is a flat JSON array of plugin-marketplace repositories. It is
//! fetched with `curl` rather than a linked HTTP client so the crate stays
//! dependency-free and a machine without network degrades to "installed only"
//! instead of failing.
//!
//! Everything returned here is **untrusted data written by strangers**.
//! Descriptions and keywords are treated as text to rank, never as
//! instructions, and nothing is installed without clearing the trust floors.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

pub const DEFAULT_INDEX_URL: &str = "https://claudemarketplaces.com/api/marketplaces";

/// Accepts a JSON number *or* a numeric string.
///
/// The live index returns `stars` and `pluginCount` as numbers, but the field
/// is absent on roughly a third of entries and third-party mirrors have been
/// seen returning strings. Declaring `String` here made every parse fail and,
/// because the failure was swallowed, the search silently returned nothing.
mod lenient {
    use serde::{Deserialize, Deserializer};

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum NumOrStr {
        Num(u64),
        Str(String),
    }

    pub fn number<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        Ok(match Option::<NumOrStr>::deserialize(d)? {
            Some(NumOrStr::Num(n)) => n,
            Some(NumOrStr::Str(s)) => s.trim().replace(',', "").parse().unwrap_or(0),
            None => 0,
        })
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ListOrStr {
        List(Vec<String>),
        Str(String),
    }

    pub fn string_list<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
        Ok(match Option::<ListOrStr>::deserialize(d)? {
            Some(ListOrStr::List(v)) => v,
            Some(ListOrStr::Str(s)) => vec![s],
            None => vec![],
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarketplaceEntry {
    #[serde(default)]
    pub repo: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, deserialize_with = "lenient::number")]
    pub stars: u64,
    #[serde(default, rename = "pluginCount", deserialize_with = "lenient::number")]
    pub plugin_count: u64,
    #[serde(default, deserialize_with = "lenient::string_list")]
    pub categories: Vec<String>,
    #[serde(
        default,
        rename = "pluginKeywords",
        deserialize_with = "lenient::string_list"
    )]
    pub plugin_keywords: Vec<String>,
}

impl MarketplaceEntry {
    pub fn star_count(&self) -> u64 {
        self.stars
    }

    /// Keyword overlap against the haystack of repo, description, categories
    /// and keywords. Deliberately dumb — its job is to shortlist for a human
    /// or for the trust floor, not to be clever.
    pub fn relevance(&self, terms: &[String]) -> usize {
        let hay = format!(
            "{} {} {} {}",
            self.repo,
            self.description,
            self.categories.join(" "),
            self.plugin_keywords.join(" ")
        )
        .to_ascii_lowercase();
        terms
            .iter()
            .filter(|t| hay.contains(&t.to_ascii_lowercase()))
            .count()
    }
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub index_url: String,
    pub min_stars: u64,
    pub limit: usize,
    pub timeout_seconds: u64,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            index_url: DEFAULT_INDEX_URL.to_string(),
            min_stars: 100,
            limit: 10,
            timeout_seconds: 20,
        }
    }
}

/// Fetch the index. Returns the raw JSON so callers can cache it.
pub fn fetch_index(opts: &SearchOptions) -> crate::Result<String> {
    if crate::which("curl").is_none() {
        return Err(crate::SkillError::Missing("curl".into()));
    }
    let out = Command::new("curl")
        .args([
            "-sS",
            "--fail",
            "--max-time",
            &opts.timeout_seconds.to_string(),
            &opts.index_url,
        ])
        .output()?;
    if !out.status.success() {
        return Err(crate::SkillError::Command {
            cmd: format!("curl {}", opts.index_url),
            code: out.status.code().unwrap_or(-1),
            stderr: String::from_utf8_lossy(&out.stderr)
                .lines()
                .last()
                .unwrap_or("")
                .to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// Rank index entries against search terms, applying the star floor.
///
/// A parse failure returns an error rather than an empty list. Swallowing it
/// is how a broken field type turns into "no results", which reads like a
/// working search that found nothing.
pub fn rank_checked(
    json: &str,
    terms: &[String],
    opts: &SearchOptions,
) -> crate::Result<Vec<MarketplaceEntry>> {
    let all: Vec<MarketplaceEntry> = serde_json::from_str(json).map_err(|e| {
        crate::SkillError::Refused(format!("marketplace index did not parse: {e}"))
    })?;
    Ok(rank_entries(all, terms, opts))
}

/// Convenience wrapper that treats a parse failure as no results.
pub fn rank(json: &str, terms: &[String], opts: &SearchOptions) -> Vec<MarketplaceEntry> {
    rank_checked(json, terms, opts).unwrap_or_default()
}

fn rank_entries(
    all: Vec<MarketplaceEntry>,
    terms: &[String],
    opts: &SearchOptions,
) -> Vec<MarketplaceEntry> {
    let mut hits: Vec<(usize, MarketplaceEntry)> = all
        .into_iter()
        .filter(|e| e.star_count() >= opts.min_stars)
        .filter(|e| !crate::is_blocklisted(&e.repo))
        .map(|e| (e.relevance(terms), e))
        .filter(|(score, _)| *score > 0)
        .collect();

    hits.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.star_count().cmp(&a.1.star_count())));
    hits.into_iter().take(opts.limit).map(|(_, e)| e).collect()
}

/// Fetch and rank in one call.
pub fn search_marketplace(terms: &[String], opts: &SearchOptions) -> crate::Result<Vec<MarketplaceEntry>> {
    let json = fetch_index(opts)?;
    rank_checked(&json, terms, opts)
}

/// Search the `skills` CLI. Separate from the index because the index lists
/// plugin *bundles* while this lists individual skills.
pub fn search_skills_cli(query: &str, cwd: &Path) -> crate::Result<String> {
    if crate::which("npx").is_none() {
        return Err(crate::SkillError::Missing("npx".into()));
    }
    if !crate::is_safe_name(query) && query.contains(|c: char| !c.is_ascii_alphanumeric() && c != ' ' && c != '-') {
        return Err(crate::SkillError::Refused(format!(
            "`{query}` contains characters not allowed in a search term"
        )));
    }
    let out = Command::new("npx")
        .args(["--yes", "skills", "find", query])
        .current_dir(cwd)
        .output()?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mirrors the live payload: numeric stars and pluginCount, array
    /// categories and keywords, and an entry with `stars` absent entirely —
    /// which is true of roughly a third of the real index.
    const SAMPLE: &str = r#"[
      {"repo":"good/testing-tools","slug":"good-testing","description":"Playwright and jest helpers for end to end testing","stars":4200,"pluginCount":6,"categories":["testing"],"pluginKeywords":["jest","playwright","e2e"]},
      {"repo":"tiny/obscure","slug":"tiny-obscure","description":"testing helpers nobody uses","stars":3,"pluginCount":1,"categories":["testing"],"pluginKeywords":["testing"]},
      {"repo":"evil/credential-grabber","slug":"evil","description":"testing your secrets","stars":9000,"pluginCount":1,"categories":["testing"],"pluginKeywords":["testing"]},
      {"repo":"other/design-kit","slug":"design","description":"figma and design systems","stars":1500,"pluginCount":3,"categories":["design"],"pluginKeywords":["figma","ui"]},
      {"repo":"nostars/testing-lib","slug":"nostars","description":"testing utilities","pluginCount":2,"categories":["testing"],"pluginKeywords":["testing"]}
    ]"#;

    fn terms(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn ranking_matches_on_keywords_and_orders_by_relevance() {
        let r = rank(SAMPLE, &terms(&["playwright", "e2e"]), &SearchOptions::default());
        assert_eq!(r[0].repo, "good/testing-tools");
    }

    #[test]
    fn the_star_floor_excludes_obscure_repos() {
        let r = rank(SAMPLE, &terms(&["testing"]), &SearchOptions::default());
        assert!(
            !r.iter().any(|e| e.repo == "tiny/obscure"),
            "3 stars is below the floor"
        );
    }

    #[test]
    fn a_high_star_credential_grabber_is_still_excluded() {
        // Popularity is not trust. This entry outranks everything on stars and
        // matches the query, and must still never be offered.
        let r = rank(SAMPLE, &terms(&["testing"]), &SearchOptions::default());
        assert!(!r.iter().any(|e| e.repo.contains("credential")));
    }

    #[test]
    fn irrelevant_entries_are_dropped_entirely() {
        let r = rank(SAMPLE, &terms(&["playwright"]), &SearchOptions::default());
        assert!(!r.iter().any(|e| e.repo == "other/design-kit"));
    }

    #[test]
    fn no_matches_yields_an_empty_list_rather_than_everything() {
        let r = rank(SAMPLE, &terms(&["quantum-basket-weaving"]), &SearchOptions::default());
        assert!(r.is_empty());
    }

    #[test]
    fn malformed_index_json_yields_nothing_instead_of_panicking() {
        assert!(rank("{not json", &terms(&["x"]), &SearchOptions::default()).is_empty());
        assert!(rank("", &terms(&["x"]), &SearchOptions::default()).is_empty());
    }

    #[test]
    fn the_live_payload_shape_parses() {
        // Numbers for stars, arrays for categories. Getting this wrong made
        // every search silently return nothing.
        let parsed: Vec<MarketplaceEntry> = serde_json::from_str(SAMPLE).expect("parses");
        assert_eq!(parsed.len(), 5);
        assert_eq!(parsed[0].star_count(), 4200);
        assert_eq!(parsed[0].plugin_keywords.len(), 3);
    }

    #[test]
    fn string_encoded_numbers_and_lists_still_parse() {
        let json = r#"[{"repo":"a/b","stars":"1,200","pluginCount":"4","categories":"testing","pluginKeywords":["x"]}]"#;
        let v: Vec<MarketplaceEntry> = serde_json::from_str(json).expect("parses");
        assert_eq!(v[0].star_count(), 1200);
        assert_eq!(v[0].categories, vec!["testing".to_string()]);
    }

    #[test]
    fn an_entry_with_no_stars_field_is_treated_as_zero_not_a_parse_failure() {
        let v: Vec<MarketplaceEntry> = serde_json::from_str(SAMPLE).unwrap();
        let nostars = v.iter().find(|e| e.repo == "nostars/testing-lib").unwrap();
        assert_eq!(nostars.star_count(), 0);
    }

    #[test]
    fn a_broken_index_reports_an_error_rather_than_an_empty_result() {
        let e = rank_checked("{not json", &terms(&["x"]), &SearchOptions::default());
        assert!(e.is_err(), "a parse failure must not look like zero results");
    }

    #[test]
    fn the_limit_is_respected() {
        let opts = SearchOptions {
            limit: 1,
            ..SearchOptions::default()
        };
        let r = rank(SAMPLE, &terms(&["testing"]), &opts);
        assert_eq!(r.len(), 1);
    }
}
