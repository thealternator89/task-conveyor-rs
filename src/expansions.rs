use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExpansionResult {
    pub expanded: bool,
    pub new_text: String,
    pub new_caret_pos: usize,
}

/// When `prev_value` matches `^!{0,2}\s*$` and `new_value` matches `^(!{0,2}\s*)([a-z])$`,
/// uppercases that single lowercase character.
pub fn auto_capitalize_first_letter(new_value: &str, prev_value: &str) -> String {
    if !is_prefix_only(prev_value) {
        return new_value.to_string();
    }

    let bang_count = new_value.chars().take_while(|&c| c == '!').count();
    if bang_count > 2 {
        return new_value.to_string();
    }

    let after_bangs = &new_value[bang_count..];
    let ws_bytes: usize = after_bangs
        .chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| c.len_utf8())
        .sum();
    let prefix_len = bang_count + ws_bytes;
    let remainder = &new_value[prefix_len..];

    let mut chars = remainder.chars();
    if let (Some(ch), None) = (chars.next(), chars.next()) {
        if ch.is_lowercase() {
            return format!("{}{}", &new_value[..prefix_len], ch.to_uppercase());
        }
    }

    new_value.to_string()
}

fn is_prefix_only(s: &str) -> bool {
    let bang_count = s.chars().take_while(|&c| c == '!').count();
    if bang_count > 2 {
        return false;
    }
    s[bang_count..].chars().all(|c| c.is_whitespace())
}

fn uppercase_first_char(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

/// Expands a shorthand token immediately preceding `caret_pos` when the user presses Space.
pub fn expand_on_space(
    text: &str,
    caret_pos: usize,
    expansions: &HashMap<String, String>,
) -> ExpansionResult {
    let unchanged = || ExpansionResult {
        expanded: false,
        new_text: text.to_string(),
        new_caret_pos: caret_pos,
    };

    if expansions.is_empty()
        || caret_pos == 0
        || caret_pos > text.len()
        || !text.is_char_boundary(caret_pos)
    {
        return unchanged();
    }

    let before_caret = &text[..caret_pos];
    if before_caret.chars().next_back().is_none_or(|c| c.is_whitespace()) {
        return unchanged();
    }

    // Find `word_start_index` by scanning backward to whitespace.
    let mut word_start_index = 0;
    for (idx, ch) in before_caret.char_indices().rev() {
        if ch.is_whitespace() {
            word_start_index = idx + ch.len_utf8();
            break;
        }
    }

    let (is_first_word, word_key_start_index) = if word_start_index == 0 {
        let key_start = if caret_pos >= 2 && before_caret.starts_with("!!") {
            2
        } else if caret_pos >= 1 && before_caret.starts_with('!') {
            1
        } else {
            0
        };
        (true, key_start)
    } else {
        (is_prefix_only(&text[..word_start_index]), word_start_index)
    };

    if word_key_start_index >= caret_pos {
        return unchanged();
    }

    let raw_word = &text[word_key_start_index..caret_pos];
    if raw_word.is_empty() || raw_word.starts_with(['/', '#', '$', '@']) {
        return unchanged();
    }

    let expanded_text = if is_first_word {
        let matched_val = expansions.get(raw_word).or_else(|| {
            let lower = raw_word.to_lowercase();
            expansions
                .iter()
                .find(|(k, _)| k.to_lowercase() == lower)
                .map(|(_, v)| v)
        });
        matched_val.map(|v| uppercase_first_char(v))
    } else {
        expansions.get(raw_word).cloned()
    };

    let Some(expanded_text) = expanded_text else {
        return unchanged();
    };

    let new_text = format!(
        "{}{} {}",
        &text[..word_key_start_index],
        expanded_text,
        &text[caret_pos..]
    );
    let new_caret_pos = word_key_start_index + expanded_text.len() + 1;

    ExpansionResult {
        expanded: true,
        new_text,
        new_caret_pos,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_expansions() -> HashMap<String, String> {
        let mut map = HashMap::new();
        map.insert("rpr".to_string(), "review PR".to_string());
        map.insert("wip".to_string(), "work in progress".to_string());
        map
    }

    #[test]
    fn test_auto_capitalize_first_letter() {
        assert_eq!(auto_capitalize_first_letter("f", ""), "F");
        assert_eq!(auto_capitalize_first_letter("!f", "!"), "!F");
        assert_eq!(auto_capitalize_first_letter("!!f", "!!"), "!!F");
        assert_eq!(auto_capitalize_first_letter("! f", "! "), "! F");
        // Does not alter subsequent characters or non-empty previous words
        assert_eq!(auto_capitalize_first_letter("fi", "f"), "fi");
        assert_eq!(auto_capitalize_first_letter("!!!f", "!!!"), "!!!f");
        assert_eq!(auto_capitalize_first_letter("/", ""), "/");
    }

    #[test]
    fn test_expand_on_space_first_word_vs_subsequent() {
        let exp = sample_expansions();

        // First word exact match -> capitalized
        let res = expand_on_space("rpr", 3, &exp);
        assert!(res.expanded);
        assert_eq!(res.new_text, "Review PR ");
        assert_eq!(res.new_caret_pos, "Review PR ".len());

        // First word case-insensitive match (after auto-capitalization of 'R') -> capitalized
        let res = expand_on_space("Rpr", 3, &exp);
        assert!(res.expanded);
        assert_eq!(res.new_text, "Review PR ");

        // First word with `!` or `!!` prefix
        let res = expand_on_space("!!Rpr", 5, &exp);
        assert!(res.expanded);
        assert_eq!(res.new_text, "!!Review PR ");
        assert_eq!(res.new_caret_pos, "!!Review PR ".len());

        // First word after `! ` (prefix + whitespace)
        let res = expand_on_space("! rpr", 5, &exp);
        assert!(res.expanded);
        assert_eq!(res.new_text, "! Review PR ");

        // Second word exact match -> preserves original lowercase from expansion map
        let res = expand_on_space("Please rpr", 10, &exp);
        assert!(res.expanded);
        assert_eq!(res.new_text, "Please review PR ");
        assert_eq!(res.new_caret_pos, "Please review PR ".len());

        // Second word case-mismatch -> does NOT expand
        let res = expand_on_space("Please Rpr", 10, &exp);
        assert!(!res.expanded);
        assert_eq!(res.new_text, "Please Rpr");
    }

    #[test]
    fn test_expand_on_space_ignores_prefixed_tokens() {
        let mut exp = sample_expansions();
        exp.insert("#rpr".to_string(), "should not expand".to_string());

        assert!(!expand_on_space("#rpr", 4, &exp).expanded);
        assert!(!expand_on_space("$rpr", 4, &exp).expanded);
        assert!(!expand_on_space("@rpr", 4, &exp).expanded);
        assert!(!expand_on_space("/rpr", 4, &exp).expanded);
    }
}
