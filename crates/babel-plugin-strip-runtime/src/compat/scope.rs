//! SWC analogue of Babel's `path.scope.getBinding(...).path.{node,remove()}`.
//!
//! Babel maintains a live scope chain during traversal; SWC plugins
//! don't have that out of the box. Strip-runtime only needs MODULE-LEVEL
//! string-binding lookup (Compiled hoists every `_n = "..."` style
//! declarator to the program top), so a flat one-pass index is
//! sufficient.
//!
//! Lifecycle, used from §1.4's dispatcher:
//!
//! 1. `ModuleScope::from_module(&program)` — build the index once.
//! 2. While visiting CC/CS sites, call `get_string_binding(name)` to
//!    extract the literal value. If found, `mark_for_removal(name)`.
//! 3. In `Program::exit`, call `apply_removals(&mut program)` to
//!    delete the marked declarators (and any var-decl statements they
//!    leave empty).

use indexmap::{IndexMap, IndexSet};
use swc_core::ecma::ast::{
    Decl, Expr, Lit, Module, ModuleDecl, ModuleItem, Pat, Stmt, VarDecl,
};

#[derive(Debug, Clone, Copy)]
pub struct BindingLocation {
    /// Index into `Module.body` of the containing item.
    pub item_index: usize,
    /// Index into `VarDecl.decls` of the declarator within that item.
    pub declarator_index: usize,
}

#[derive(Debug, Default)]
pub struct ModuleScope {
    /// `name` → `(location, optional cached string-init value)`.
    /// We cache the string value at build time so visitor lookups
    /// never need a `&Module` ref — that would conflict with the
    /// `&mut Module` we're holding during VisitMut traversal.
    bindings: IndexMap<String, (BindingLocation, Option<String>)>,
    pending: IndexSet<(usize, usize)>,
}

impl ModuleScope {
    /// One-pass scan of `Module.body`; only top-level
    /// `var/let/const X = ...;` (and `export var/let/const X = ...;`)
    /// declarators are indexed. Function declarations, class
    /// declarations, destructuring patterns, etc. are out of scope —
    /// strip-runtime never queries those.
    pub fn from_module(module: &Module) -> Self {
        let mut bindings: IndexMap<String, (BindingLocation, Option<String>)> = IndexMap::new();
        for (i, item) in module.body.iter().enumerate() {
            let var_decl = match var_decl_in_item(item) {
                Some(v) => v,
                None => continue,
            };
            for (j, declr) in var_decl.decls.iter().enumerate() {
                if let Pat::Ident(b) = &declr.name {
                    let cached = match declr.init.as_deref() {
                        Some(Expr::Lit(Lit::Str(s))) => {
                            // `Str.value` is `Wtf8Atom`. Style-rule
                            // strings from Compiled are valid UTF-8;
                            // the lossy conversion round-trips.
                            Some(s.value.to_atom_lossy().as_str().to_string())
                        }
                        _ => None,
                    };
                    bindings.insert(
                        b.id.sym.to_string(),
                        (
                            BindingLocation {
                                item_index: i,
                                declarator_index: j,
                            },
                            cached,
                        ),
                    );
                }
            }
        }
        Self {
            bindings,
            pending: IndexSet::new(),
        }
    }

    /// If `name` resolves to a top-level `const X = "literal"` (or
    /// `var X = "literal"` / `let X = "literal"`), return the literal
    /// value. Mirrors the upstream check
    /// `t.isVariableDeclarator(binding.path.node) && t.isStringLiteral(value)`.
    pub fn get_string_binding(&self, name: &str) -> Option<&str> {
        self.bindings.get(name).and_then(|(_, v)| v.as_deref())
    }

    /// Mark the declarator with this name for removal in
    /// `apply_removals`. No-op if `name` was never indexed.
    pub fn mark_for_removal(&mut self, name: &str) {
        if let Some((loc, _)) = self.bindings.get(name) {
            self.pending
                .insert((loc.item_index, loc.declarator_index));
        }
    }

    /// Apply every pending removal in two passes:
    ///   1. Drop marked declarators within their containing `VarDecl`.
    ///   2. Drop now-empty `var/let/const` statements from `Module.body`.
    /// Both passes iterate in DESCENDING index order so removal
    /// doesn't shift indices we still need.
    pub fn apply_removals(self, module: &mut Module) {
        let mut by_item: IndexMap<usize, Vec<usize>> = IndexMap::new();
        for (i, j) in self.pending {
            by_item.entry(i).or_default().push(j);
        }
        for (item_idx, mut decl_idxs) in by_item {
            decl_idxs.sort_unstable_by(|a, b| b.cmp(a));
            if let Some(item) = module.body.get_mut(item_idx) {
                if let Some(var_decl) = var_decl_in_item_mut(item) {
                    for j in decl_idxs {
                        if j < var_decl.decls.len() {
                            var_decl.decls.remove(j);
                        }
                    }
                }
            }
        }

        let empty_indices: Vec<usize> = module
            .body
            .iter()
            .enumerate()
            .filter_map(|(i, item)| match var_decl_in_item(item) {
                Some(v) if v.decls.is_empty() => Some(i),
                _ => None,
            })
            .collect();
        for i in empty_indices.into_iter().rev() {
            module.body.remove(i);
        }
    }
}

fn var_decl_in_item(item: &ModuleItem) -> Option<&VarDecl> {
    match item {
        ModuleItem::Stmt(Stmt::Decl(Decl::Var(v))) => Some(v),
        ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ed)) => match &ed.decl {
            Decl::Var(v) => Some(v),
            _ => None,
        },
        _ => None,
    }
}

