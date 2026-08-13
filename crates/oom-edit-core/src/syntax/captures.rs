//! Capture-name → `SemanticStyle` mapping.
//!
//! Tree-sitter captures (e.g. `@keyword`, `@function.builtin`, `@string`) are
//! mapped into the stable `SemanticStyle` set via **prefix-based rules**.
//! This keeps the public style surface fixed regardless of which grammar is
//! active — new grammars never grow the style enum.
//!
//! See architecture §5 and task T06 for the canonical prefix table.

use crate::style::SemanticStyle;

// ── Capture mapping ─────────────────────────────────────────────────────────

/// Map a tree-sitter capture name (without the `@` prefix) to a
/// `SemanticStyle`. Uses prefix-based rules: the first rule whose prefix
/// matches wins; unmatched captures fall back to `Text`.
///
/// Capture names may contain dots (e.g. `@function.builtin`,
/// `@constant.numeric`); the function strips the suffix before matching.
pub fn capture_to_style(capture: &str) -> SemanticStyle {
    // Normalise: strip leading `@` if present, lowercase for matching
    let name = capture.strip_prefix('@').unwrap_or(capture);
    let name_lower = name.to_lowercase();

    // Prefix-based rules (order matters — first match wins)
    // keyword* → Keyword
    if name_lower.starts_with("keyword") {
        return SemanticStyle::Keyword;
    }
    // function* → Function
    if name_lower.starts_with("function") {
        return SemanticStyle::Function;
    }
    // type* → TypeName
    if name_lower.starts_with("type") {
        return SemanticStyle::TypeName;
    }
    // string* → StringLit
    if name_lower.starts_with("string") {
        return SemanticStyle::StringLit;
    }
    // number* or constant.numeric* → NumberLit
    if name_lower.starts_with("number")
        || name_lower.starts_with("constant.numeric")
        || name_lower.starts_with("constant")
    {
        return SemanticStyle::NumberLit;
    }
    // comment* → Comment
    if name_lower.starts_with("comment") {
        return SemanticStyle::Comment;
    }
    // operator* → Operator
    if name_lower.starts_with("operator") {
        return SemanticStyle::Operator;
    }
    // variable* → Variable
    if name_lower.starts_with("variable") {
        return SemanticStyle::Variable;
    }
    // punctuation* → Punct
    if name_lower.starts_with("punctuation") {
        return SemanticStyle::Punct;
    }

    // Unmapped → Text
    SemanticStyle::Text
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_prefix_maps_to_keyword() {
        assert_eq!(capture_to_style("keyword"), SemanticStyle::Keyword);
        assert_eq!(capture_to_style("@keyword"), SemanticStyle::Keyword);
        assert_eq!(capture_to_style("keyword.control"), SemanticStyle::Keyword);
        assert_eq!(capture_to_style("Keyword"), SemanticStyle::Keyword);
    }

    #[test]
    fn function_prefix_maps_to_function() {
        assert_eq!(capture_to_style("function"), SemanticStyle::Function);
        assert_eq!(capture_to_style("@function"), SemanticStyle::Function);
        assert_eq!(
            capture_to_style("function.builtin"),
            SemanticStyle::Function
        );
        assert_eq!(capture_to_style("Function"), SemanticStyle::Function);
    }

    #[test]
    fn type_prefix_maps_to_typename() {
        assert_eq!(capture_to_style("type"), SemanticStyle::TypeName);
        assert_eq!(capture_to_style("@type"), SemanticStyle::TypeName);
        assert_eq!(capture_to_style("type.definition"), SemanticStyle::TypeName);
    }

    #[test]
    fn string_prefix_maps_to_stringlit() {
        assert_eq!(capture_to_style("string"), SemanticStyle::StringLit);
        assert_eq!(capture_to_style("@string"), SemanticStyle::StringLit);
        assert_eq!(capture_to_style("string.regexp"), SemanticStyle::StringLit);
    }

    #[test]
    fn number_prefix_maps_to_numberlit() {
        assert_eq!(capture_to_style("number"), SemanticStyle::NumberLit);
        assert_eq!(capture_to_style("@number"), SemanticStyle::NumberLit);
        assert_eq!(
            capture_to_style("constant.numeric"),
            SemanticStyle::NumberLit
        );
        assert_eq!(capture_to_style("constant"), SemanticStyle::NumberLit);
    }

    #[test]
    fn comment_prefix_maps_to_comment() {
        assert_eq!(capture_to_style("comment"), SemanticStyle::Comment);
        assert_eq!(capture_to_style("@comment"), SemanticStyle::Comment);
    }

    #[test]
    fn operator_prefix_maps_to_operator() {
        assert_eq!(capture_to_style("operator"), SemanticStyle::Operator);
        assert_eq!(capture_to_style("@operator"), SemanticStyle::Operator);
    }

    #[test]
    fn variable_prefix_maps_to_variable() {
        assert_eq!(capture_to_style("variable"), SemanticStyle::Variable);
        assert_eq!(capture_to_style("@variable"), SemanticStyle::Variable);
        assert_eq!(
            capture_to_style("variable.parameter"),
            SemanticStyle::Variable
        );
    }

    #[test]
    fn punctuation_prefix_maps_to_punct() {
        assert_eq!(capture_to_style("punctuation"), SemanticStyle::Punct);
        assert_eq!(capture_to_style("@punctuation"), SemanticStyle::Punct);
        assert_eq!(
            capture_to_style("punctuation.delimiter"),
            SemanticStyle::Punct
        );
    }

    #[test]
    fn unmapped_captures_fallback_to_text() {
        // These captures should not match any prefix rule
        assert_eq!(capture_to_style("heading"), SemanticStyle::Text);
        assert_eq!(capture_to_style("title"), SemanticStyle::Text);
        assert_eq!(capture_to_style("delimiter"), SemanticStyle::Text);
        assert_eq!(capture_to_style("emphasis"), SemanticStyle::Text);
        assert_eq!(capture_to_style("strong"), SemanticStyle::Text);
        assert_eq!(capture_to_style("uri"), SemanticStyle::Text);
        assert_eq!(capture_to_style("none"), SemanticStyle::Text);
        assert_eq!(capture_to_style(""), SemanticStyle::Text);
    }

    #[test]
    fn case_insensitive_matching() {
        assert_eq!(capture_to_style("KEYWORD"), SemanticStyle::Keyword);
        assert_eq!(capture_to_style("Keyword"), SemanticStyle::Keyword);
        assert_eq!(
            capture_to_style("FUNCTION.builtin"),
            SemanticStyle::Function
        );
        assert_eq!(capture_to_style("STRING.regexp"), SemanticStyle::StringLit);
    }

    #[test]
    fn prefix_order_keyword_before_keyword_control() {
        // keyword.control should match keyword prefix (first rule)
        assert_eq!(capture_to_style("keyword.control"), SemanticStyle::Keyword);
    }
}
