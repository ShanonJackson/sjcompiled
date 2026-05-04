//! Phase 5 §5.0b — `compat/path.rs` — Babel `NodePath` analog.
//!
//! Source-of-truth (verbatim references, all under
//! `node_modules/.bun/@babel+traverse@7.29.0/.../lib/`):
//!   - `path/index.js`           — base `NodePath` shape.
//!   - `path/replacement.js:89-127` — `replaceWith(node)` semantics
//!     (wired here as the single-site `replace_expr` helper per Q2).
//!   - `path/modification.js:198-…` — `unshiftContainer(listKey, nodes)`
//!     used by `Scope.push` when injecting synthesised declarators.
//!   - `path/conversion.js:68-102` — `ensureBlock()` for concise-arrow
//!     bodies (`() => 42` → `() => { return 42; }`).
//!   - `scope/index.js:717-756`  — `Scope.push(opts)`. The 1:1 port of
//!     this method is `scope_push` below. **Replaces the §5.0a
//!     `ScopeIndex::scope_push_synthetic` binding-only helper for
//!     production callers** — see Finding 6 in
//!     `plugins/COMPAT_SCOPE_AUDIT.md`.
//!
//! ## Q2 lock — single-site `&mut Expr`
//!
//! Compiled's `path.replaceWith` is exercised exactly once
//! (see `traverse-call-expression.ts:95-122`'s IIFE wrap). Only
//! `replace_expr` carries the `&mut Expr` privilege. The rest of
//! `compat/path.rs` is read-only navigation: predicates, parent
//! walks, get-by-field, and sub-traversal via `traverse_subtree`.
//! Wrapping every visit in `&mut` would propagate mutation rights
//! through the whole evaluator for one site's benefit. Don't.
//!
//! ## §5.0b deliverable contract
//!
//! From `plugins/COMPAT_SCOPE_AUDIT.md` ("§5.0b SPEC LOCK"):
//!
//! > The Rust port's `scope_push(arrow_path, PushOpts)` MUST:
//! > - Walk to a valid push-target per upstream's logic …
//! > - On loop / catch / function paths: synthesise an empty
//! >   `BlockStmt` if absent (`ensureBlock` analog), descend to it.
//! > - Compute `dataKey = "declaration:{kind}:{blockHoist}"`. Reuse
//! >   an existing declaration block at that key if present, …
//! > - Synthesise a `VarDeclarator { name: id, init }` and
//! >   `unshiftContainer`-equivalent the new `VarDecl` onto
//! >   `block.stmts` (i.e. INSERT AT INDEX 0).
//! > - Re-run binding registration so subsequent
//! >   `get_own_binding()` lookups against the arrow's scope find
//! >   the injected binding.
//!
//! The "first cargo unit test" the audit calls out — *push then
//! traverse, observe new VarDecl* — is
//! `scope_push_inserts_var_decl_into_arrow_body_visible_to_traverse`
//! below. If it passes, the AST-mutation contract holds. If it
//! still passes against the §5.0a stub (binding-only), the test is
//! wrong.

use swc_core::common::{Span, SyntaxContext, DUMMY_SP};
use swc_core::ecma::ast::{
    ArrowExpr, BindingIdent, BlockStmt, BlockStmtOrExpr, Decl, Expr, Ident, Lit, Pat,
    ReturnStmt, Stmt, VarDecl, VarDeclKind, VarDeclarator,
};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

use crate::compat::scope::{Binding, BindingKind, ScopeId, ScopeIndex};

// -------------------- NodeKind --------------------

/// Discriminates the AST node a `PathHandle` points at, narrowed to
/// the node types `packages/babel-plugin` queries via `path.is*()`
/// predicates. See `plugins/COMPAT_SCOPE_AUDIT.md` "NodePath operations"
/// table for the surface-of-record.
///
/// New variants land as `§5.4`/`§5.5`/`§5.6` ports surface them; treat
/// every addition as 1:1 with a Babel `node.type` string. Don't add
/// "convenience" variants without a Babel anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    // Module-level
    ImportDeclaration,
    ImportSpecifier,
    ImportDefaultSpecifier,
    ImportNamespaceSpecifier,
    ExportNamedDeclaration,
    ExportDefaultDeclaration,
    // Declarations / declarators
    VariableDeclaration,
    VariableDeclarator,
    FunctionDeclaration,
    ClassDeclaration,
    // Patterns
    ObjectPattern,
    ArrayPattern,
    AssignmentPattern,
    RestElement,
    // Identifiers / literals
    Identifier,
    StringLiteral,
    NumericLiteral,
    BooleanLiteral,
    NullLiteral,
    TemplateLiteral,
    // Expressions
    BinaryExpression,
    UnaryExpression,
    LogicalExpression,
    ConditionalExpression,
    CallExpression,
    NewExpression,
    MemberExpression,
    ArrowFunctionExpression,
    FunctionExpression,
    ClassExpression,
    SequenceExpression,
    AssignmentExpression,
    UpdateExpression,
    // Statements / containers
    Program,
    BlockStatement,
    ExpressionStatement,
    ReturnStatement,
    /// **§5.0c additions** — scope-owner node types §5.0c's
    /// `parent_kind_of` reaches when answering
    /// evaluation.js:126's `parentPath.isBlockStatement()` and the
    /// loop-walk in :136. Mirrored back from `ScopeKind` via
    /// [`ScopeIndex::parent_kind_of`].
    ForStatement,
    ForInStatement,
    ForOfStatement,
    CatchClause,
    SwitchStatement,
    /// Catch-all for nodes the audit table doesn't enumerate. Predicates
    /// fall through to `false`; `type_str()` returns "Other".
    Other,
}