fn var_decl_in_item_mut(item: &mut ModuleItem) -> Option<&mut VarDecl> {
    match item {
        ModuleItem::Stmt(Stmt::Decl(Decl::Var(v))) => Some(v),
        ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ed)) => match &mut ed.decl {
            Decl::Var(v) => Some(v),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use swc_core::common::{SyntaxContext, DUMMY_SP};
    use swc_core::ecma::ast::{
        BindingIdent, Ident, Str, VarDecl, VarDeclKind, VarDeclarator,
    };

    fn const_str(name: &str, value: &str) -> ModuleItem {
        ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(VarDecl {
            span: DUMMY_SP,
            ctxt: SyntaxContext::empty(),
            kind: VarDeclKind::Const,
            declare: false,
            decls: vec![VarDeclarator {
                span: DUMMY_SP,
                name: Pat::Ident(BindingIdent {
                    id: Ident::new(name.into(), DUMMY_SP, SyntaxContext::empty()),
                    type_ann: None,
                }),
                init: Some(Box::new(Expr::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: value.into(),
                    raw: None,
                })))),
                definite: false,
            }],
        }))))
    }

    fn const_pair_str(a: (&str, &str), b: (&str, &str)) -> ModuleItem {
        ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(VarDecl {
            span: DUMMY_SP,
            ctxt: SyntaxContext::empty(),
            kind: VarDeclKind::Const,
            declare: false,
            decls: vec![
                VarDeclarator {
                    span: DUMMY_SP,
                    name: Pat::Ident(BindingIdent {
                        id: Ident::new(a.0.into(), DUMMY_SP, SyntaxContext::empty()),
                        type_ann: None,
                    }),
                    init: Some(Box::new(Expr::Lit(Lit::Str(Str {
                        span: DUMMY_SP,
                        value: a.1.into(),
                        raw: None,
                    })))),
                    definite: false,
                },
                VarDeclarator {
                    span: DUMMY_SP,
                    name: Pat::Ident(BindingIdent {
                        id: Ident::new(b.0.into(), DUMMY_SP, SyntaxContext::empty()),
                        type_ann: None,
                    }),
                    init: Some(Box::new(Expr::Lit(Lit::Str(Str {
                        span: DUMMY_SP,
                        value: b.1.into(),
                        raw: None,
                    })))),
                    definite: false,
                },
            ],
        }))))
    }

    fn module(items: Vec<ModuleItem>) -> Module {
        Module {
            span: DUMMY_SP,
            body: items,
            shebang: None,
        }
    }

    #[test]
    fn finds_string_binding_at_top_level() {
        let m = module(vec![const_str("_1", "._abc{color:red}")]);
        let scope = ModuleScope::from_module(&m);
        assert_eq!(scope.get_string_binding("_1"), Some("._abc{color:red}"));
    }

    #[test]
    fn returns_none_for_unknown_name() {
        let m = module(vec![const_str("_1", "x")]);
        let scope = ModuleScope::from_module(&m);
        assert!(scope.get_string_binding("_99").is_none());
    }

    #[test]
    fn returns_none_for_non_string_init() {
        let m = module(vec![ModuleItem::Stmt(Stmt::Decl(Decl::Var(Box::new(
            VarDecl {
                span: DUMMY_SP,
                ctxt: SyntaxContext::empty(),
                kind: VarDeclKind::Const,
                declare: false,
                decls: vec![VarDeclarator {
                    span: DUMMY_SP,
                    name: Pat::Ident(BindingIdent {
                        id: Ident::new("_1".into(), DUMMY_SP, SyntaxContext::empty()),
                        type_ann: None,
                    }),
                    init: Some(Box::new(Expr::Lit(Lit::Num(
                        swc_core::ecma::ast::Number {
                            span: DUMMY_SP,
                            value: 42.0,
                            raw: None,
                        },
                    )))),
                    definite: false,
                }],
            },
        ))))]);
        let scope = ModuleScope::from_module(&m);
        assert!(scope.get_string_binding("_1").is_none());
    }

    #[test]
    fn apply_removals_drops_single_declarator() {
        let mut m = module(vec![
            const_str("_1", "._abc{color:red}"),
            const_str("_2", "._def{color:blue}"),
        ]);
        let mut scope = ModuleScope::from_module(&m);
        scope.mark_for_removal("_1");
        scope.apply_removals(&mut m);
        assert_eq!(m.body.len(), 1);
        let scope2 = ModuleScope::from_module(&m);
        assert!(scope2.get_string_binding("_1").is_none());
        assert_eq!(scope2.get_string_binding("_2"), Some("._def{color:blue}"));
    }

    #[test]
    fn apply_removals_drops_partial_var_decl() {
        // `const _a = "...", _b = "..."` — remove _a, keep _b. The
        // var-decl statement stays (still has one declarator).
        let mut m = module(vec![const_pair_str(
            ("_a", "rule_a"),
            ("_b", "rule_b"),
        )]);
        let mut scope = ModuleScope::from_module(&m);
        scope.mark_for_removal("_a");
        scope.apply_removals(&mut m);
        assert_eq!(m.body.len(), 1);
        let scope2 = ModuleScope::from_module(&m);
        assert!(scope2.get_string_binding("_a").is_none());
        assert_eq!(scope2.get_string_binding("_b"), Some("rule_b"));
    }

    #[test]
    fn mark_for_removal_unknown_name_is_noop() {
        let mut m = module(vec![const_str("_1", "x")]);
        let mut scope = ModuleScope::from_module(&m);
        scope.mark_for_removal("_does_not_exist");
        scope.apply_removals(&mut m);
        // Original binding survives untouched.
        assert_eq!(m.body.len(), 1);
        let scope2 = ModuleScope::from_module(&m);
        assert_eq!(scope2.get_string_binding("_1"), Some("x"));
    }
}
