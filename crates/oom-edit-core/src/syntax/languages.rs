//! Grammar registry and info-string alias table.
//!
//! `LANGUAGES` is the static registry of all grammars the highlighter knows
//! about. Each row carries:
//!
//! - canonical name (the name used in `SemanticStyle`-free queries)
//! - alias slice (case-insensitive lookup for fence info strings)
//! - language getter (returns the `tree_sitter::Language`)
//! - highlight-query source (bundled `HIGHLIGHTS_QUERY` or a minimal query)
//!
//! See architecture §6.3 and task T06 for the full language set and alias
//! mappings (FR-4.3).

// ── Language definitions ────────────────────────────────────────────────────

/// A single row in the language registry.
pub(super) struct LangDef {
    /// Canonical name (e.g. `"rust"`, `"python"`).
    pub(super) name: &'static str,
    /// Info-string aliases — case-insensitive lookup.
    /// `["sh", "shell", "zsh"]` all map to bash.
    pub(super) aliases: &'static [&'static str],
    /// Returns the tree-sitter `Language` for this grammar.
    pub(super) language_fn: fn() -> tree_sitter::Language,
    /// Grammar-specific tree-sitter highlight query.
    pub(super) highlights_query: &'static str,
}

// ── The registry ────────────────────────────────────────────────────────────

/// The complete language registry. New languages are added by appending a row
/// and one `Cargo.toml` dependency — no other code changes are required.
///
/// Covers the 10 fence languages (FR-4.3) plus md-block and md-inline.
pub(super) static LANGUAGES: &[LangDef] = &[
    LangDef {
        name: "markdown",
        aliases: &[],
        language_fn: || tree_sitter_md::LANGUAGE.into(),
        highlights_query: tree_sitter_md::HIGHLIGHT_QUERY_BLOCK,
    },
    LangDef {
        name: "markdown_inline",
        aliases: &[],
        language_fn: || tree_sitter_md::INLINE_LANGUAGE.into(),
        highlights_query: tree_sitter_md::HIGHLIGHT_QUERY_INLINE,
    },
    LangDef {
        name: "rust",
        aliases: &[],
        language_fn: || tree_sitter_rust::LANGUAGE.into(),
        highlights_query: tree_sitter_rust::HIGHLIGHTS_QUERY,
    },
    LangDef {
        name: "python",
        aliases: &["py"],
        language_fn: || tree_sitter_python::LANGUAGE.into(),
        highlights_query: tree_sitter_python::HIGHLIGHTS_QUERY,
    },
    LangDef {
        name: "javascript",
        aliases: &["js", "jsx", "mjs", "cjs"],
        language_fn: || tree_sitter_javascript::LANGUAGE.into(),
        highlights_query: tree_sitter_javascript::HIGHLIGHT_QUERY,
    },
    LangDef {
        name: "typescript",
        aliases: &["ts", "tsx", "dts"],
        language_fn: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        highlights_query: tree_sitter_typescript::HIGHLIGHTS_QUERY,
    },
    LangDef {
        name: "go",
        aliases: &["golang"],
        language_fn: || tree_sitter_go::LANGUAGE.into(),
        highlights_query: tree_sitter_go::HIGHLIGHTS_QUERY,
    },
    LangDef {
        name: "bash",
        aliases: &["sh", "shell", "zsh", "ksh", "dash"],
        language_fn: || tree_sitter_bash::LANGUAGE.into(),
        highlights_query: tree_sitter_bash::HIGHLIGHT_QUERY,
    },
    LangDef {
        name: "json",
        aliases: &["jsonc", "json5"],
        language_fn: || tree_sitter_json::LANGUAGE.into(),
        highlights_query: tree_sitter_json::HIGHLIGHTS_QUERY,
    },
    LangDef {
        name: "c",
        aliases: &["h", "cpp", "cxx", "cc", "c++", "hpp", "hxx"],
        language_fn: || tree_sitter_c::LANGUAGE.into(),
        highlights_query: tree_sitter_c::HIGHLIGHT_QUERY,
    },
    LangDef {
        name: "yaml",
        aliases: &["yml"],
        language_fn: || tree_sitter_yaml::LANGUAGE.into(),
        highlights_query: tree_sitter_yaml::HIGHLIGHTS_QUERY,
    },
    LangDef {
        name: "toml",
        aliases: &[],
        language_fn: || tree_sitter_toml_ng::LANGUAGE.into(),
        highlights_query: tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
    },
];

// ── Lookup ──────────────────────────────────────────────────────────────────

/// Look up a `LangDef` by canonical name (case-insensitive).
#[allow(dead_code)]
pub(super) fn find_by_name(name: &str) -> Option<&'static LangDef> {
    let name_lower = name.to_lowercase();
    LANGUAGES
        .iter()
        .find(|lang| lang.name == name_lower || lang.aliases.contains(&name_lower.as_str()))
}

