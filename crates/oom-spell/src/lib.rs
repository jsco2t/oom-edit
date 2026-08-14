//! Dependency-free, resumable English spell-checking primitives.
//!
//! `oom-spell` owns dictionary normalization, incremental engine construction,
//! word lookup, bounded spelling suggestions, resumable UTF-8 tokenization,
//! and text-generic candidate policy. It deliberately has no filesystem,
//! Markdown, editor, terminal, or clock knowledge.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod engine;
mod policy;
mod tokenize;

pub use engine::{
    normalize_dictionary_entry, AddWordOutcome, BuildIncomplete, BuildProgress,
    DictionaryEntryError, SpellEngine, SpellEngineBuilder, MAX_CHECKED_WORD_BYTES,
};
pub use policy::{classify_candidate, CandidateDecision};
pub use tokenize::{tokenize_chunk, Token, TokenChunk, TokenShape, TokenizerState};