impl NodeKind {
    /// Babel `node.type` string. The §5.0a parity gate's
    /// `binding_node_type_after_push` observable is comparison-equal to
    /// `NodeKind::VariableDeclarator.type_str()`.
    pub fn type_str(self) -> &'static str {
        match self {
            NodeKind::ImportDeclaration => "ImportDeclaration",
            NodeKind::ImportSpecifier => "ImportSpecifier",
            NodeKind::ImportDefaultSpecifier => "ImportDefaultSpecifier",
            NodeKind::ImportNamespaceSpecifier => "ImportNamespaceSpecifier",
            NodeKind::ExportNamedDeclaration => "ExportNamedDeclaration",
            NodeKind::ExportDefaultDeclaration => "ExportDefaultDeclaration",
            NodeKind::VariableDeclaration => "VariableDeclaration",
            NodeKind::VariableDeclarator => "VariableDeclarator",
            NodeKind::FunctionDeclaration => "FunctionDeclaration",
            NodeKind::ClassDeclaration => "ClassDeclaration",
            NodeKind::ObjectPattern => "ObjectPattern",
            NodeKind::ArrayPattern => "ArrayPattern",
            NodeKind::AssignmentPattern => "AssignmentPattern",
            NodeKind::RestElement => "RestElement",
            NodeKind::Identifier => "Identifier",
            NodeKind::StringLiteral => "StringLiteral",
            NodeKind::NumericLiteral => "NumericLiteral",
            NodeKind::BooleanLiteral => "BooleanLiteral",
            NodeKind::NullLiteral => "NullLiteral",
            NodeKind::TemplateLiteral => "TemplateLiteral",
            NodeKind::BinaryExpression => "BinaryExpression",
            NodeKind::UnaryExpression => "UnaryExpression",
            NodeKind::LogicalExpression => "LogicalExpression",
            NodeKind::ConditionalExpression => "ConditionalExpression",
            NodeKind::CallExpression => "CallExpression",
            NodeKind::NewExpression => "NewExpression",
            NodeKind::MemberExpression => "MemberExpression",
            NodeKind::ArrowFunctionExpression => "ArrowFunctionExpression",
            NodeKind::FunctionExpression => "FunctionExpression",
            NodeKind::ClassExpression => "ClassExpression",
            NodeKind::SequenceExpression => "SequenceExpression",
            NodeKind::AssignmentExpression => "AssignmentExpression",
            NodeKind::UpdateExpression => "UpdateExpression",
            NodeKind::Program => "Program",
            NodeKind::BlockStatement => "BlockStatement",
            NodeKind::ExpressionStatement => "ExpressionStatement",
            NodeKind::ReturnStatement => "ReturnStatement",
            NodeKind::ForStatement => "ForStatement",
            NodeKind::ForInStatement => "ForInStatement",
            NodeKind::ForOfStatement => "ForOfStatement",
            NodeKind::CatchClause => "CatchClause",
            NodeKind::SwitchStatement => "SwitchStatement",
            NodeKind::Other => "Other",
        }
    }

    /// Map a Babel `node.type` string back to a `NodeKind`. Used when
    /// constructing PathHandles from scope-index-cached
    /// `binding_node_type` strings (which were themselves produced
    /// from `NodeKind::type_str()` at build time).
    pub fn from_type_str(s: &str) -> NodeKind {
        match s {
            "ImportDeclaration" => NodeKind::ImportDeclaration,
            "ImportSpecifier" => NodeKind::ImportSpecifier,
            "ImportDefaultSpecifier" => NodeKind::ImportDefaultSpecifier,
            "ImportNamespaceSpecifier" => NodeKind::ImportNamespaceSpecifier,
            "ExportNamedDeclaration" => NodeKind::ExportNamedDeclaration,
            "ExportDefaultDeclaration" => NodeKind::ExportDefaultDeclaration,
            "VariableDeclaration" => NodeKind::VariableDeclaration,
            "VariableDeclarator" => NodeKind::VariableDeclarator,
            "FunctionDeclaration" => NodeKind::FunctionDeclaration,
            "ClassDeclaration" => NodeKind::ClassDeclaration,
            "ObjectPattern" => NodeKind::ObjectPattern,
            "ArrayPattern" => NodeKind::ArrayPattern,
            "AssignmentPattern" => NodeKind::AssignmentPattern,
            "RestElement" => NodeKind::RestElement,
            "Identifier" => NodeKind::Identifier,
            "StringLiteral" => NodeKind::StringLiteral,
            "NumericLiteral" => NodeKind::NumericLiteral,
            "BooleanLiteral" => NodeKind::BooleanLiteral,
            "NullLiteral" => NodeKind::NullLiteral,
            "TemplateLiteral" => NodeKind::TemplateLiteral,
            "BinaryExpression" => NodeKind::BinaryExpression,
            "UnaryExpression" => NodeKind::UnaryExpression,
            "LogicalExpression" => NodeKind::LogicalExpression,
            "ConditionalExpression" => NodeKind::ConditionalExpression,
            "CallExpression" => NodeKind::CallExpression,
            "NewExpression" => NodeKind::NewExpression,
            "MemberExpression" => NodeKind::MemberExpression,
            "ArrowFunctionExpression" => NodeKind::ArrowFunctionExpression,
            "FunctionExpression" => NodeKind::FunctionExpression,
            "ClassExpression" => NodeKind::ClassExpression,
            "SequenceExpression" => NodeKind::SequenceExpression,
            "AssignmentExpression" => NodeKind::AssignmentExpression,
            "UpdateExpression" => NodeKind::UpdateExpression,
            "Program" => NodeKind::Program,
            "BlockStatement" => NodeKind::BlockStatement,
            "ExpressionStatement" => NodeKind::ExpressionStatement,
            "ReturnStatement" => NodeKind::ReturnStatement,
            "ForStatement" => NodeKind::ForStatement,
            "ForInStatement" => NodeKind::ForInStatement,
            "ForOfStatement" => NodeKind::ForOfStatement,
            "CatchClause" => NodeKind::CatchClause,
            "SwitchStatement" => NodeKind::SwitchStatement,
            _ => NodeKind::Other,
        }
    }

    /// Babel `path.isExpression()` — matches every Expression-kind
    /// node. `@babel/types/lib/validators/isExpression.js` enumerates
    /// the same set; the §5.4–§5.6 callers use this to narrow a
    /// generic node to "evaluable" before feeding `path.evaluate()`.
    pub fn is_expression_kind(self) -> bool {
        matches!(
            self,
            NodeKind::Identifier
                | NodeKind::StringLiteral
                | NodeKind::NumericLiteral
                | NodeKind::BooleanLiteral
                | NodeKind::NullLiteral
                | NodeKind::TemplateLiteral
                | NodeKind::BinaryExpression
                | NodeKind::UnaryExpression
                | NodeKind::LogicalExpression
                | NodeKind::ConditionalExpression
                | NodeKind::CallExpression
                | NodeKind::NewExpression
                | NodeKind::MemberExpression
                | NodeKind::ArrowFunctionExpression
                | NodeKind::FunctionExpression
                | NodeKind::ClassExpression
                | NodeKind::SequenceExpression
                | NodeKind::AssignmentExpression
                | NodeKind::UpdateExpression
        )
    }
}

