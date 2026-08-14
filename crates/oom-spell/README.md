# oom-spell

`oom-spell` is oom-edit's reusable, dependency-free English spell engine. It
accepts owned UTF-8 plain word lists, builds incrementally under explicit byte
budgets, performs exact/case-folded/possessive lookup, and returns deterministic
optimal-string-alignment suggestions within edit distance two.

Its tokenizer advances under explicit byte budgets, preserves absolute UTF-8
source ranges, and accepts caller-owned exclusion spans. Candidate policy is
text-generic: it skips acronyms, identifiers, entity-shaped and non-ASCII
tokens, and splits eligible ASCII-hyphenated words. Markdown construct policy
remains the responsibility of the embedding application.

The crate performs no filesystem access, clock reads, Markdown parsing, editor
integration, terminal work, or networking. Dictionary entries are normalized
to lowercase ASCII letters with optional internal straight apostrophes. English
v1 intentionally makes no Unicode normalization or non-ASCII case-fold claim.

All supported API is re-exported from the crate root. Build and verification are
run through the workspace `Makefile`.
