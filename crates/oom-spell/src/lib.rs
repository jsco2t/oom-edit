//! Dependency-free, resumable English spell-checking primitives.
//!
//! `oom-spell` owns dictionary normalization, incremental engine construction,
//! word lookup, and bounded spelling suggestions. It deliberately has no
//! filesystem, Markdown, editor, terminal, or clock knowledge.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod engine;

pub use engine::{
    normalize_dictionary_entry, AddWordOutcome, BuildIncomplete, BuildProgress,
    DictionaryEntryError, SpellEngine, SpellEngineBuilder, MAX_CHECKED_WORD_BYTES,
};