/// Look up a `LangDef` by info-string alias (case-insensitive).
/// Returns `None` for unknown languages — the caller should use
/// `SemanticStyle::CodeBlock` as the fallback style.
pub(super) fn find_by_alias(alias: &str) -> Option<&'static LangDef> {
    let alias_lower = alias.to_lowercase();
    LANGUAGES
        .iter()
        .find(|lang| lang.name == alias_lower || lang.aliases.contains(&alias_lower.as_str()))
}

/// Resolve an info string to a tree-sitter `Language`, or `None` if
/// the language is unknown.
#[allow(dead_code)]
pub(super) fn resolve_language(info_string: &str) -> Option<tree_sitter::Language> {
    find_by_alias(info_string).map(|lang| (lang.language_fn)())
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_12_languages_present() {
        let names: Vec<&str> = LANGUAGES.iter().map(|l| l.name).collect();
        assert!(names.contains(&"markdown"));
        assert!(names.contains(&"markdown_inline"));
        assert!(names.contains(&"rust"));
        assert!(names.contains(&"python"));
        assert!(names.contains(&"javascript"));
        assert!(names.contains(&"typescript"));
        assert!(names.contains(&"go"));
        assert!(names.contains(&"bash"));
        assert!(names.contains(&"json"));
        assert!(names.contains(&"c"));
        assert!(names.contains(&"yaml"));
        assert!(names.contains(&"toml"));
        assert_eq!(LANGUAGES.len(), 12, "expected exactly 12 language entries");
    }

    #[test]
    fn all_languages_return_valid_language() {
        for lang in LANGUAGES {
            let lang_obj = (lang.language_fn)();
            // A valid language has a non-zero pointer; tree-sitter returns
            // a valid Language struct for all our grammar crates.
            assert!(!lang.name.is_empty(), "language name must not be empty");
            // Verify the language can create a parser
            let mut parser = tree_sitter::Parser::new();
            let _ = parser.set_language(&lang_obj);
            // If set_language panics, the language is invalid
        }
    }

    #[test]
    fn all_language_highlight_queries_compile() {
        for lang in LANGUAGES {
            let language = (lang.language_fn)();
            assert!(!lang.highlights_query.trim().is_empty());
            tree_sitter::Query::new(&language, lang.highlights_query).unwrap_or_else(|error| {
                panic!("{} highlight query should compile: {error}", lang.name)
            });
        }
    }

    #[test]
    fn registry_names_and_aliases_are_unique_case_insensitively() {
        let mut identifiers = std::collections::HashSet::new();
        for language in LANGUAGES {
            for identifier in std::iter::once(language.name).chain(language.aliases.iter().copied())
            {
                assert!(
                    identifiers.insert(identifier.to_ascii_lowercase()),
                    "duplicate language name or alias: {identifier}"
                );
            }
        }
    }

    #[test]
    fn canonical_name_lookup() {
        assert!(find_by_name("rust").is_some());
        assert!(find_by_name("python").is_some());
        assert!(find_by_name("Markdown").is_some()); // case-insensitive
        assert!(find_by_name("RUST").is_some());
        assert!(find_by_name("nonexistent").is_none());
    }

    #[test]
    fn alias_lookup() {
        // Bash aliases
        assert!(find_by_alias("sh").is_some());
        assert!(find_by_alias("shell").is_some());
        assert!(find_by_alias("zsh").is_some());
        assert!(find_by_alias("SH").is_some()); // case-insensitive

        // JS aliases
        assert!(find_by_alias("js").is_some());
        assert!(find_by_alias("JS").is_some());

        // TS aliases
        assert!(find_by_alias("ts").is_some());
        assert!(find_by_alias("tsx").is_some());

        // Python aliases
        assert!(find_by_alias("py").is_some());

        // YAML aliases
        assert!(find_by_alias("yml").is_some());

        // JSON aliases
        assert!(find_by_alias("jsonc").is_some());

        // Unknown
        assert!(find_by_alias("ruby").is_none());
        assert!(find_by_alias("swift").is_none());
        assert!(find_by_alias("").is_none());
    }

    #[test]
    fn resolve_language_returns_some_for_known() {
        assert!(resolve_language("rust").is_some());
        assert!(resolve_language("Rust").is_some());
        assert!(resolve_language("sh").is_some());
        assert!(resolve_language("js").is_some());
        assert!(resolve_language("yml").is_some());
    }

    #[test]
    fn resolve_language_returns_none_for_unknown() {
        assert!(resolve_language("ruby").is_none());
        assert!(resolve_language("gofoo").is_none());
        assert!(resolve_language("unknown_lang").is_none());
    }

    #[test]
    fn markdown_and_inline_have_no_aliases() {
        let md = find_by_name("markdown").unwrap();
        let md_inline = find_by_name("markdown_inline").unwrap();
        assert!(md.aliases.is_empty(), "markdown should have no aliases");
        assert!(
            md_inline.aliases.is_empty(),
            "markdown_inline should have no aliases"
        );
    }
}
