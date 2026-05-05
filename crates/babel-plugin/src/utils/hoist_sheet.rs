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

use swc_core::common::{SyntaxContext, DUMMY_SP};
use swc_core::ecma::ast::{
    BindingIdent, Decl, Expr, Ident, Lit, Module, ModuleDecl, ModuleItem, Pat, Stmt, Str,
    VarDecl, VarDeclKind, VarDeclarator,
};

use crate::mutation_recorder::{MutationRecorder, StateDiff};
use crate::state::State;
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

/// Phase 6 §6.8a-ii — emit-pass for the hoisted sheets recorded
/// during the children walk. Reads `state.sheets()` in
/// IndexMap insertion order and inserts a `const <name> = "<sheet>";`
/// `ModuleItem::Stmt(VarDecl)` for each, immediately BEFORE the first
/// non-`ImportDeclaration` body item — the same insertion point
/// upstream's `path.insertBefore(...)` lands on (per
/// `hoist-sheet.ts`'s `parentBody.filter(p => !p.isImportDeclaration())[0]`).
///
/// Insertion happens via repeated `body.insert(idx, ...)` with
/// monotonically-increasing `idx`, which preserves IndexMap order
/// (first-recorded sheet ends up first in the body).
///
/// Edge cases:
/// - `state.sheets()` empty → no-op.
/// - All-import module (no non-import body item) → insert at end. The
///   sheets land after every import. Babel's behaviour is identical
///   in this shape: `parentBody.filter(...)[0]` is `undefined`, so
///   the `if (path)` guard skips the AST insert — but `state.sheets`
///   still records the entry. For our parity gate this is observed
///   nowhere (no real fixture has sheets without a consumer); we
///   nevertheless emit at end-of-body so a future fixture surfaces
///   no surprise.
///
/// Called from `babel_plugin.rs::visit_mut_program` AFTER the runtime
/// import + React + forwardRef injections — those unshift items at
/// `body[0]`, so the "first non-import" target shifts only by one
/// `forEach` of imports, which the index recompute here handles.
pub fn emit_hoisted_sheets(module: &mut Module, state: &State) {
    let sheets = state.sheets();
    if sheets.is_empty() {
        return;
    }
    // Find the index of the first non-ImportDeclaration body item.
    let insert_idx = module
        .body
        .iter()
        .position(|item| {
            !matches!(
                item,
                ModuleItem::ModuleDecl(ModuleDecl::Import(_))
            )
        })
        .unwrap_or(module.body.len());

    // Iterate state.sheets() in insertion order, building each
    // `const <name> = "<sheet>";` and inserting at the running idx.
    for (offset, (sheet_text, hoisted_name)) in sheets.iter().enumerate() {
        let var_decl = VarDecl {
            span: DUMMY_SP,
            kind: VarDeclKind::Const,
            declare: false,
            decls: vec![VarDeclarator {
                span: DUMMY_SP,
                name: Pat::Ident(BindingIdent {
                    id: Ident::new(
                        hoisted_name.as_str().into(),
                        DUMMY_SP,
                        SyntaxContext::empty(),
                    ),
                    type_ann: None,
                }),
                init: Some(Box::new(Expr::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: sheet_text.as_str().into(),
                    raw: None,
                })))),
                definite: false,
            }],
            ctxt: SyntaxContext::empty(),
        };
        module.body.insert(
            insert_idx + offset,
            ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(var_decl)))),
        );
    }
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
            own_scope_override: None,
        }
    }

    #[test]
    fn first_hoist_mints_bare_underscore() {
        // §6.8a-iv: Babel's `scope.generateUidIdentifier('')` returns
        // `_` for the first call (no numeric suffix when i==1).
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let name = hoist_sheet("._abc{color:red}", &mut meta, &mut recorder);
        assert_eq!(name, "_");
    }

    #[test]
    fn distinct_sheets_get_distinct_uids() {
        // §6.8a-iv: first → `_`, second → `_2`, ... matching Babel's
        // `i > 1 ? '_<i>' : '_'` UID scheme.
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        let a = hoist_sheet("._a{color:red}", &mut meta, &mut recorder);
        let b = hoist_sheet("._b{color:blue}", &mut meta, &mut recorder);
        assert_eq!(a, "_");
        assert_eq!(b, "_2");
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
                assert_eq!(hoisted_name, "_");
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

    // ──────────────── §6.8a-ii — emit_hoisted_sheets ────────────────

    use swc_core::ecma::ast::ImportDecl;

    fn make_import_item(src: &str) -> ModuleItem {
        ModuleItem::ModuleDecl(ModuleDecl::Import(ImportDecl {
            span: DUMMY_SP,
            specifiers: vec![],
            src: Box::new(Str {
                span: DUMMY_SP,
                value: src.into(),
                raw: None,
            }),
            type_only: false,
            with: None,
            phase: swc_core::ecma::ast::ImportPhase::Evaluation,
        }))
    }

    fn make_dummy_var(name: &str) -> ModuleItem {
        // `const <name> = 1;` — used as a stand-in non-import body
        // item.
        use swc_core::ecma::ast::{BindingIdent, Decl, Number, Pat, Stmt};
        ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(VarDecl {
            span: DUMMY_SP,
            kind: VarDeclKind::Const,
            declare: false,
            ctxt: SyntaxContext::empty(),
            decls: vec![VarDeclarator {
                span: DUMMY_SP,
                name: Pat::Ident(BindingIdent {
                    id: Ident::new(name.into(), DUMMY_SP, SyntaxContext::empty()),
                    type_ann: None,
                }),
                init: Some(Box::new(Expr::Lit(Lit::Num(Number {
                    span: DUMMY_SP,
                    value: 1.0,
                    raw: None,
                })))),
                definite: false,
            }],
        }))))
    }

    fn assert_sheet_const(item: &ModuleItem, expected_name: &str, expected_text: &str) {
        let ModuleItem::Stmt(swc_core::ecma::ast::Stmt::Decl(Decl::Var(vd))) = item else {
            panic!("expected ModuleItem::Stmt(Decl::Var)");
        };
        assert_eq!(vd.kind, VarDeclKind::Const);
        assert_eq!(vd.decls.len(), 1);
        let decl = &vd.decls[0];
        let Pat::Ident(bi) = &decl.name else {
            panic!()
        };
        assert_eq!(bi.id.sym.as_ref(), expected_name);
        let Some(init) = decl.init.as_deref() else {
            panic!()
        };
        let Expr::Lit(Lit::Str(s)) = init else {
            panic!()
        };
        assert_eq!(s.value.to_atom_lossy().as_str(), expected_text);
    }

    #[test]
    fn emit_no_op_when_sheets_empty() {
        let mut state = State::default();
        let mut module = Module {
            span: DUMMY_SP,
            body: vec![make_import_item("react"), make_dummy_var("x")],
            shebang: None,
        };
        let before = module.body.len();
        emit_hoisted_sheets(&mut module, &state);
        assert_eq!(module.body.len(), before);
        let _ = &mut state; // silence unused-mut
    }

    #[test]
    fn emit_inserts_const_before_first_non_import() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        hoist_sheet("._a{color:red}", &mut meta, &mut recorder);
        hoist_sheet("._b{color:blue}", &mut meta, &mut recorder);

        let mut module = Module {
            span: DUMMY_SP,
            body: vec![
                make_import_item("react"),
                make_import_item("@compiled/react/runtime"),
                make_dummy_var("x"),
            ],
            shebang: None,
        };
        emit_hoisted_sheets(&mut module, &state);

        // Body now: [import, import, sheet0, sheet1, var].
        assert_eq!(module.body.len(), 5);
        assert_sheet_const(&module.body[2], "_", "._a{color:red}");
        assert_sheet_const(&module.body[3], "_2", "._b{color:blue}");
    }

    #[test]
    fn emit_appends_when_module_is_all_imports() {
        // Edge case: no non-import body items at all. Sheets land at
        // the end of the body. (Babel's behaviour SKIPS the AST insert
        // in this case; we emit defensively. No fixture surfaces this
        // shape today — the styled/css/keyframes handlers always
        // produce a consumer.)
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        hoist_sheet("._a{color:red}", &mut meta, &mut recorder);

        let mut module = Module {
            span: DUMMY_SP,
            body: vec![make_import_item("react")],
            shebang: None,
        };
        emit_hoisted_sheets(&mut module, &state);
        assert_eq!(module.body.len(), 2);
        assert_sheet_const(&module.body[1], "_", "._a{color:red}");
    }

    #[test]
    fn emit_preserves_indexmap_insertion_order() {
        let mut state = State::default();
        let mut recorder = MutationRecorder::new();
        let mut meta = fresh_meta(&mut state);
        hoist_sheet("c", &mut meta, &mut recorder);
        hoist_sheet("a", &mut meta, &mut recorder);
        hoist_sheet("b", &mut meta, &mut recorder);

        let mut module = Module {
            span: DUMMY_SP,
            body: vec![make_dummy_var("x")],
            shebang: None,
        };
        emit_hoisted_sheets(&mut module, &state);
        // Insertion order: c, a, b. Var stays last.
        assert_eq!(module.body.len(), 4);
        assert_sheet_const(&module.body[0], "_", "c");
        assert_sheet_const(&module.body[1], "_2", "a");
        assert_sheet_const(&module.body[2], "_3", "b");
    }
}