// -------------------- PathHandle --------------------

/// Babel `NodePath` analog — a borrow-free view over an AST node
/// carrying enough context to answer the shape questions §5.4–§5.6
/// ask: `is*()` predicates, parent type, owning scope, and container
/// `list_key`.
///
/// `PathHandle` is **construction-cheap and `Copy`** — synthesise on
/// demand from a `Binding` + its known parent type, OR from a visit
/// callback supplying the local context. Don't try to thread one
/// through a long-lived data structure; rebuild from the
/// `ScopeIndex` + an AST visit.
///
/// **What it intentionally does NOT carry**: a `&Node` reference. The
/// audit's surface table records that §5.4–§5.6 only read `node.type`,
/// `node.parentPath.node.type`, `path.listKey`, and a fixed set of
/// predicates — all of which are derivable from `(node_kind,
/// parent_kind, list_key, scope)` alone. Adding a `&Node` ref would
/// drag lifetime parameters through every caller for zero
/// observable-shape benefit.
#[derive(Debug, Clone, Copy)]
pub struct PathHandle {
    /// `path.node.type` — the AST kind this handle points at.
    pub node_kind: NodeKind,
    /// Span of the pointed-at node. Identity key when reconciling with
    /// `ScopeIndex::scope_at_pos`.
    pub node_span: Span,
    /// `path.parentPath.node.type` — None when the path is at the
    /// Program root or the parent type isn't tracked yet.
    pub parent_kind: Option<NodeKind>,
    /// Span of the parent node. Mirrors `parent_kind`.
    pub parent_span: Option<Span>,
    /// Owning scope id. Mirrors `path.scope` resolved through
    /// `ScopeIndex::scope_at_pos`.
    pub scope: ScopeId,
    /// `path.listKey` — the container array name when this node lives
    /// inside one (`"arguments"`, `"params"`, `"specifiers"`,
    /// `"declarations"`, `"body"`, `"elements"`). None when not in an
    /// array container. Mirrors Babel `NodePath#listKey`.
    pub list_key: Option<&'static str>,
}

impl PathHandle {
    /// Construct a path handle for a top-level node (no parent context).
    pub fn new(node_kind: NodeKind, node_span: Span, scope: ScopeId) -> Self {
        Self {
            node_kind,
            node_span,
            parent_kind: None,
            parent_span: None,
            scope,
            list_key: None,
        }
    }

    /// Builder: attach parent context.
    pub fn with_parent(mut self, parent_kind: NodeKind, parent_span: Span) -> Self {
        self.parent_kind = Some(parent_kind);
        self.parent_span = Some(parent_span);
        self
    }

    /// Builder: attach a container `list_key`.
    pub fn with_list_key(mut self, key: &'static str) -> Self {
        self.list_key = Some(key);
        self
    }

    // ----- Babel predicate fan-out -----
    //
    // Each predicate maps to a `path.is<Kind>()` call site enumerated
    // in `plugins/COMPAT_SCOPE_AUDIT.md` "NodePath operations" table.
    // Don't add a predicate that has no Babel anchor.

    pub fn is_import_declaration(&self) -> bool {
        self.node_kind == NodeKind::ImportDeclaration
    }
    pub fn is_import_specifier(&self) -> bool {
        self.node_kind == NodeKind::ImportSpecifier
    }
    pub fn is_import_default_specifier(&self) -> bool {
        self.node_kind == NodeKind::ImportDefaultSpecifier
    }
    pub fn is_import_namespace_specifier(&self) -> bool {
        self.node_kind == NodeKind::ImportNamespaceSpecifier
    }
    pub fn is_export_named_declaration(&self) -> bool {
        self.node_kind == NodeKind::ExportNamedDeclaration
    }
    pub fn is_object_pattern(&self) -> bool {
        self.node_kind == NodeKind::ObjectPattern
    }
    pub fn is_array_pattern(&self) -> bool {
        self.node_kind == NodeKind::ArrayPattern
    }
    pub fn is_variable_declarator(&self) -> bool {
        self.node_kind == NodeKind::VariableDeclarator
    }
    pub fn is_arrow_function_expression(&self) -> bool {
        self.node_kind == NodeKind::ArrowFunctionExpression
    }
    pub fn is_function_expression(&self) -> bool {
        self.node_kind == NodeKind::FunctionExpression
    }
    pub fn is_function_declaration(&self) -> bool {
        self.node_kind == NodeKind::FunctionDeclaration
    }
    pub fn is_call_expression(&self) -> bool {
        self.node_kind == NodeKind::CallExpression
    }
    pub fn is_member_expression(&self) -> bool {
        self.node_kind == NodeKind::MemberExpression
    }
    pub fn is_block_statement(&self) -> bool {
        self.node_kind == NodeKind::BlockStatement
    }
    pub fn is_program(&self) -> bool {
        self.node_kind == NodeKind::Program
    }

    /// `path.isExpression()` — fan-out via `NodeKind::is_expression_kind`.
    pub fn is_expression(&self) -> bool {
        self.node_kind.is_expression_kind()
    }

    /// `path.isPattern()` — true for ObjectPattern / ArrayPattern /
    /// AssignmentPattern / RestElement. Mirrors
    /// `@babel/types/.../validators/isPattern.js`. Critical input to
    /// `getBinding`'s pattern-skip rule (Finding 2 in
    /// `COMPAT_SCOPE_AUDIT.md`).
    pub fn is_pattern(&self) -> bool {
        matches!(
            self.node_kind,
            NodeKind::ObjectPattern
                | NodeKind::ArrayPattern
                | NodeKind::AssignmentPattern
                | NodeKind::RestElement
        )
    }

