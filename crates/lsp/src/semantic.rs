//! Lexical, scope-aware symbol table.
//!
//! Phase A ships only the [`SymbolTable`] type so that
//! [`crate::source_db::SourceDatabase::symbol_table`] has a stable signature. The scope/index/
//! resolution logic (see the plan, Phase B) replaces the empty body below — *without* changing the
//! trait surface.

/// Placeholder symbol table. Phase B fills this with a `by_scope` index plus a `resolve` walk;
/// until then it carries no data.
#[derive(Debug, Default, Clone)]
pub struct SymbolTable;
