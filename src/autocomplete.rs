use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutocompleteConfig {
    pub tags: Vec<String>,
    pub projects: Vec<String>,
    pub mentions: Vec<String>,
    pub expansions: HashMap<String, String>,
}

impl AutocompleteConfig {
    pub fn sanitized(self) -> Self {
        Self {
            tags: sanitize_list(self.tags),
            projects: sanitize_list(self.projects),
            mentions: sanitize_list(self.mentions),
            expansions: sanitize_expansions(self.expansions),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AutocompleteType {
    Tag,
    Project,
    Mention,
}

impl AutocompleteType {
    pub fn prefix(self) -> char {
        match self {
            AutocompleteType::Tag => '#',
            AutocompleteType::Project => '$',
            AutocompleteType::Mention => '@',
        }
    }

    pub fn from_prefix(ch: char) -> Option<Self> {
        match ch {
            '#' => Some(AutocompleteType::Tag),
            '$' => Some(AutocompleteType::Project),
            '@' => Some(AutocompleteType::Mention),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            AutocompleteType::Tag => "TAG",
            AutocompleteType::Project => "PROJECT",
            AutocompleteType::Mention => "MENTION",
        }
    }

    pub fn spotlight_badge_label(self) -> &'static str {
        match self {
            AutocompleteType::Tag => "[Tab] Tag",
            AutocompleteType::Project => "[Tab] Project",
            AutocompleteType::Mention => "[Tab] Mention",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveToken {
    pub token_type: AutocompleteType,
    pub prefix: char,
    pub query: String,
    pub start_index: usize,
    pub end_index: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutocompleteMatch {
    pub token_type: AutocompleteType,
    pub prefix: char,
    pub value: String,
    pub query: String,
    pub start_index: usize,
    pub end_index: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutocompleteResult {
    pub token: Option<ActiveToken>,
    pub matches: Vec<AutocompleteMatch>,
    pub ghost_suffix: String,
}

/// Trims whitespace, strips leading `#`, `$`, or `@`, and deduplicates
/// case-insensitively while preserving first-seen canonical casing.
pub fn sanitize_list(items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();

    for item in items {
        let trimmed = item.trim();
        let stripped = trimmed.trim_start_matches(['#', '$', '@']).trim();
        if stripped.is_empty() {
            continue;
        }
        let lower = stripped.to_lowercase();
        if seen.insert(lower) {
            result.push(stripped.to_string());
        }
    }

    result
}

/// Trims keys and values, dropping entries where either is empty.
pub fn sanitize_expansions(map: HashMap<String, String>) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for (k, v) in map {
        let key = k.trim();
        let val = v.trim();
        if !key.is_empty() && !val.is_empty() {
            result.insert(key.to_string(), val.to_string());
        }
    }
    result
}

/// Detects an active `#tag`, `$project`, or `@mention` token at `caret_pos` (UTF-8 byte offset).
pub fn detect_token_at_caret(text: &str, caret_pos: usize) -> Option<ActiveToken> {
    if text.is_empty() || caret_pos > text.len() || !text.is_char_boundary(caret_pos) {
        return None;
    }

    // Scan backward from `caret_pos` to find the start of the current whitespace-delimited word.
    let mut start_index = 0;
    for (idx, ch) in text[..caret_pos].char_indices().rev() {
        if ch.is_whitespace() {
            start_index = idx + ch.len_utf8();
            break;
        }
    }

    // Handle priority prefixes (`!!` or `!`) at index 0 when immediately followed by `#`, `$`, or `@`.
    if start_index == 0 {
        if caret_pos >= 2
            && text.starts_with("!!")
            && text[2..]
                .chars()
                .next()
                .and_then(AutocompleteType::from_prefix)
                .is_some()
        {
            start_index = 2;
        } else if caret_pos >= 1
            && text.starts_with('!')
            && text[1..]
                .chars()
                .next()
                .and_then(AutocompleteType::from_prefix)
                .is_some()
        {
            start_index = 1;
        }
    }

    if caret_pos <= start_index {
        return None;
    }

    let prefix = text[start_index..].chars().next()?;
    let token_type = AutocompleteType::from_prefix(prefix)?;

    // Scan forward from `caret_pos` to find `end_index` (next whitespace or end of string).
    let end_index = text[caret_pos..]
        .char_indices()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(rel_idx, _)| caret_pos + rel_idx)
        .unwrap_or(text.len());

    let query_start = start_index + prefix.len_utf8();
    let query = text[query_start..caret_pos].to_string();

    Some(ActiveToken {
        token_type,
        prefix,
        query,
        start_index,
        end_index,
    })
}

/// Computes ranked autocomplete suggestions and optional inline ghost suffix at `caret_pos`.
pub fn get_suggestions(
    config: &AutocompleteConfig,
    text: &str,
    caret_pos: usize,
) -> AutocompleteResult {
    let Some(token) = detect_token_at_caret(text, caret_pos) else {
        return AutocompleteResult::default();
    };

    let candidates = match token.token_type {
        AutocompleteType::Tag => &config.tags,
        AutocompleteType::Project => &config.projects,
        AutocompleteType::Mention => &config.mentions,
    };

    let query_lower = token.query.to_lowercase();
    let mut tier1 = Vec::new();
    let mut tier2 = Vec::new();

    for item in candidates {
        if query_lower.is_empty() {
            tier1.push(item);
        } else {
            let item_lower = item.to_lowercase();
            if item_lower.starts_with(&query_lower) {
                tier1.push(item);
            } else if item_lower.contains(&query_lower) {
                tier2.push(item);
            }
        }
    }

    let matches: Vec<AutocompleteMatch> = tier1
        .into_iter()
        .chain(tier2)
        .map(|value| AutocompleteMatch {
            token_type: token.token_type,
            prefix: token.prefix,
            value: value.clone(),
            query: token.query.clone(),
            start_index: token.start_index,
            end_index: token.end_index,
        })
        .collect();

    let ghost_suffix = if !matches.is_empty()
        && caret_pos == token.end_index
        && matches[0].value.to_lowercase().starts_with(&query_lower)
    {
        let query_char_count = token.query.chars().count();
        matches[0].value.chars().skip(query_char_count).collect()
    } else {
        String::new()
    };

    AutocompleteResult {
        token: Some(token),
        matches,
        ghost_suffix,
    }
}

/// Replaces the active token range with `{prefix}{value} ` and returns `(new_text, new_caret_pos)`.
pub fn apply_completion(text: &str, matched: &AutocompleteMatch) -> (String, usize) {
    let before = &text[..matched.start_index];
    let after = &text[matched.end_index..];
    let inserted = format!("{}{} ", matched.prefix, matched.value);
    let new_caret_pos = before.len() + inserted.len();
    let new_text = format!("{before}{inserted}{after}");
    (new_text, new_caret_pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> AutocompleteConfig {
        AutocompleteConfig {
            tags: sanitize_list(vec![
                "#urgent".to_string(),
                "  backend ".to_string(),
                "URGENT".to_string(),
                "bug-fix".to_string(),
                "ui-urgent".to_string(),
            ]),
            projects: sanitize_list(vec!["$apollo".to_string(), "zeus".to_string()]),
            mentions: sanitize_list(vec!["@Alice".to_string(), "bob".to_string()]),
            expansions: HashMap::new(),
        }
    }

    #[test]
    fn test_sanitize_list_and_expansions() {
        let list = sanitize_list(vec![
            " #Frontend ".into(),
            "frontend".into(),
            "$backend".into(),
            "@alice".into(),
            "   ".into(),
            "#".into(),
        ]);
        assert_eq!(list, vec!["Frontend", "backend", "alice"]);

        let mut map = HashMap::new();
        map.insert(" rpr ".to_string(), " review PR ".to_string());
        map.insert("empty".to_string(), "   ".to_string());
        map.insert("   ".to_string(), "value".to_string());
        let clean = sanitize_expansions(map);
        assert_eq!(clean.len(), 1);
        assert_eq!(clean.get("rpr").map(String::as_str), Some("review PR"));
    }

    #[test]
    fn test_detect_token_basic_and_prefixes() {
        // Basic #tag at end
        let text = "Fix issue #urg";
        let tok = detect_token_at_caret(text, text.len()).unwrap();
        assert_eq!(tok.token_type, AutocompleteType::Tag);
        assert_eq!(tok.prefix, '#');
        assert_eq!(tok.query, "urg");
        assert_eq!(tok.start_index, 10);
        assert_eq!(tok.end_index, 14);

        // Priority prefix `!#tag` at index 0
        let text = "!#urg";
        let tok = detect_token_at_caret(text, text.len()).unwrap();
        assert_eq!(tok.token_type, AutocompleteType::Tag);
        assert_eq!(tok.query, "urg");
        assert_eq!(tok.start_index, 1);
        assert_eq!(tok.end_index, 5);

        // Priority prefix `!!$proj` at index 0
        let text = "!!$apo";
        let tok = detect_token_at_caret(text, text.len()).unwrap();
        assert_eq!(tok.token_type, AutocompleteType::Project);
        assert_eq!(tok.query, "apo");
        assert_eq!(tok.start_index, 2);
        assert_eq!(tok.end_index, 6);

        // Mention `@` with empty query
        let text = "Ping @";
        let tok = detect_token_at_caret(text, text.len()).unwrap();
        assert_eq!(tok.token_type, AutocompleteType::Mention);
        assert_eq!(tok.query, "");
        assert_eq!(tok.start_index, 5);
        assert_eq!(tok.end_index, 6);
    }

    #[test]
    fn test_detect_token_mid_word_and_utf8() {
        // Caret in the middle of a token: `Fix #urgent today` with caret after `#urg`
        let text = "Fix #urgent today";
        let caret = 8; // "Fix #urg"
        let tok = detect_token_at_caret(text, caret).unwrap();
        assert_eq!(tok.query, "urg");
        assert_eq!(tok.start_index, 4);
        assert_eq!(tok.end_index, 11);

        // Multi-byte UTF-8 before token
        let text = "Café ☕ $ze";
        let tok = detect_token_at_caret(text, text.len()).unwrap();
        assert_eq!(tok.token_type, AutocompleteType::Project);
        assert_eq!(tok.query, "ze");
        assert_eq!(&text[tok.start_index..tok.end_index], "$ze");

        // Non-char boundary caret returns None safely without panicking
        assert!(detect_token_at_caret("Café #tag", 4).is_none());
    }

    #[test]
    fn test_tier_ranking_and_ghost_suffix() {
        let cfg = sample_config();
        // Query "urg" should rank "urgent" (Tier 1 starts_with) before "ui-urgent" (Tier 2 contains)
        let res = get_suggestions(&cfg, "Task #urg", 9);
        assert_eq!(res.matches.len(), 2);
        assert_eq!(res.matches[0].value, "urgent");
        assert_eq!(res.matches[1].value, "ui-urgent");
        assert_eq!(res.ghost_suffix, "ent");

        // Mid-token caret should suppress ghost_suffix
        let res_mid = get_suggestions(&cfg, "Task #urgent", 9); // caret after "#urg"
        assert_eq!(res_mid.matches[0].value, "urgent");
        assert_eq!(res_mid.ghost_suffix, "");

        // Tier-2-only match (contains, not starts_with) should have empty ghost_suffix
        let res_contains = get_suggestions(&cfg, "Task #fix", 9);
        assert_eq!(res_contains.matches.len(), 1);
        assert_eq!(res_contains.matches[0].value, "bug-fix");
        assert_eq!(res_contains.ghost_suffix, "");
    }

    #[test]
    fn test_apply_completion() {
        let cfg = sample_config();
        let text = "Deploy @al to prod";
        let caret = 10; // right after "@al"
        let res = get_suggestions(&cfg, text, caret);
        let (new_text, new_caret) = apply_completion(text, &res.matches[0]);
        assert_eq!(new_text, "Deploy @Alice  to prod");
        assert_eq!(new_caret, "Deploy @Alice ".len());
    }
}