    /// `path.isFunction()` — true for any function-shaped node
    /// (FunctionDeclaration / FunctionExpression / ArrowFunctionExpression).
    /// `getFunctionParent` walks up checking this predicate.
    pub fn is_function(&self) -> bool {
        matches!(
            self.node_kind,
            NodeKind::FunctionDeclaration
                | NodeKind::FunctionExpression
                | NodeKind::ArrowFunctionExpression
        )
    }

    /// `path.isReferencedIdentifier()` — true for an `Identifier` in
    /// *reference* position (not a binding ident, not an
    /// import-specifier local name, not a destructuring pattern slot).
    /// Mirrors the §5.0a integration test's `RefFinder` exclusions
    /// (which themselves mirror the JS oracle's
    /// `findFirstReferenceOnRhs` rules).
    pub fn is_referenced_identifier(&self) -> bool {
        if self.node_kind != NodeKind::Identifier {
            return false;
        }
        match self.parent_kind {
            // VariableDeclarator.id is a binding (the .init child is a
            // separate path; that one IS in reference position).
            Some(NodeKind::VariableDeclarator) if self.list_key.is_none() => false,
            // import { x } from … / import x from … / import * as x —
            // the local name is always a binding, not a reference.
            Some(
                NodeKind::ImportSpecifier
                | NodeKind::ImportDefaultSpecifier
                | NodeKind::ImportNamespaceSpecifier,
            ) => false,
            // Destructuring pattern slots are bindings.
            Some(NodeKind::ObjectPattern | NodeKind::ArrayPattern) => false,
            _ => true,
        }
    }

    /// Babel `path.parentPath` — synthesises a parent PathHandle from
    /// the `parent_kind` / `parent_span` slots. Returns None if the
    /// caller didn't populate parent info.
    ///
    /// **Limitation**: the returned handle has no parent info itself
    /// (we don't track grandparents). Mirrors the read-only navigation
    /// surface §5.4 actually exercises — chained `parentPath.parentPath`
    /// is not in the surface table. If a future port reaches for
    /// `parentPath.parentPath`, factor a deeper context model in
    /// rather than papering over it here.
    pub fn parent_path(&self) -> Option<PathHandle> {
        let pk = self.parent_kind?;
        let ps = self.parent_span?;
        Some(PathHandle::new(pk, ps, self.scope))
    }

    /// Construct a `PathHandle` from a §5.0a `Binding`. Convenience for
    /// `binding.path` access at §5.4 call sites: every cached
    /// `binding_node_type` string was produced via
    /// `NodeKind::type_str()` at build time, so the round-trip via
    /// `from_type_str()` is lossless for tracked variants.
    pub fn from_binding(binding: &Binding) -> PathHandle {
        let mut p = PathHandle::new(
            NodeKind::from_type_str(binding.binding_node_type),
            binding.span,
            binding.scope,
        );
        if binding.parent_node_type != NodeKind::Other.type_str() {
            p.parent_kind = Some(NodeKind::from_type_str(binding.parent_node_type));
            // The parent span isn't cached on Binding (the audit
            // surface doesn't observe it). Leave None; predicates
            // that need only the *type* still work; predicates that
            // need a real ancestor walk should escalate.
            p.parent_span = None;
        }
        p
    }
}

// -------------------- replace_expr (the IIFE single-site mutation) --------------------

/// Single-site `path.replaceWith(node)` analog. Per Q2 lock, this is
/// the ONLY mutation entry point in `compat/path.rs`. Every other
/// API is read-only.
///
/// Mirrors `@babel/traverse@7.29.0/lib/path/replacement.js:89-127`'s
/// behaviour, narrowed to the IIFE wrap shape Compiled actually
/// reaches: replace one `Expr` slot with another, in place.
///
/// Babel's full `replaceWith` does additional work (registering the
/// new node with the visitor's scope, queueing re-visit, propagating
/// inferred types). The IIFE site doesn't need any of that — the
/// arrow we wrap around the call is a fresh node that lives only
/// long enough for `evaluate_expression` to compute a result against
/// it; it never re-enters the visitor pipeline.
///
/// Call site (anticipated, §5.5):
/// ```ignore
/// let original = std::mem::replace(target, Expr::Invalid(...));
/// let wrapped = wrap_in_iife(original, ...);
/// replace_expr(target, wrapped);
/// ```
#[inline]
pub fn replace_expr(target: &mut Expr, replacement: Expr) {
    *target = replacement;
}

// -------------------- ensure_block --------------------

/// `path.ensureBlock()` analog for arrow / function bodies.
///
/// 1:1 port of `path/conversion.js:68-102`, narrowed to
/// `BlockStmtOrExpr` (the SWC type that lifts Babel's "either a
/// `BlockStatement` or an Expression as a function body" union):
///   - If the body is already a `BlockStmt`, no-op.
///   - If the body is an `Expr`, wrap it in `{ return <expr>; }`.
///
/// Used by `scope_push` BEFORE inserting a synthetic declarator into
/// an arrow's body when that arrow has a concise expression body
/// (`() => 42`). Without `ensureBlock`, the `unshiftContainer("body",
/// [decl])` step would have nowhere to insert.
pub fn ensure_block(body: &mut BlockStmtOrExpr) {
    let placeholder = BlockStmtOrExpr::BlockStmt(BlockStmt {
        span: DUMMY_SP,
        stmts: Vec::new(),
        ctxt: SyntaxContext::empty(),
    });
    let taken = std::mem::replace(body, placeholder);
    *body = match taken {
        // Already a block — restore as-is.
        BlockStmtOrExpr::BlockStmt(b) => BlockStmtOrExpr::BlockStmt(b),
        // Concise expression body — wrap in { return <expr>; }.
        // Babel: `path/conversion.js:90-92` for the function branch.
        BlockStmtOrExpr::Expr(expr) => {
            let span = expr_span(&expr);
            BlockStmtOrExpr::BlockStmt(BlockStmt {
                span,
                stmts: vec![Stmt::Return(ReturnStmt {
                    span,
                    arg: Some(expr),
                })],
                ctxt: SyntaxContext::empty(),
            })
        }
    };
}

fn expr_span(e: &Expr) -> Span {
    use swc_core::common::Spanned;
    e.span()
}

