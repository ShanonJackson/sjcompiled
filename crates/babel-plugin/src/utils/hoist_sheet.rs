//! 1:1 port of `packages/babel-plugin/src/utils/hoist-sheet.ts`.
//!
//! Hoists a sheet to the top of the module if it's not already
//! hoisted. Returns the symbol name of the referencing identifier;
//! callers reconstruct the SWC `Ident` from the name at emit time
//! (see `state.rs` `sheets` field comment).
//!
//! Babel→SWC behavioural divergences (none affect output bytes):
//!
//! * Babel call site: `meta.parentPath.scope.generateUidIdentifier('')`
//!   plus `path.insertBefore(...)` plus `scope.registerBinding(...)`.
//!   The SWC visitor doesn't have NodePath / scope tracking yet —
//!   the production-equivalent landing point is Phase 5 §5.4
//!   (resolve_binding). For §4.6 the registration boils down to:
//!   1. Mint a fresh `_<n>` UID via `state.next_uid_name()`.
//!   2. Record the (sheet_text, hoisted_name) pair in `state.sheets`
//!      via `MutationRecorder::SheetsInsert` (the §5.3 cache schema's
//!      site 8).
//!   3. The actual `const _<n> = "<sheet>";` declaration insert into
//!      `Program.body` is a Phase 6 emit-pass concern — the visitor's
//!      `Program::exit` reads `state.sheets()` and synthesises the
//!      VarDecls deterministically. NOT a `paths_to_cleanup` entry;
//!      the data is already on `state.sheets` and the AST emit is
//!      a one-shot read at exit.
//!
//! * Babel returns `t.Identifier`; the Rust port returns `String`
//!   (the symbol name). Callers that need a usable AST `Ident`
//!   wrap with `Ident::new(name.into(), DUMMY_SP, Default::default())`.
//!   This matches `state.rs`'s "we store the symbol name and
//!   reconstruct the SWC `Ident` on emit" contract.
//!
//! * Babel's `findParent(path => path.isProgram())` then "first
//!   non-import body item" lookup is dropped — we don't insert AST
//!   nodes here (see point 1 above). The Phase 6 Program::exit
//!   walker iterates `state.sheets()` in IndexMap insertion order
//!   and prepends the VarDecls to the post-import body region.

use crate::mutation_recorder::{MutationRecorder, StateDiff};
use crate::types::Metadata;

/// Hoist a stylesheet under a fresh UID. Returns the symbol name
/// of the hoisted identifier (cached idempotently — calling twice
/// with the same `sheet` returns the same name).
///
/// Signature divergence from upstream: takes an explicit
/// `&mut MutationRecorder`. The upstream JS plugin reads/writes
/// `state.sheets` directly; the Rust port routes captured-field
/// writes through the recorder per PLAN.md §3.9.8. Tests pass
/// `&mut MutationRecorder::new()`; the visitor threads
/// `&mut self.recorder` through the handler chain.
pub fn hoist_sheet(
    sheet: &str,
    meta: &mut Metadata<'_>,
    recorder: &mut MutationRecorder,
) -> String {
    // 1. Cache hit — return the existing hoist name unchanged.
    if let Some(existing) = meta.state.sheets().get(sheet) {
        return existing.clone();
    }

    // 2. Cache miss — mint a UID, record the (sheet_text, name) pair.
    let hoisted_name = meta.state.next_uid_name();

    recorder.apply(
        StateDiff::SheetsInsert {
            sheet_text: sheet.to_string(),
            hoisted_name: hoisted_name.clone(),
        },
        meta.state,
    );

    hoisted_name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::State;
    use crate::types::MetadataContext;

    fn fresh_meta(state: &mut State) -> Metadata<'_> {
        Metadata {
            state,
            parent_id: 0,
            own_id: None,
            context: MetadataContext::Root,
        }
    }

    #[test]
    fn first_hoist_mints_underscore_zero() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let name = hoist_sheet("._abc{color:red}", &mut meta, &mut recorder);
        assert_eq!(name, "_0");
    }

    #[test]
    fn distinct_sheets_get_distinct_uids() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let a = hoist_sheet("._a{color:red}", &mut meta, &mut recorder);
        let b = hoist_sheet("._b{color:blue}", &mut meta, &mut recorder);
        assert_eq!(a, "_0");
        assert_eq!(b, "_1");
        assert_ne!(a, b);
    }

    #[test]
    fn duplicate_hoist_is_idempotent_and_no_recorder_write() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let a = hoist_sheet("._a{color:red}", &mut meta, &mut recorder);
        let a_again = hoist_sheet("._a{color:red}", &mut meta, &mut recorder);
        assert_eq!(a, a_again);
        // Diff log captures only the first write.
        assert_eq!(recorder.diff_log().len(), 1);
    }

    #[test]
    fn recorder_captures_sheets_insert_diff() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let _ = hoist_sheet("._a{color:red}", &mut meta, &mut recorder);
        let log = recorder.diff_log();
        assert_eq!(log.len(), 1);
        match &log[0] {
            StateDiff::SheetsInsert {
                sheet_text,
                hoisted_name,
            } => {
                assert_eq!(sheet_text, "._a{color:red}");
                assert_eq!(hoisted_name, "_0");
            }
            other => panic!("expected SheetsInsert, got {:?}", other),
        }
    }

    #[test]
    fn sheets_indexmap_preserves_insertion_order() {
        // Phase 6 emit-pass reads state.sheets() in IndexMap order;
        // duplicates must NOT shift earlier entries to the back.
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        hoist_sheet("a", &mut meta, &mut recorder);
        hoist_sheet("b", &mut meta, &mut recorder);
        hoist_sheet("a", &mut meta, &mut recorder); // dup of first
        hoist_sheet("c", &mut meta, &mut recorder);

        let keys: Vec<&String> = meta.state.sheets().keys().collect();
        assert_eq!(keys, vec![&"a".to_string(), &"b".to_string(), &"c".to_string()]);
    }
}