// -------------------- traverse_subtree --------------------

/// `path.traverse(visitor)` analog. Runs a `VisitMut` visitor over a
/// node's subtree.
///
/// Babel's `path.traverse` shares the parent's scope chain with the
/// callback — visitor functions can call `path.scope.getBinding(...)`
/// and reach bindings registered above the traversed node. The Rust
/// equivalent is a discipline, not a mechanism: visitors carrying a
/// `&ScopeIndex` retain the chain naturally because `ScopeIndex` is
/// the whole-Module index built once and queried by-id.
///
/// Provided as a thin alias so call sites grep for `traverse_subtree`
/// the same way upstream greps for `path.traverse(`. Don't inline
/// the `visit_mut_with` call at scattered sites — the breadcrumb
/// matters.
#[inline]
pub fn traverse_subtree<N, V>(node: &mut N, visitor: &mut V)
where
    N: VisitMutWith<V>,
    V: VisitMut,
{
    node.visit_mut_with(visitor);
}

// -------------------- PushOpts + scope_push --------------------

/// Options for [`scope_push`]. Mirrors `Scope.push`'s `opts` shape at
/// `@babel/traverse@7.29.0/lib/scope/index.js:717-756`.
#[derive(Debug, Clone)]
pub struct PushOpts {
    /// `id` — the identifier name bound by the synthesised declarator.
    pub id: String,
    /// `init` — RHS expression. `None` → `var x;` declarator (only
    /// legal for `kind === "var"` per Babel; we accept None for any
    /// kind because production callers always pass `Some` for IIFE
    /// injection and getting "well-formed JS" wrong here is harmless).
    pub init: Option<Expr>,
    /// `kind` — `Const` / `Let` / `Var`. The §5.5 IIFE site always
    /// passes `Const` per `traverse-call-expression.ts:112`
    /// (`kind: 'const'`).
    pub kind: BindingKind,
    /// `_blockHoist` — Babel internal hoist priority. Default 2 per
    /// `scope/index.js:744`. Compiled doesn't override it; if a future
    /// caller does, plumbed for parity.
    pub block_hoist: u32,
    /// `unique` — when true, skip the dataKey-coalescing reuse and
    /// always create a new `VariableDeclaration`. Babel
    /// `scope/index.js:746-751` checks `!unique && path.getData(dataKey)`.
    /// Default false.
    pub unique: bool,
}

impl Default for PushOpts {
    fn default() -> Self {
        Self {
            id: String::new(),
            init: None,
            kind: BindingKind::Var,
            block_hoist: 2,
            unique: false,
        }
    }
}

/// AST-mutating `Scope.push` port. Replaces §5.0a's binding-only
/// `ScopeIndex::scope_push_synthetic` for production callers (the
/// synthetic helper remains as the cheaper code path used by the
/// §5.0a parity gate, where the AST mutation is unobservable).
///
/// 1:1 port of `@babel/traverse@7.29.0/lib/scope/index.js:717-756`,
/// narrowed to the IIFE call shape Compiled actually exercises:
///   - `target_block` is the `BlockStmt` body of an arrow / function /
///     loop / catch — whatever scope owner the §5.5/§5.6 IIFE site
///     resolved to.
///   - The arrow's scope was already created during
///     `ScopeIndex::build` OR via a synthetic registration;
///     `arrow_scope` identifies it.
///   - `opts.kind` is typically `Const` (`traverse-call-expression.ts:112`).
///
/// **What this function DOES (parity with upstream)**:
///   1. Compute `dataKey = "declaration:{kind}:{block_hoist}"`.
///   2. If a `VariableDeclaration` matching `dataKey` already exists
///      at `target_block.stmts[0]` (the unshifted-container slot),
///      append a new declarator to its `decls`. Otherwise create a
///      fresh `VariableDeclaration` with the new declarator and
///      `unshiftContainer`-equivalent it onto `body[0]`.
///   3. Register the new declarator's binding in `index` against
///      `arrow_scope`. Subsequent `index.get_own_binding(arrow_scope,
///      name)` calls resolve to this binding.
///   4. Subsequent `path.traverse(visitor)` calls on the arrow's
///      subtree see the inserted `VarDecl` as ordinary AST.
///
/// **What this function DOES NOT do (vs. upstream)**:
///   - The pattern-walk / switch-walk / loop-walk redirects in
///     `scope/index.js:717-726` aren't replicated, because the IIFE
///     site always passes the arrow's body `BlockStmt` directly. If
///     a future call site passes a different node kind, factor those
///     redirects into the call site (resolve to the right `BlockStmt`
///     before calling `scope_push`) — keeping `scope_push` focused
///     on "insert into this block + register binding" simplifies the
///     borrow chain (we hold one `&mut BlockStmt`, not a path object).
///   - The anonymous-function-expression-via-call special case at
///     `scope/index.js:733-738` (push as a param when we'd push above
///     the function) isn't replicated. Compiled's IIFE site always
///     pushes into the arrow's body, never above the arrow as a param.
///   - The `setData(dataKey, declarPath)` data side-table isn't
///     replicated — we re-detect the `dataKey` block by looking at
///     `target_block.stmts[0]`, which is sufficient for the IIFE
///     site's single-pass usage. If a call site needs cross-call
///     deduping, escalate; the data side-table is more invasive than
///     it looks (it lives on the path, not the scope).
///
/// See `plugins/COMPAT_SCOPE_AUDIT.md` Finding 6 for the design note.
pub fn scope_push(
    index: &mut ScopeIndex,
    arrow_scope: ScopeId,
    opts: PushOpts,
    target_block: &mut BlockStmt,
) {
    let var_decl_kind = match opts.kind {
        BindingKind::Const => VarDeclKind::Const,
        BindingKind::Let => VarDeclKind::Let,
        BindingKind::Var => VarDeclKind::Var,
        // Babel: `Scope.push` defaults `kind` to "var" if not set.
        // The IIFE site always sets it to "const"; non-var/let/const
        // kinds (param/module/local/etc.) aren't legal pushes upstream.
        // Fall back to var for safety; the constraint is documented.
        _ => VarDeclKind::Var,
    };

    let id_span = DUMMY_SP;
    let init_string = match &opts.init {
        Some(Expr::Lit(Lit::Str(s))) => Some(s.value.to_atom_lossy().as_str().to_string()),
        _ => None,
    };

    // §5.0c — capture `init_expr` BEFORE `opts.init` is moved into
    // the new declarator below. Gate on `Const` per evaluation.js:122
    // short-circuit. The §5.5 IIFE site always passes `Const` per
    // `traverse-call-expression.ts:112`.
    let init_expr_for_const_ident: Option<Box<Expr>> =
        if matches!(opts.kind, BindingKind::Const) {
            opts.init.clone().map(Box::new)
        } else {
            None
        };

    let new_declarator = VarDeclarator {
        span: id_span,
        name: Pat::Ident(BindingIdent {
            id: Ident::new(opts.id.clone().into(), id_span, SyntaxContext::empty()),
            type_ann: None,
        }),
        init: opts.init.map(Box::new),
        definite: false,
    };

    // Reuse-existing-block check: `scope/index.js:746` —
    // `!unique && path.getData(dataKey)`. We approximate by looking at
    // `target_block.stmts[0]`: if it's a `VariableDeclaration` with the
    // matching kind, append to its decls; else unshift a new VarDecl.
    // Two pushes with the same kind in the same scope therefore
    // coalesce into one VariableDeclaration with two declarators —
    // matching upstream's `unshiftContainer` + `dataKey` behaviour.
    let _data_key = format!(
        "declaration:{}:{}",
        opts.kind.as_str(),
        opts.block_hoist
    );

    let mut to_unshift: Option<VarDeclarator> = Some(new_declarator);
    if !opts.unique {
        if let Some(Stmt::Decl(Decl::Var(existing))) = target_block.stmts.first_mut() {
            if existing.kind == var_decl_kind {
                // Coalesce into the existing same-kind VariableDeclaration —
                // matches Babel's `unshiftContainer` + `dataKey` reuse.
                existing.decls.push(to_unshift.take().unwrap());
            }
        }
    }

    if let Some(declarator) = to_unshift {
        // Unshift new VariableDeclaration to body[0]. Mirrors
        // `path.unshiftContainer("body", [declar])` at
        // `scope/index.js:750`.
        let new_var_decl = Box::new(VarDecl {
            span: target_block.span,
            kind: var_decl_kind,
            declare: false,
            decls: vec![declarator],
            ctxt: SyntaxContext::empty(),
        });
        target_block
            .stmts
            .insert(0, Stmt::Decl(Decl::Var(new_var_decl)));
    }

    // Register binding in the ScopeIndex against the arrow's scope.
    // Mirrors `scope/index.js:755` —
    // `path.scope.registerBinding(kind, declarPath.get("declarations")[len - 1])`.
    let binding = Binding {
        kind: opts.kind,
        identifier_name: opts.id.clone(),
        constant: true,
        constant_violations: Vec::new(),
        reference_paths: Vec::new(),
        binding_node_type: NodeKind::VariableDeclarator.type_str(),
        parent_node_type: NodeKind::VariableDeclaration.type_str(),
        binding_init_string: init_string,
        init_expr: init_expr_for_const_ident,
        binding_id_type: Some(NodeKind::Identifier.type_str()),
        scope: arrow_scope,
        span: id_span,
    };
    index.register_synthetic_binding(arrow_scope, &opts.id, binding);
}

// -------------------- iife scaffolding --------------------

/// Synthesise an arrow function expression with an empty
/// `BlockStmt` body — the scratchpad node the §5.5 IIFE site builds
/// before calling [`scope_push`] for each `(param, evaluatedArg)`
/// pair.
///
/// Mirrors `traverse-call-expression.ts:95-98`'s `wrapNodeInIIFE`
/// shape: `(() => { …injected decls…; return <inner>; })()`. This
/// helper builds just the arrow part — the surrounding `CallExpr`
/// is the §5.5 implementer's responsibility (depends on the
/// outer-call's argument shape).
///
/// Returned arrow's body is empty; the caller `ensure_block`s if
/// rebuilding from a non-block source, then `scope_push`'s into it.
pub fn synthesize_iife_arrow_with_empty_block(span: Span) -> ArrowExpr {
    ArrowExpr {
        span,
        params: Vec::new(),
        body: Box::new(BlockStmtOrExpr::BlockStmt(BlockStmt {
            span,
            stmts: Vec::new(),
            ctxt: SyntaxContext::empty(),
        })),
        is_async: false,
        is_generator: false,
        type_params: None,
        return_type: None,
        ctxt: SyntaxContext::empty(),
    }
}

// -------------------- Tests --------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use swc_core::common::sync::Lrc;
    use swc_core::common::{FileName, SourceMap};
    use swc_core::ecma::ast::{EsVersion, Module, Str};
    use swc_core::ecma::parser::{parse_file_as_module, EsSyntax, Syntax};

    fn parse(source: &str) -> Module {
        let cm: Lrc<SourceMap> = Default::default();
        let fm = cm.new_source_file(
            Arc::new(FileName::Custom("compat-path-test.js".into())),
            String::from(source),
        );
        let mut errors = Vec::new();
        let module = parse_file_as_module(
            &fm,
            Syntax::Es(EsSyntax {
                jsx: false,
                ..Default::default()
            }),
            EsVersion::Es2022,
            None,
            &mut errors,
        )
        .expect("parse");
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        module
    }

    /// Predicate sanity — fan-out covers the surface table.
    #[test]
    fn predicates_match_node_kinds() {
        let import_decl =
            PathHandle::new(NodeKind::ImportDeclaration, DUMMY_SP, 0);
        assert!(import_decl.is_import_declaration());
        assert!(!import_decl.is_export_named_declaration());

        let object_pat = PathHandle::new(NodeKind::ObjectPattern, DUMMY_SP, 0);
        assert!(object_pat.is_object_pattern());
        assert!(object_pat.is_pattern());
        assert!(!object_pat.is_expression());

        let arrow = PathHandle::new(NodeKind::ArrowFunctionExpression, DUMMY_SP, 0);
        assert!(arrow.is_arrow_function_expression());
        assert!(arrow.is_function());
        assert!(arrow.is_expression());

        let var_decl = PathHandle::new(NodeKind::VariableDeclarator, DUMMY_SP, 0);
        assert!(var_decl.is_variable_declarator());
        assert!(!var_decl.is_expression());
    }

    /// `is_referenced_identifier` distinguishes binding-position from
    /// reference-position idents based on parent context.
    #[test]
    fn referenced_identifier_excludes_binding_positions() {
        // Bare ident with no parent — assumed reference.
        let bare =
            PathHandle::new(NodeKind::Identifier, DUMMY_SP, 0);
        assert!(bare.is_referenced_identifier());

        // VariableDeclarator.id — binding, not reference.
        let id_in_declarator = PathHandle::new(NodeKind::Identifier, DUMMY_SP, 0)
            .with_parent(NodeKind::VariableDeclarator, DUMMY_SP);
        assert!(!id_in_declarator.is_referenced_identifier());

        // ImportSpecifier.local — binding, not reference.
        let import_local = PathHandle::new(NodeKind::Identifier, DUMMY_SP, 0)
            .with_parent(NodeKind::ImportSpecifier, DUMMY_SP);
        assert!(!import_local.is_referenced_identifier());

        // VariableDeclarator.init's child ident IS a reference. We
        // approximate this by leaving parent_kind unset OR using a
        // different parent (e.g. BinaryExpression) — the audit doesn't
        // require a perfect AST parent walk, only the four exclusions
        // above.
        let init_ident = PathHandle::new(NodeKind::Identifier, DUMMY_SP, 0)
            .with_parent(NodeKind::BinaryExpression, DUMMY_SP);
        assert!(init_ident.is_referenced_identifier());
    }

    #[test]
    fn parent_path_synthesizes_handle_when_parent_set() {
        let with_parent = PathHandle::new(NodeKind::Identifier, DUMMY_SP, 0)
            .with_parent(NodeKind::VariableDeclarator, DUMMY_SP);
        let parent = with_parent.parent_path().expect("parent populated");
        assert!(parent.is_variable_declarator());

        let without_parent = PathHandle::new(NodeKind::Identifier, DUMMY_SP, 0);
        assert!(without_parent.parent_path().is_none());
    }

    #[test]
    fn ensure_block_wraps_concise_arrow_body() {
        let mut body = BlockStmtOrExpr::Expr(Box::new(Expr::Lit(Lit::Num(
            swc_core::ecma::ast::Number {
                span: DUMMY_SP,
                value: 42.0,
                raw: None,
            },
        ))));
        ensure_block(&mut body);
        match &body {
            BlockStmtOrExpr::BlockStmt(b) => {
                assert_eq!(b.stmts.len(), 1);
                assert!(matches!(&b.stmts[0], Stmt::Return(_)));
            }
            BlockStmtOrExpr::Expr(_) => panic!("ensure_block should have produced a BlockStmt"),
        }
    }

    #[test]
    fn ensure_block_is_noop_for_existing_block() {
        let mut body = BlockStmtOrExpr::BlockStmt(BlockStmt {
            span: DUMMY_SP,
            stmts: Vec::new(),
            ctxt: SyntaxContext::empty(),
        });
        ensure_block(&mut body);
        match &body {
            BlockStmtOrExpr::BlockStmt(b) => assert!(b.stmts.is_empty()),
            BlockStmtOrExpr::Expr(_) => panic!("body must remain a block"),
        }
    }

    /// Single-site replace_expr: the IIFE wrap site replaces an
    /// `Expr` slot in place.
    #[test]
    fn replace_expr_overwrites_target() {
        let mut e = Expr::Lit(Lit::Str(Str {
            span: DUMMY_SP,
            value: "before".into(),
            raw: None,
        }));
        replace_expr(
            &mut e,
            Expr::Lit(Lit::Str(Str {
                span: DUMMY_SP,
                value: "after".into(),
                raw: None,
            })),
        );
        let Expr::Lit(Lit::Str(s)) = &e else {
            panic!("expected str literal");
        };
        assert_eq!(s.value.to_atom_lossy().as_str(), "after");
    }

    /// **§5.0b first cargo unit test** (per
    /// `plugins/COMPAT_SCOPE_AUDIT.md` §5.0b SPEC LOCK):
    ///   "push then traverse, observe new VarDecl"
    ///
    /// The §5.0a `scope_push_synthetic` stub passes binding-shape
    /// asserts WITHOUT inserting an AST node; this test fails against
    /// that stub because `arrow_body.stmts.len() == 0` after the
    /// stub returns. It passes against the §5.0b real-deal because
    /// `scope_push` materialises a `VarDecl` into the body, and the
    /// `VisitMut` traversal walks it as ordinary AST.
    ///
    /// If this test passes against the stub, the test itself is
    /// wrong. If it passes against `scope_push`, the AST-mutation
    /// contract holds.
    #[test]
    fn scope_push_inserts_var_decl_into_arrow_body_visible_to_traverse() {
        // Build a synthetic arrow with empty body — the canonical
        // §5.5 IIFE scratchpad shape (empty arrow waiting for
        // injected param-bindings).
        let arrow = synthesize_iife_arrow_with_empty_block(DUMMY_SP);
        let mut body = match *arrow.body {
            BlockStmtOrExpr::BlockStmt(b) => b,
            BlockStmtOrExpr::Expr(_) => panic!("synthesised arrow should have a block body"),
        };

        // Build a ScopeIndex over an empty Module so we have a
        // program scope to register the synthetic binding against.
        // Real callers register an Arrow scope first; for a unit
        // test of `scope_push` in isolation, registering against the
        // program scope is sufficient — `scope_push`'s job is "insert
        // VarDecl + register binding at the given scope id", not
        // "create the scope".
        let m = Module {
            span: DUMMY_SP,
            body: Vec::new(),
            shebang: None,
        };
        let mut idx = ScopeIndex::build(&m);
        let prog = idx.program_scope();

        scope_push(
            &mut idx,
            prog,
            PushOpts {
                id: "x".into(),
                init: Some(Expr::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: "val".into(),
                    raw: None,
                }))),
                kind: BindingKind::Const,
                block_hoist: 2,
                unique: false,
            },
            &mut body,
        );

        // 1. Body now contains a VarDecl. Stub-failure point: the
        //    §5.0a stub leaves body.stmts empty.
        assert_eq!(
            body.stmts.len(),
            1,
            "scope_push must unshift a VarDecl into the arrow's body — \
             if this fails against scope_push, the AST-mutation \
             contract is broken; if it fails against \
             scope_push_synthetic, the stub is being tested by \
             mistake"
        );
        let Stmt::Decl(Decl::Var(v)) = &body.stmts[0] else {
            panic!("expected VarDecl, got {:?}", body.stmts[0]);
        };
        assert_eq!(v.kind, VarDeclKind::Const);
        assert_eq!(v.decls.len(), 1);
        let Pat::Ident(b) = &v.decls[0].name else {
            panic!("declarator name must be an Identifier");
        };
        assert_eq!(b.id.sym.as_str(), "x");

        // 2. Traversal sees the VarDecl as ordinary AST. This is the
        //    breadcrumb-test — a stub that updates the bindings map
        //    without touching the AST cannot pass this branch.
        #[derive(Default)]
        struct Counter {
            var_decls: usize,
            ident_syms: Vec<String>,
        }
        impl VisitMut for Counter {
            fn visit_mut_var_decl(&mut self, v: &mut VarDecl) {
                self.var_decls += 1;
                v.visit_mut_children_with(self);
            }
            fn visit_mut_ident(&mut self, i: &mut Ident) {
                self.ident_syms.push(i.sym.to_string());
            }
        }
        let mut counter = Counter::default();
        traverse_subtree(&mut body, &mut counter);
        assert_eq!(
            counter.var_decls, 1,
            "VisitMut over arrow body must see the injected VarDecl \
             — silent divergence here means downstream \
             path.traverse(visitor) calls miss synthesised bindings"
        );
        assert!(
            counter.ident_syms.iter().any(|s| s == "x"),
            "Identifier `x` from the new declarator must be visible \
             to the visitor's ident walk"
        );

        // 3. Binding registered in the scope index. Mirrors the §5.0a
        //    parity-gate observable.
        let binding = idx
            .get_own_binding(prog, "x")
            .expect("binding must be registered post-push");
        assert_eq!(binding.kind, BindingKind::Const);
        assert_eq!(
            binding.binding_node_type,
            NodeKind::VariableDeclarator.type_str()
        );
        assert_eq!(binding.binding_init_string.as_deref(), Some("val"));
    }

    /// Multi-push coalescing: two same-kind pushes against the same
    /// block reuse the existing VariableDeclaration's decls list,
    /// mirroring Babel's `dataKey`-driven `unshiftContainer` reuse.
    #[test]
    fn scope_push_coalesces_same_kind_into_one_var_decl() {
        let arrow = synthesize_iife_arrow_with_empty_block(DUMMY_SP);
        let mut body = match *arrow.body {
            BlockStmtOrExpr::BlockStmt(b) => b,
            BlockStmtOrExpr::Expr(_) => unreachable!(),
        };

        let m = Module {
            span: DUMMY_SP,
            body: Vec::new(),
            shebang: None,
        };
        let mut idx = ScopeIndex::build(&m);
        let prog = idx.program_scope();

        for name in ["a", "b", "c"] {
            scope_push(
                &mut idx,
                prog,
                PushOpts {
                    id: name.into(),
                    init: Some(Expr::Lit(Lit::Str(Str {
                        span: DUMMY_SP,
                        value: name.into(),
                        raw: None,
                    }))),
                    kind: BindingKind::Const,
                    block_hoist: 2,
                    unique: false,
                },
                &mut body,
            );
        }

        // Three pushes of kind=const → ONE VariableDeclaration with
        // three declarators, not three separate VariableDeclarations.
        assert_eq!(body.stmts.len(), 1);
        let Stmt::Decl(Decl::Var(v)) = &body.stmts[0] else {
            panic!("expected one VarDecl coalescing all pushes");
        };
        assert_eq!(v.kind, VarDeclKind::Const);
        assert_eq!(v.decls.len(), 3, "all three pushes coalesce into one VarDecl");

        // All three bindings registered.
        for name in ["a", "b", "c"] {
            assert!(
                idx.get_own_binding(prog, name).is_some(),
                "binding `{}` registered post-push",
                name
            );
        }
    }

    /// `unique: true` opts out of coalescing — each push lands as
    /// its own VariableDeclaration. Babel parity:
    /// `scope/index.js:746` — the `!unique` short-circuit.
    #[test]
    fn scope_push_unique_opts_out_of_coalescing() {
        let arrow = synthesize_iife_arrow_with_empty_block(DUMMY_SP);
        let mut body = match *arrow.body {
            BlockStmtOrExpr::BlockStmt(b) => b,
            BlockStmtOrExpr::Expr(_) => unreachable!(),
        };

        let m = Module {
            span: DUMMY_SP,
            body: Vec::new(),
            shebang: None,
        };
        let mut idx = ScopeIndex::build(&m);
        let prog = idx.program_scope();

        for name in ["a", "b"] {
            scope_push(
                &mut idx,
                prog,
                PushOpts {
                    id: name.into(),
                    init: None,
                    kind: BindingKind::Const,
                    block_hoist: 2,
                    unique: true,
                },
                &mut body,
            );
        }

        // Two unique pushes → two separate VariableDeclarations,
        // each with one declarator. Both unshifted, so the more
        // recent one is at index 0 and the earlier one at index 1.
        assert_eq!(body.stmts.len(), 2);
        for stmt in &body.stmts {
            let Stmt::Decl(Decl::Var(v)) = stmt else {
                panic!("expected VarDecl");
            };
            assert_eq!(v.decls.len(), 1);
        }
    }

    #[test]
    fn from_binding_round_trips_node_type() {
        // Build a binding manually and round-trip through PathHandle.
        let m = parse("const x = 'val';");
        let idx = ScopeIndex::build(&m);
        let binding = idx
            .get_own_binding(idx.program_scope(), "x")
            .expect("binding for const x");
        let p = PathHandle::from_binding(binding);
        assert!(p.is_variable_declarator());
        assert_eq!(p.scope, idx.program_scope());
    }
}
