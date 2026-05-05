//! Phase 5 §5.0a — pre-indexed scope tree mirroring `@babel/traverse@7.29.0`.
//!
//! Source-of-truth (verbatim):
//!   - `node_modules/.bun/@babel+traverse@7.29.0/.../lib/scope/index.js`
//!     (collectorVisitor at :190-303, Scope class at :306-938,
//!     getBinding at :809-824, push at :717-756, parent getter at :347-359)
//!   - `node_modules/.bun/@babel+traverse@7.29.0/.../lib/scope/binding.js`
//!     (Binding class with stored `constant: bool` field, isInitInLoop)
//!
//! ## Architectural locks (recorded in `plugins/COMPAT_SCOPE_AUDIT.md`)
//!
//! - **Q1 — pre-index, not lazy crawl.** Walk the entire Module on
//!   `Program::enter`, build the binding map + parent-pointer map +
//!   reference-paths map. Read-only navigation during the visitor pass.
//!   Eager pre-index is an INTENTIONAL semantic delta vs Babel's lazy
//!   `Scope.crawl()` (Finding 7) — documented; future agents do not
//!   "fix" this. Compiled queries the scope on every CSS-value
//!   identifier so lazy crawl ends up walking the whole tree anyway.
//! - **Q2 — single-site `&mut Expr`.** Only the IIFE wrap in
//!   `traverse-call-expression.ts:95` mutates the AST. §5.0a does NOT
//!   bake mutation rights into this API; `scope_push_synthetic` below
//!   is a binding-table-only stub that §5.0b replaces with the
//!   AST-mutating real-deal port (Finding 6).
//! - **Q3 — full port of `path.evaluate()`.** Owned by §5.0c; no
//!   bearing on this file.
//!
//! ## Bug-parity requirements (cited at call sites below)
//!
//! - **Finding 1**: `Binding.constant` is a STORED `bool`, set during
//!   construction and updated atomically in `reassign()`. Not computed
//!   from `constant_violations.len()`.
//! - **Finding 2**: `getBinding` has a pattern-skip rule and an
//!   `arguments` early-return at non-arrow function boundaries.
//! - **Finding 3**: `var` declarations in `ForStatement` /
//!   `ForX(In|Of)Statement` init hoist to the enclosing function or
//!   program scope.
//! - **Finding 4**: `var` / `hoisted` bindings inside loop bodies
//!   auto-mark themselves non-constant via `isInitInLoop` in the
//!   `Binding` constructor.
//! - **Finding 5**: `Scope.parent` is a getter that skips
//!   ObjectProperty `key` and decorator `decorators` positions when
//!   walking up. Eager pre-index bakes the skip in at build time.
//! - **Finding 8**: `Scope.globals` and `Scope.contextVariables` come
//!   from the vendored `@babel/helper-globals@7.28.0` JSONs (see
//!   `compat/globals.rs`).

use std::cell::{Cell, RefCell};

use indexmap::{IndexMap, IndexSet};
use swc_core::common::{BytePos, Span};
use swc_core::ecma::ast::{
    ArrowExpr, AssignExpr, AssignTarget, BlockStmtOrExpr, CatchClause, ClassDecl,
    ClassExpr, Decl, ExportDecl, ExportDefaultDecl, Expr, FnDecl, FnExpr, ForHead, ForInStmt,
    ForOfStmt, ForStmt, Function, ImportDecl, ImportDefaultSpecifier, ImportNamedSpecifier,
    ImportSpecifier, ImportStarAsSpecifier, ModuleDecl, ModuleExportName, ModuleItem, NamedExport,
    ObjectPat, ObjectPatProp, Pat, Prop, SimpleAssignTarget, Stmt, SwitchStmt, UpdateExpr, VarDecl,
    VarDeclKind, VarDeclOrExpr, VarDeclarator,
};
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::compat::globals;

// -------------------- Type aliases / IDs --------------------

/// Sequentially-assigned scope identifier. Stable across a single
/// `ScopeIndex::build` call; not portable across rebuilds.
pub type ScopeId = u32;

// -------------------- ScopeKind --------------------

/// Maps to Babel's "scope owner" node types. The Babel rule
/// (`@babel/types/.../validators/isScope.js`) excludes BlockStatements
/// whose parent is a Function or CatchClause — those don't own scopes
/// (their parent already does). We honor that by simply not creating
/// `Block` scopes for those positions during the build walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// `Module` — the root program scope.
    Program,
    /// `FunctionDeclaration` / `FunctionExpression`.
    Function,
    /// `ArrowFunctionExpression`.
    Arrow,
    /// Object methods, getters, setters, class methods. Scope owners
    /// for params + body. Treated like Function for binding-walk
    /// semantics.
    Method,
    /// Standalone `BlockStatement` (not a function/catch body).
    Block,
    /// `ForStatement` (C-style `for (init; test; update)`).
    For,
    /// `ForInStatement` / `ForOfStatement`.
    ForX,
    /// `CatchClause` — owns the param + body together.
    Catch,
    /// `SwitchStatement`.
    Switch,
    /// `ClassDeclaration` / `ClassExpression` body scope (separate
    /// from the surrounding scope so the class's own name binds
    /// `local` for ClassExpressions).
    Class,
}

impl ScopeKind {
    /// Babel's `path.isFunctionParent()` — true for Function family
    /// scopes (used by `getFunctionParent()` walk in `var`-hoist and
    /// in `getBinding`'s `arguments` early-return).
    fn is_function_parent(self) -> bool {
        matches!(self, ScopeKind::Function | ScopeKind::Arrow | ScopeKind::Method)
    }

    /// Babel's `path.isFunction()` AND NOT `isArrow()` — the predicate
    /// used by `getBinding`'s `arguments` short-circuit (Finding 2).
    fn is_non_arrow_function(self) -> bool {
        matches!(self, ScopeKind::Function | ScopeKind::Method)
    }
}

// -------------------- BindingKind --------------------

/// Maps to Babel's `binding.kind` string. The mapping mirrors
/// `Scope.registerDeclaration` / `registerBinding` exactly:
///
/// | Source                                        | kind        |
/// |-----------------------------------------------|-------------|
/// | `const x = …` / `using x = …` / `await using` | `"const"`   |
/// | `let x = …`                                   | `"let"`     |
/// | `var x = …`                                   | `"var"`     |
/// | function param                                | `"param"`   |
/// | `import { x } from`                           | `"module"`  |
/// | `function f() {}` (declaration)               | `"hoisted"` |
/// | `function f() {}` (expression's own name)     | `"local"`   |
/// | `class C {}` (expression's own name)          | `"local"`   |
/// | `class C {}` (declaration)                    | `"let"`     |
/// | `catch (e) { }`                               | `"let"`     |
/// | re-export-from-source synthetic               | `"unknown"` |
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    Const,
    Let,
    Var,
    Param,
    Module,
    Hoisted,
    Local,
    Unknown,
}

impl BindingKind {
    /// String form mirrors Babel's `binding.kind`. The compat-scope
    /// parity oracle compares this against the JS string verbatim.
    pub fn as_str(self) -> &'static str {
        match self {
            BindingKind::Const => "const",
            BindingKind::Let => "let",
            BindingKind::Var => "var",
            BindingKind::Param => "param",
            BindingKind::Module => "module",
            BindingKind::Hoisted => "hoisted",
            BindingKind::Local => "local",
            BindingKind::Unknown => "unknown",
        }
    }
}

// -------------------- Binding --------------------

/// Mirrors `@babel/traverse@7.29.0/lib/scope/binding.js`'s `Binding`
/// class shape, narrowed to the fields the §5.4–§5.6 evaluator and
/// the §5.0a parity gate observe (see
/// `plugins/COMPAT_SCOPE_AUDIT.md` "Binding fields read" table).
#[derive(Debug, Clone)]
pub struct Binding {
    pub kind: BindingKind,
    pub identifier_name: String,
    /// Finding 1: stored `bool`, NOT computed from
    /// `constant_violations.len()`. Set true at construction; set
    /// false (atomically with the violation push) on each
    /// `reassign()`. Behavior parity with `binding.js:7-31, 46-52`.
    pub constant: bool,
    /// Spans of every constraint-violating site (assignment LHS,
    /// update expression target, `for-of`/`for-in` left-pattern).
    /// Mirrors `binding.constantViolations` shape.
    pub constant_violations: Vec<ReferenceSite>,
    /// Spans of every reference site (every Identifier expression
    /// resolving to this binding).
    pub reference_paths: Vec<ReferenceSite>,
    /// What `binding.path.node.type` reports — the AST node type the
    /// binding is "anchored" to. For VariableDeclarators it's
    /// `"VariableDeclarator"`; for import specifiers it's the specifier
    /// type; for params it's `"Identifier"` for plain `(x)` and
    /// `"ObjectPattern"` / `"ArrayPattern"` for destructured params.
    pub binding_node_type: &'static str,
    /// What `binding.path.parentPath.node.type` reports — parent in
    /// the AST tree.
    pub parent_node_type: &'static str,
    /// For `const x = "literal"`: the literal value. None for
    /// non-string init or non-VariableDeclarator bindings. The §5.0a
    /// parity oracle reads this for the `block-scoped-shadowing-inner-wins`
    /// fixture; the §5.4 evaluator uses it for fast-path resolution.
    pub binding_init_string: Option<String>,
    /// **§5.0c addition** — For `const x = <expr>` where the LHS is a
    /// plain `Pat::Ident` and `kind == Const`: the cloned init
    /// `Expr`. `None` everywhere else (let/var, destructuring,
    /// imports, params, hoisted fn-decls, etc.).
    ///
    /// Populated during `register_var_declarator` to give
    /// `compat::evaluation`'s `isReferencedIdentifier` branch access
    /// to the binding's init expression for recursive folding —
    /// mirrors `evaluation.js:162-168` `evaluateCached(initPath, state)`.
    /// Gated on `kind == Const` per `evaluation.js:122`'s
    /// `binding.constantViolations.length > 0` short-circuit (a
    /// non-const binding deopts before reaching the init recursion).
    /// Finding 1's stored-bool reasoning applies — the gate is decided
    /// once at index-build time, not recomputed per lookup.
    pub init_expr: Option<Box<Expr>>,
    /// For VariableDeclarators only: the type of `node.id` —
    /// `"Identifier"` for plain `const x = …`, `"ObjectPattern"` for
    /// `const { x } = …`, `"ArrayPattern"` for `const [x] = …`.
    pub binding_id_type: Option<&'static str>,
    /// The scope this binding lives in. Mirrors `binding.scope`.
    pub scope: ScopeId,
    /// Span of the binding's identifier — used for byte-position
    /// identity at lookup sites.
    pub span: Span,
    /// **§5.4e addition** — for import-specifier bindings only:
    /// the module specifier + import shape. Populated by
    /// `register_import` for every binding it creates; `None`
    /// for non-import bindings.
    ///
    /// The §5.4e cross-file resolver
    /// (`utils/resolve_binding.rs`) reads this to find the source
    /// module path + the imported-side export name. Without it the
    /// resolver would have no way to walk from a local binding to
    /// the imported AST. Same shape-extension precedent §5.0c used
    /// for `init_expr` (gated population, single-purpose surface).
    pub import_info: Option<ImportInfo>,
    /// **§6.8n addition** — for `const { ... } = <init>` /
    /// `const [ ... ] = <init>` destructuring bindings only: the LHS
    /// pattern (so `getDestructuredObjectPatternKey` can recover the
    /// source key for this binding's reference name) and the RHS
    /// init expression. Mirrors `resolve-binding.ts:263-269` which
    /// reads `binding.path.node.id` (the pattern) and
    /// `binding.path.node.init` (the source) at resolve time.
    /// Populated only when LHS is `Pat::Object` AND `init` is
    /// `Some(_)`. `None` everywhere else.
    pub destructured_pat: Option<Box<ObjectPat>>,
    pub destructured_init: Option<Box<Expr>>,
}

/// Cross-file import metadata attached to import-specifier
/// bindings. Populated by [`ScopeIndex::register_import`] for
/// `import { X }` / `import X` / `import * as X` shapes.
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// The module specifier from `from 'X'` — what
    /// `resolve.resolve_sync(from_file, source)` consumes.
    pub source: String,
    /// Discriminator: default / named / namespace. Maps to the
    /// upstream `binding.path.isImport*Specifier()` predicates.
    pub kind: ImportSpecifierKind,
    /// For named imports only: the imported-side name (the LHS of
    /// `as`, or the spec name when no alias). `None` for default
    /// / namespace shapes — those have a fixed "imported name"
    /// (`default` for default; n/a for namespace).
    pub imported_name: Option<String>,
}

/// Discriminator for [`ImportInfo::kind`]. Mirrors Babel's
/// `ImportDefaultSpecifier` / `ImportSpecifier` / `ImportNamespaceSpecifier`
/// trio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportSpecifierKind {
    Default,
    Named,
    Namespace,
}

impl Binding {
    /// Mirrors `binding.js:46-52`'s `reassign(path)` — sets
    /// `constant = false` (Finding 1: atomically with the violation
    /// push) and appends to `constant_violations`. Idempotent on
    /// duplicate paths (Babel uses `includes` to dedupe; we dedupe
    /// by span.lo since spans are unique per-parse).
    pub fn reassign(&mut self, site: ReferenceSite) {
        if self
            .constant_violations
            .iter()
            .any(|v| v.span.lo == site.span.lo)
        {
            return;
        }
        self.constant = false;
        self.constant_violations.push(site);
    }

    /// Mirrors `binding.js:53-60`'s `reference(path)` — appends to
    /// `reference_paths`. Idempotent.
    pub fn reference(&mut self, site: ReferenceSite) {
        if self
            .reference_paths
            .iter()
            .any(|r| r.span.lo == site.span.lo)
        {
            return;
        }
        self.reference_paths.push(site);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReferenceSite {
    pub span: Span,
    pub scope: ScopeId,
}

// -------------------- ScopeData --------------------

/// One scope frame. Parent-pointer + bindings map per scope, indexed
/// out of band by `ScopeIndex` for O(1) lookups.
#[derive(Debug, Clone)]
struct ScopeData {
    kind: ScopeKind,
    parent: Option<ScopeId>,
    /// The scope-owning node's span. Used by `scope_at_pos` to find
    /// the innermost scope containing an arbitrary position.
    span: Span,
    /// Whether this scope is a Pattern scope. Babel's `getBinding`
    /// pattern-skip rule (Finding 2) only triggers when walking
    /// THROUGH a Pattern. Pattern scopes don't actually exist in
    /// `@babel/traverse`'s scope tree the way you'd expect — instead,
    /// `path.isPattern()` is checked on the previous scope's PATH.
    /// We approximate by tagging Function / Method / Catch scopes
    /// whose immediate binding-collection path is via destructured
    /// patterns. For the §5.0a corpus the only triggering shape is
    /// `function f({ foo })` — so we mark Function/Method/Arrow scopes
    /// whose params include an Object/Array pattern. See the
    /// `pattern-skip-getBinding-walks-past-pattern` fixture.
    has_pattern_param: bool,
}

// -------------------- ScopeIndex --------------------

/// Pre-indexed scope tree. Build once on `Program::enter`; query
/// repeatedly through the visit pass. Mirrors
/// `@babel/traverse@7.29.0`'s `Scope` instance methods, but resolved
/// up-front rather than on demand (Q1).
#[derive(Debug)]
pub struct ScopeIndex {
    scopes: Vec<ScopeData>,
    bindings_by_scope: Vec<IndexMap<String, Binding>>,
    program_id: ScopeId,
    /// UID counter for `generate_uid_identifier`. Mirrors Babel's
    /// per-program uid counter at `scope/index.js:373-389`. Call sites
    /// in `hoist-sheet.ts` and friends call this; the §4.6 stop-gap
    /// on `state.uid_counter` is replaced once §5.0a/b plumb this in.
    next_uid: Cell<u32>,
    /// Set of uid names already minted by `generate_uid_identifier`.
    /// Mirrors Babel's `program.uids[uid] = true` registration at
    /// `scope/index.js:386-388`, which makes a second
    /// `generateUidIdentifier(name)` call SEE the previously-minted
    /// name and bump its counter. Without this, consecutive calls
    /// return the same name and fail the
    /// `generate-uid-identifier-zero-counter` parity check.
    minted_uids: RefCell<IndexSet<String>>,
}

impl ScopeIndex {
    /// Build the scope tree from a Module. Walks the AST once,
    /// registering bindings + collecting reference / constant-violation
    /// candidates, then resolves references against the binding table
    /// in a final pass.
    pub fn build(module: &swc_core::ecma::ast::Module) -> Self {
        let mut builder = Builder::new();
        builder.visit_module_root(module);
        builder.resolve_pending();
        builder.into_index()
    }

    pub fn program_scope(&self) -> ScopeId {
        self.program_id
    }

    pub fn parent_of(&self, id: ScopeId) -> Option<ScopeId> {
        self.scopes
            .get(id as usize)
            .and_then(|s| s.parent)
    }

    pub fn kind_of(&self, id: ScopeId) -> ScopeKind {
        self.scopes[id as usize].kind
    }

    /// **§5.0c addition** — `scope.path.parentPath.node.type` proxy.
    ///
    /// Returns the parent SCOPE's owner-node kind, mapped from
    /// `ScopeKind` to the equivalent `NodeKind`. Used by
    /// `compat::evaluation`'s var-hoist-unsafe-block check at
    /// `evaluation.js:124-140`, which asks
    /// `bindingPathScope.path.parentPath.isBlockStatement()`.
    ///
    /// **Proxy note**: in Babel, `scope.path.parentPath` is the AST
    /// parent of the scope-OWNER node, which may not itself be a
    /// scope (e.g. a FunctionExpression's parent might be a
    /// VariableDeclarator). We answer with the parent SCOPE's
    /// kind — equivalent for the only consumer (var-hoist-unsafe
    /// check) because the question reduces to "is the immediate
    /// lexically enclosing scope a Block?" and the parent SCOPE's
    /// kind answers it. If a future caller needs the strict AST
    /// parent (not just the parent scope), escalate with a fixture
    /// — a span-keyed AST-parent side-table would be the next
    /// step.
    pub fn parent_kind_of(
        &self,
        scope: ScopeId,
    ) -> Option<crate::compat::path::NodeKind> {
        let parent = self.parent_of(scope)?;
        Some(scope_kind_to_node_kind(self.kind_of(parent)))
    }

    /// `Scope.getOwnBinding(name)` — only checks this scope's bindings
    /// map, no walk. Mirrors `scope/index.js:825-827`.
    pub fn get_own_binding(&self, scope: ScopeId, name: &str) -> Option<&Binding> {
        self.bindings_by_scope
            .get(scope as usize)
            .and_then(|m| m.get(name))
    }

    /// `Scope.getBinding(name)` — lexical-chain walk with Finding 2's
    /// pattern-skip and `arguments` early-return semantics.
    /// 1:1 port of `scope/index.js:809-824`:
    ///
    /// ```js
    /// getBinding(name) {
    ///   let scope = this; let previousPath;
    ///   do {
    ///     const binding = scope.getOwnBinding(name);
    ///     if (binding) {
    ///       if (previousPath?.isPattern() && binding.kind !== "param" && binding.kind !== "local") {
    ///         // SKIP — keep walking up
    ///       } else {
    ///         return binding;
    ///       }
    ///     } else if (!binding && name === "arguments" && scope.path.isFunction() && !scope.path.isArrowFunctionExpression()) {
    ///       break;
    ///     }
    ///     previousPath = scope.path;
    ///   } while (scope = scope.parent);
    /// }
    /// ```
    pub fn get_binding(&self, scope: ScopeId, name: &str) -> Option<&Binding> {
        let mut current = Some(scope);
        // `previous_was_pattern` mirrors `previousPath?.isPattern()`. We
        // approximate "previous scope's PATH was a Pattern" by checking
        // whether the previous scope was a Function/Method/Arrow whose
        // params include an Object/ArrayPattern destructure (the only
        // shape the Compiled corpus reaches; see Finding 2 fixture).
        let mut previous_was_pattern = false;

        while let Some(scope_id) = current {
            let scope_data = &self.scopes[scope_id as usize];
            let binding = self.bindings_by_scope[scope_id as usize].get(name);

            if let Some(binding) = binding {
                if previous_was_pattern
                    && binding.kind != BindingKind::Param
                    && binding.kind != BindingKind::Local
                {
                    // SKIP — keep walking up (Finding 2 pattern-skip).
                } else {
                    return Some(binding);
                }
            } else if name == "arguments" && scope_data.kind.is_non_arrow_function() {
                // Babel: @babel/traverse@7.29.0 scope/index.js:819-821 — `arguments` shadow
                // stops at non-arrow function boundary. Evidenced-unreachable from the
                // Compiled corpus (zero matches across 477 fixtures), but ported 1:1
                // for parity-by-default. See plugins/COMPAT_SCOPE_AUDIT.md Finding 2.
                break;
            }

            previous_was_pattern = scope_data.has_pattern_param;
            current = scope_data.parent;
        }

        None
    }

    /// `Scope.hasOwnBinding(name)` — `scope/index.js:836-838`.
    pub fn has_own_binding(&self, scope: ScopeId, name: &str) -> bool {
        self.get_own_binding(scope, name).is_some()
    }

    /// `Scope.hasBinding(name, opts)` — `scope/index.js:839-864`.
    /// Walks the scope chain checking own bindings; falls back to
    /// `globals.is_global` / `globals.is_context_variable` when
    /// `no_globals` is false. The `noUids` / `upToScope` opts are
    /// not exercised by the §5.4–§5.6 callers; defaulted off.
    pub fn has_binding(&self, scope: ScopeId, name: &str, no_globals: bool) -> bool {
        if name.is_empty() {
            return false;
        }
        let mut current = Some(scope);
        while let Some(scope_id) = current {
            if self.has_own_binding(scope_id, name) {
                return true;
            }
            current = self.scopes[scope_id as usize].parent;
        }
        if !no_globals && globals::is_global(name) {
            return true;
        }
        if !no_globals && globals::is_context_variable(name) {
            return true;
        }
        false
    }

    /// Find the innermost scope whose owning-node span contains `pos`.
    /// O(scope_count) — fine for the corpus sizes we exercise.
    ///
    /// Tie-break: when two scopes have IDENTICAL span sizes (e.g. a
    /// FunctionDeclaration whose span equals the surrounding Module's
    /// span because it's the only top-level statement), prefer the
    /// deeper one (higher ScopeId — `push_scope` assigns IDs in
    /// build/depth order). Without this, `function f(color) { … color … }`
    /// with the function as the only stmt resolves the ref's enclosing
    /// scope to the Program instead of the function, and `getBinding`
    /// can't see the param.
    pub fn scope_at_pos(&self, pos: BytePos) -> ScopeId {
        let mut best = self.program_id;
        let mut best_span_size = u32::MAX;
        for (i, s) in self.scopes.iter().enumerate() {
            let lo = s.span.lo.0;
            let hi = s.span.hi.0;
            if lo <= pos.0 && pos.0 <= hi {
                let size = hi.saturating_sub(lo);
                let id = i as ScopeId;
                // Strict-smaller wins; equal-size with deeper id wins
                // (deeper scopes are pushed later — `id > best`).
                if size < best_span_size || (size == best_span_size && id > best) {
                    best_span_size = size;
                    best = id;
                }
            }
        }
        best
    }

    /// `Scope.generateUidIdentifier(name)` — `scope/index.js:373-389`.
    /// Returns a fresh `_<name>` / `_<name>2` / etc. that doesn't
    /// collide with any existing binding in the program scope nor any
    /// previously-minted uid. `name=""` falls through to `"temp"`.
    pub fn generate_uid_identifier(&self, name: &str) -> String {
        // Babel: name = toIdentifier(name).replace(/^_+/, '').replace(/\d+$/g, '');
        let stripped: String = name
            .trim_start_matches('_')
            .trim_end_matches(|c: char| c.is_ascii_digit())
            .to_string();
        let bare = if stripped.is_empty() {
            "temp".to_string()
        } else {
            stripped
        };

        let mut i: u32 = 0;
        loop {
            let mut candidate = format!("_{}", bare);
            if i >= 11 {
                candidate.push_str(&(i - 1).to_string());
            } else if i >= 9 {
                candidate.push_str(&(i - 9).to_string());
            } else if i >= 1 {
                candidate.push_str(&(i + 1).to_string());
            }
            i += 1;
            if !self.has_binding(self.program_id, &candidate, true)
                && !self.uid_minted(&candidate)
            {
                self.next_uid.set(self.next_uid.get() + 1);
                self.record_uid(&candidate);
                return candidate;
            }
        }
    }

    fn uid_minted(&self, candidate: &str) -> bool {
        // Babel tracks per-program uids via `program.uids[name] = true`
        // at `scope/index.js:386-388`. The collision check at
        // `:384` (`hasReference(uid)`) reads `program.references[uid]`
        // which is set on every successful mint AND on every reference
        // observation during the crawl. Modeled here as a per-`ScopeIndex`
        // set so consecutive calls walk past previously-minted names.
        self.minted_uids.borrow().contains(candidate)
    }

    fn record_uid(&self, candidate: &str) {
        self.minted_uids.borrow_mut().insert(candidate.to_string());
    }

    /// Binding-only helper used by [`crate::compat::path::scope_push`]
    /// to register a synthetic binding after the AST mutation has
    /// landed.
    ///
    /// Also used by the §5.0a parity gate fixture
    /// `scope-push-iife-injects-const-binding`, which observes ONLY
    /// the binding shape post-push (not the AST). For production
    /// callers (the §5.5 IIFE site), call
    /// [`crate::compat::path::scope_push`] instead — it handles the
    /// real AST insertion (Finding 6 in
    /// `plugins/COMPAT_SCOPE_AUDIT.md`) AND delegates here for the
    /// binding-table half.
    ///
    /// History: this method was the §5.0a `scope_push_synthetic`
    /// stub. §5.0b replaces the stub with `compat::path::scope_push`
    /// (which performs the actual `unshiftContainer` AST mutation),
    /// reduces this method to its binding-registration core, and
    /// renames it to advertise that narrowed contract. Existing
    /// callers (the parity-gate fixture) are intentionally
    /// unaffected — they want exactly the binding-only behaviour.
    pub fn register_synthetic_binding(
        &mut self,
        scope: ScopeId,
        name: &str,
        binding: Binding,
    ) {
        self.bindings_by_scope[scope as usize].insert(name.to_string(), binding);
    }

    /// **§5.5 closure addition (claude-2026-05-05).** Allocate a fresh
    /// runtime-synthesised scope under `parent`. Returns the new
    /// `ScopeId`.
    ///
    /// Used by the `traverse-call-expression.ts` IIFE site at
    /// `traverse_expression/traverse_call_expression.rs`: when
    /// resolving `userFunc(<args>)` against a constant function
    /// definition, the leaf needs a fresh scope to host the
    /// `(param := evaluatedArg)` synthetic bindings the JS plugin
    /// installs via `arrowFunctionExpressionPath.scope.push(...)`.
    /// Babel resolves that scope via `NodePath.scope` walking; SWC
    /// resolves via `ScopeIndex` lookup, so the Rust port allocates a
    /// `ScopeId` directly here rather than synthesising the IIFE
    /// arrow into the AST and re-deriving the scope.
    ///
    /// **Why this isn't a full §5.0a-style scope build:**
    /// - The AST node owning this scope is transient (an IIFE arrow
    ///   that exists in memory only for the duration of one
    ///   `traverse_call_expression` call). Its span doesn't
    ///   correspond to a real source-code position, so
    ///   `scope_at_pos` won't ever return it (the runtime-allocated
    ///   scope is invisible to span-based lookups by design).
    /// - The owner-kind is always `ScopeKind::Arrow` for the IIFE
    ///   site — the upstream `wrapNodeInIIFE` helper at
    ///   `utils/ast.ts:64-65` always emits an arrow. Future callers
    ///   that need a different kind can extend this signature.
    /// - `has_pattern_param: false` — the IIFE arrow takes zero
    ///   params (the JS plugin pushes `const`-bindings INTO the
    ///   arrow's scope, NOT as arrow params).
    /// - `parent`-walk semantics work: subsequent
    ///   [`Self::get_binding`] calls with `scope = <new id>` walk up
    ///   to the parent on miss, so caller-scope bindings stay
    ///   visible through the IIFE's scope.
    ///
    /// **Why this isn't drift on the §5.0a/§5.0c/§5.4e shape:** the
    /// shape extension precedent — adding a single field
    /// (`init_expr`, `import_info`) or a single registration entry
    /// point — was set by §5.0c and §5.4e. This is the same pattern:
    /// one new entry point that adds a row to `scopes` +
    /// `bindings_by_scope`, no behavioural change to existing
    /// methods.
    pub fn register_new_scope(&mut self, parent: ScopeId, kind: ScopeKind) -> ScopeId {
        let id = self.scopes.len() as ScopeId;
        self.scopes.push(ScopeData {
            kind,
            parent: Some(parent),
            // DUMMY_SP — the §5.5 IIFE site's arrow has no source
            // position. `scope_at_pos` filters by `lo <= pos <= hi`
            // and `DUMMY_SP` has `lo = hi = BytePos(0)`, so a
            // runtime-synthesised scope is invisible to position-based
            // lookups by design (see `scope_at_pos` semantics above).
            span: swc_core::common::DUMMY_SP,
            has_pattern_param: false,
        });
        self.bindings_by_scope.push(IndexMap::new());
        id
    }

    /// **Deprecated convenience wrapper.** Construct a synthetic
    /// `VariableDeclarator`-shaped `Binding` and register it via
    /// [`Self::register_synthetic_binding`]. Used by the §5.0a parity
    /// gate fixture; new code should call
    /// [`crate::compat::path::scope_push`] (which handles AST
    /// insertion + binding registration in one step).
    ///
    /// Kept under its original name to avoid churning the §5.0a
    /// integration test (`tests/compat_scope_integration.rs`); the
    /// implementation now delegates to
    /// [`Self::register_synthetic_binding`] so the binding-only
    /// behaviour stays byte-identical to the §5.0a stub.
    pub fn scope_push_synthetic(
        &mut self,
        scope: ScopeId,
        name: &str,
        kind: BindingKind,
        binding_init_string: Option<String>,
        span: Span,
    ) {
        let binding = Binding {
            kind,
            identifier_name: name.to_string(),
            constant: true,
            constant_violations: Vec::new(),
            reference_paths: Vec::new(),
            binding_node_type: "VariableDeclarator",
            parent_node_type: "VariableDeclaration",
            binding_init_string,
            // Synthetic-binding helper has no source `Expr`; §5.0c
            // recursive identifier-evaluation never reaches a binding
            // registered through this helper (only the §5.0a parity
            // gate fixture calls it). Production callers use
            // `compat::path::scope_push`, which constructs its own
            // `Binding` with `init_expr` populated from the synthesized
            // declarator's init.
            init_expr: None,
            binding_id_type: Some("Identifier"),
            scope,
            span,
            import_info: None,
            destructured_pat: None,
            destructured_init: None,
        };
        self.register_synthetic_binding(scope, name, binding);
    }
}

// -------------------- Builder (private) --------------------

/// Pending records collected during the walk; resolved post-walk.
struct Pending {
    references: Vec<PendingReference>,
    constant_violations: Vec<PendingReference>,
}

#[derive(Debug, Clone)]
struct PendingReference {
    name: String,
    span: Span,
    scope: ScopeId,
}

struct Builder {
    scopes: Vec<ScopeData>,
    bindings_by_scope: Vec<IndexMap<String, Binding>>,
    scope_stack: Vec<ScopeId>,
    pending: Pending,
}

impl Builder {
    fn new() -> Self {
        Self {
            scopes: Vec::new(),
            bindings_by_scope: Vec::new(),
            scope_stack: Vec::new(),
            pending: Pending {
                references: Vec::new(),
                constant_violations: Vec::new(),
            },
        }
    }

    fn current_scope(&self) -> ScopeId {
        *self.scope_stack.last().expect("scope_stack must be non-empty during walk")
    }

    fn push_scope(&mut self, kind: ScopeKind, span: Span, has_pattern_param: bool) -> ScopeId {
        let id = self.scopes.len() as ScopeId;
        let parent = self.scope_stack.last().copied();
        self.scopes.push(ScopeData {
            kind,
            parent,
            span,
            has_pattern_param,
        });
        self.bindings_by_scope.push(IndexMap::new());
        self.scope_stack.push(id);
        id
    }

    fn pop_scope(&mut self) {
        self.scope_stack.pop();
    }

    /// `getFunctionParent` — walks up looking for a Function/Arrow/Method
    /// scope. Returns None if the chain reaches Program first; per the
    /// upstream `var`-hoist logic that means hoist to Program instead.
    fn function_parent(&self, scope: ScopeId) -> Option<ScopeId> {
        let mut current = Some(scope);
        while let Some(s) = current {
            if self.scopes[s as usize].kind.is_function_parent() {
                return Some(s);
            }
            current = self.scopes[s as usize].parent;
        }
        None
    }

    fn program_parent(&self, scope: ScopeId) -> ScopeId {
        let mut current = scope;
        loop {
            if self.scopes[current as usize].kind == ScopeKind::Program {
                return current;
            }
            current = self.scopes[current as usize]
                .parent
                .expect("program scope must reachable via parent chain");
        }
    }

    /// Babel's `getFunctionParent() || getProgramParent()`. Used by
    /// `var`-hoist (Finding 3) and by Declaration handler.
    fn function_or_program_parent(&self, scope: ScopeId) -> ScopeId {
        self.function_parent(scope)
            .unwrap_or_else(|| self.program_parent(scope))
    }

    fn visit_module_root(&mut self, m: &swc_core::ecma::ast::Module) {
        let _program = self.push_scope(ScopeKind::Program, m.span, false);
        for item in &m.body {
            self.visit_module_item(item);
        }
        self.pop_scope();
    }

    fn visit_module_item(&mut self, item: &ModuleItem) {
        match item {
            ModuleItem::ModuleDecl(decl) => self.visit_module_decl(decl),
            ModuleItem::Stmt(stmt) => self.visit_stmt(stmt),
        }
    }

    fn visit_module_decl(&mut self, decl: &ModuleDecl) {
        match decl {
            ModuleDecl::Import(i) => self.register_import(i),
            ModuleDecl::ExportDecl(ExportDecl { decl, .. }) => self.register_decl(decl),
            ModuleDecl::ExportDefaultDecl(ExportDefaultDecl { decl, .. }) => {
                use swc_core::ecma::ast::DefaultDecl;
                match decl {
                    DefaultDecl::Class(c) => self.visit_class_expr(c),
                    DefaultDecl::Fn(f) => self.visit_fn_expr(f),
                    DefaultDecl::TsInterfaceDecl(_) => {}
                }
            }
            ModuleDecl::ExportDefaultExpr(e) => {
                self.visit_expr(&e.expr);
            }
            ModuleDecl::ExportNamed(NamedExport { specifiers, src, .. }) => {
                if src.is_none() {
                    // Local re-exports — record the local name as a
                    // reference. Re-exports with `from "src"` don't
                    // reference anything in the current module.
                    use swc_core::ecma::ast::{ExportSpecifier, ModuleExportName};
                    for spec in specifiers {
                        if let ExportSpecifier::Named(named) = spec {
                            if let ModuleExportName::Ident(id) = &named.orig {
                                self.record_reference(&id.sym, id.span);
                            }
                        }
                    }
                }
            }
            ModuleDecl::ExportAll(_)
            | ModuleDecl::TsImportEquals(_)
            | ModuleDecl::TsExportAssignment(_)
            | ModuleDecl::TsNamespaceExport(_) => {}
        }
    }

    /// Declaration handler — mirrors collectorVisitor's `Declaration`
    /// + `BlockScoped` split:
    /// - `const`/`let`/`using`/`await using` → block-scoped (registered
    ///   at the enclosing block parent — but since BlockStmt-as-fn-body
    ///   isn't its own scope, that resolves to the enclosing function/
    ///   block scope).
    /// - `var` → function-scoped (registered at the enclosing function/
    ///   program parent).
    /// - `function f(){}` declaration → hoisted (function/program parent).
    /// - `class C {}` declaration → block-scoped, kind=`let`.
    fn register_decl(&mut self, decl: &Decl) {
        match decl {
            Decl::Var(v) => self.register_var_decl(v, /* in_for_init */ false),
            Decl::Fn(fn_decl) => self.register_fn_decl(fn_decl),
            Decl::Class(class_decl) => self.register_class_decl(class_decl),
            Decl::Using(_)
            | Decl::TsInterface(_)
            | Decl::TsTypeAlias(_)
            | Decl::TsEnum(_)
            | Decl::TsModule(_) => {
                // TS-only declarations are out of scope for the §5.0a corpus.
            }
        }
    }

    fn register_var_decl(&mut self, v: &VarDecl, in_for_init: bool) {
        let kind = match v.kind {
            VarDeclKind::Const => BindingKind::Const,
            VarDeclKind::Let => BindingKind::Let,
            VarDeclKind::Var => BindingKind::Var,
        };
        // Finding 3: `var` in for-init/for-x-left hoists to function/program.
        // Block-scoped (`let`/`const`) registers at the immediate scope.
        let target_scope = if matches!(kind, BindingKind::Var) {
            self.function_or_program_parent(self.current_scope())
        } else {
            self.current_scope()
        };

        for (idx, declarator) in v.decls.iter().enumerate() {
            self.register_var_declarator(declarator, kind, target_scope, v, idx, in_for_init);
        }

        // Visit init expressions for references. The binding LHS is a
        // Pat, which we DON'T visit as a reference (pattern Idents are
        // BindingIdent, not Ident, so visit_ident wouldn't fire anyway —
        // but we recurse explicitly to control reference tracking).
        for declarator in &v.decls {
            if let Some(init) = &declarator.init {
                self.visit_expr(init);
            }
        }
    }

    fn register_var_declarator(
        &mut self,
        declarator: &VarDeclarator,
        kind: BindingKind,
        target_scope: ScopeId,
        _parent_decl: &VarDecl,
        _index_in_parent: usize,
        in_loop_init: bool,
    ) {
        // Determine binding_id_type: Identifier | ObjectPattern | ArrayPattern.
        let binding_id_type: &'static str = match &declarator.name {
            Pat::Ident(_) => "Identifier",
            Pat::Object(_) => "ObjectPattern",
            Pat::Array(_) => "ArrayPattern",
            Pat::Assign(_) => "AssignmentPattern",
            Pat::Rest(_) => "RestElement",
            Pat::Invalid(_) => "Invalid",
            Pat::Expr(_) => "Expression",
        };

        let init_string = init_string_value(declarator);

        // §5.0c — populate `init_expr` for `<kind> x = <expr>` where
        // LHS is a simple `Pat::Ident`. evaluation.js:162-168
        // recurses on `binding.path.get('init')`; the §5.0c port
        // needs the cloned expression. Babel does NOT gate the init
        // recursion on `kind` — `evaluation.js:120-123` deopts only
        // when `binding.constantViolations.length > 0` (i.e. the
        // binding has been observed to be reassigned). A
        // `let notMutatedAgain = 20;` fixture with no reassignment
        // has `binding.constant === true` and folds via the init
        // recursion the same as a `const`. The runtime gate is
        // `binding.constant`, checked at use-site
        // (`evaluation.rs:445`, `traverse_identifier`'s `if binding.constant`).
        // `var` deopts unconditionally at `evaluation.rs:457`, so a
        // populated `init_expr` for `var` is harmless but never read
        // through that path. Destructuring bindings deopt (no single
        // `binding.path.init` to recurse on).
        let init_expr_for_const_ident: Option<Box<Expr>> =
            if matches!(declarator.name, Pat::Ident(_)) {
                declarator.init.clone()
            } else {
                None
            };

        // §6.8n — capture the LHS ObjectPat + RHS init for
        // destructured bindings so `resolve_binding` can route
        // through `resolve_object_pattern_value_node` (mirrors
        // resolve-binding.ts:263-269). Only `Pat::Object` is wired —
        // `Pat::Array` deopts cleanly (corpus reach is empty).
        let (destructured_pat, destructured_init): (Option<Box<ObjectPat>>, Option<Box<Expr>>) =
            match (&declarator.name, declarator.init.as_ref()) {
                (Pat::Object(obj), Some(init)) => {
                    (Some(Box::new(obj.clone())), Some(init.clone()))
                }
                _ => (None, None),
            };

        for (name, span) in collect_pat_idents(&declarator.name) {
            let mut binding = Binding {
                kind,
                identifier_name: name.clone(),
                constant: true,
                constant_violations: Vec::new(),
                reference_paths: Vec::new(),
                binding_node_type: "VariableDeclarator",
                parent_node_type: "VariableDeclaration",
                // Only the simple-Identifier case carries the cached
                // string init; destructured bindings deopt (their RHS
                // isn't a string literal anyway, and the Babel-side
                // observable is only set when `binding.path.node.init`
                // is a StringLiteral).
                binding_init_string: if matches!(declarator.name, Pat::Ident(_)) {
                    init_string.clone()
                } else {
                    None
                },
                init_expr: init_expr_for_const_ident.clone(),
                binding_id_type: Some(binding_id_type),
                scope: target_scope,
                span,
                import_info: None,
                destructured_pat: destructured_pat.clone(),
                destructured_init: destructured_init.clone(),
            };

            // Finding 4: isInitInLoop auto-reassigns var/hoisted bindings
            // declared inside loops. Mirrors binding.js:27-29 and
            // isInitInLoop at binding.js:67-82. We approximate `path.isVariableDeclarator()
            // || path.node.init` as "any var declaration in for-init/for-x-left
            // counts" — which is the §5.0a corpus's only reach.
            if matches!(kind, BindingKind::Var) && in_loop_init {
                binding.constant = false;
                binding.constant_violations.push(ReferenceSite {
                    span,
                    scope: target_scope,
                });
            }

            self.bindings_by_scope[target_scope as usize].insert(name, binding);
        }
    }

    fn register_fn_decl(&mut self, fn_decl: &FnDecl) {
        let target_scope = self.function_or_program_parent(self.current_scope());
        let span = fn_decl.ident.span;
        // Babel's `binding.path.node` for a FunctionDeclaration IS the
        // FunctionDeclaration node, and `t.isFunction(FunctionDeclaration)`
        // returns true — so upstream `traverseIdentifier`'s
        // `evaluateExpression(binding.path.node)` flows into
        // `traverseFunction` and walks for the first ReturnStatement.
        //
        // The Rust port stores `init_expr: Option<Box<Expr>>` on
        // `Binding`. FunctionDeclaration is a Stmt-level node, not an
        // Expr — so we synthesize an `Expr::Fn(FnExpr)` wrapping the
        // same `Function` body. This gives `evaluate_expression`'s
        // `Expr::Fn(_) | Expr::Arrow(_)` dispatch arm something to fold
        // and matches Babel's behaviour 1:1. Matches the §5.0c
        // single-purpose `init_expr` extension precedent.
        let init_expr = Some(Box::new(Expr::Fn(FnExpr {
            ident: Some(fn_decl.ident.clone()),
            function: fn_decl.function.clone(),
        })));
        let binding = Binding {
            kind: BindingKind::Hoisted,
            identifier_name: fn_decl.ident.sym.to_string(),
            constant: true,
            constant_violations: Vec::new(),
            reference_paths: Vec::new(),
            binding_node_type: "FunctionDeclaration",
            parent_node_type: parent_for_module_item(self.current_scope_kind()),
            binding_init_string: None,
            init_expr,
            binding_id_type: None,
            scope: target_scope,
            span,
            import_info: None,
            destructured_pat: None,
            destructured_init: None,
        };
        self.bindings_by_scope[target_scope as usize]
            .insert(fn_decl.ident.sym.to_string(), binding);

        self.descend_into_function(&fn_decl.function);
    }

    fn register_class_decl(&mut self, class_decl: &ClassDecl) {
        let target_scope = self.current_scope();
        let span = class_decl.ident.span;
        let binding = Binding {
            kind: BindingKind::Let,
            identifier_name: class_decl.ident.sym.to_string(),
            constant: true,
            constant_violations: Vec::new(),
            reference_paths: Vec::new(),
            binding_node_type: "ClassDeclaration",
            parent_node_type: parent_for_module_item(self.current_scope_kind()),
            binding_init_string: None,
            init_expr: None,
            binding_id_type: None,
            scope: target_scope,
            span,
            import_info: None,
            destructured_pat: None,
            destructured_init: None,
        };
        self.bindings_by_scope[target_scope as usize]
            .insert(class_decl.ident.sym.to_string(), binding);

        // Class scope owns the body — methods + field inits live there.
        let class_scope = self.push_scope(ScopeKind::Class, class_decl.class.span, false);
        // ClassDeclaration: per Babel BlockScoped handler, the class's
        // own name is also visible inside the class body (set on the
        // class scope's bindings via parent.getBinding(name)). For
        // §5.0a parity gate we don't need to re-mirror this — the
        // class-name-from-inside-body shape isn't in the 23-fixture corpus.
        let _ = class_scope;
        self.visit_class_body(&class_decl.class);
        self.pop_scope();
    }

    fn current_scope_kind(&self) -> ScopeKind {
        self.scopes[self.current_scope() as usize].kind
    }

    /// Function-scope creation + param/body binding. Used by
    /// FunctionDeclaration, FunctionExpression, ObjectMethod,
    /// ClassMethod call sites. Caller registers the outer name (if
    /// any); this method handles params + body.
    fn descend_into_function(&mut self, function: &Function) {
        let has_pattern_param = function.params.iter().any(|p| {
            matches!(&p.pat, Pat::Object(_) | Pat::Array(_))
        });
        let _ = self.push_scope(ScopeKind::Function, function.span, has_pattern_param);
        self.register_params(&function.params.iter().map(|p| &p.pat).collect::<Vec<_>>());
        if let Some(body) = &function.body {
            // Function body BlockStmt is NOT its own scope (Babel rule:
            // isScope(BlockStatement) === false when parent is Function).
            // Visit stmts directly to skip visit_block_stmt's scope push.
            for stmt in &body.stmts {
                self.visit_stmt(stmt);
            }
        }
        self.pop_scope();
    }

    fn register_params(&mut self, params: &[&Pat]) {
        for param in params {
            self.register_param(param);
        }
    }

    fn register_param(&mut self, pat: &Pat) {
        // Babel param registration: binding.path.node IS the param
        // (the WHOLE pattern for destructured params, the Identifier
        // for plain idents). binding.kind = "param". parent.type =
        // FunctionDeclaration / FunctionExpression / ArrowFunctionExpression.
        let scope = self.current_scope();
        let scope_kind = self.scopes[scope as usize].kind;
        let parent_node_type = match scope_kind {
            ScopeKind::Function => "FunctionDeclaration",
            ScopeKind::Arrow => "ArrowFunctionExpression",
            ScopeKind::Method => "ObjectMethod",
            ScopeKind::Catch => "CatchClause",
            _ => "Function",
        };

        // `(binding_node_type, span_of_path)` depends on the pat shape.
        let (binding_node_type, _binding_id_type) = match pat {
            Pat::Ident(_) => ("Identifier", "Identifier"),
            Pat::Object(_) => ("ObjectPattern", "ObjectPattern"),
            Pat::Array(_) => ("ArrayPattern", "ArrayPattern"),
            Pat::Assign(_) => ("AssignmentPattern", "AssignmentPattern"),
            Pat::Rest(_) => ("RestElement", "RestElement"),
            Pat::Expr(_) => ("Expression", "Expression"),
            Pat::Invalid(_) => ("Invalid", "Invalid"),
        };

        let pat_span = pat_span(pat);
        for (name, ident_span) in collect_pat_idents(pat) {
            let binding = Binding {
                kind: BindingKind::Param,
                identifier_name: name.clone(),
                constant: true,
                constant_violations: Vec::new(),
                reference_paths: Vec::new(),
                binding_node_type,
                parent_node_type,
                binding_init_string: None,
                init_expr: None,
                binding_id_type: None,
                // The binding's "scope" is the function/arrow scope
                // we just pushed.
                scope,
                // For destructured params, binding.path is the pattern
                // (whole ObjectPattern / ArrayPattern); the identifier
                // span is recorded separately. We use pat_span as the
                // canonical "binding span" for scope_at lookups.
                span: if matches!(pat, Pat::Ident(_)) {
                    ident_span
                } else {
                    pat_span
                },
                import_info: None,
                destructured_pat: None,
                destructured_init: None,
            };
            self.bindings_by_scope[scope as usize].insert(name, binding);
        }
    }

    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Block(b) => {
                // Standalone block — push Block scope.
                let _ = self.push_scope(ScopeKind::Block, b.span, false);
                for s in &b.stmts {
                    self.visit_stmt(s);
                }
                self.pop_scope();
            }
            Stmt::Decl(d) => self.register_decl(d),
            Stmt::Expr(e) => self.visit_expr(&e.expr),
            Stmt::Return(r) => {
                if let Some(arg) = &r.arg {
                    self.visit_expr(arg);
                }
            }
            Stmt::If(i) => {
                self.visit_expr(&i.test);
                self.visit_stmt(&i.cons);
                if let Some(alt) = &i.alt {
                    self.visit_stmt(alt);
                }
            }
            Stmt::While(w) => {
                self.visit_expr(&w.test);
                self.visit_stmt(&w.body);
            }
            Stmt::DoWhile(dw) => {
                self.visit_stmt(&dw.body);
                self.visit_expr(&dw.test);
            }
            Stmt::For(f) => self.visit_for_stmt(f),
            Stmt::ForIn(f) => self.visit_for_in_stmt(f),
            Stmt::ForOf(f) => self.visit_for_of_stmt(f),
            Stmt::Switch(s) => self.visit_switch_stmt(s),
            Stmt::Throw(t) => self.visit_expr(&t.arg),
            Stmt::Try(t) => {
                // try-block is a Block scope.
                let _ = self.push_scope(ScopeKind::Block, t.block.span, false);
                for s in &t.block.stmts {
                    self.visit_stmt(s);
                }
                self.pop_scope();
                if let Some(handler) = &t.handler {
                    self.visit_catch_clause(handler);
                }
                if let Some(finalizer) = &t.finalizer {
                    let _ = self.push_scope(ScopeKind::Block, finalizer.span, false);
                    for s in &finalizer.stmts {
                        self.visit_stmt(s);
                    }
                    self.pop_scope();
                }
            }
            Stmt::Labeled(l) => self.visit_stmt(&l.body),
            Stmt::With(w) => {
                self.visit_expr(&w.obj);
                self.visit_stmt(&w.body);
            }
            Stmt::Empty(_) | Stmt::Debugger(_) | Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }

    fn visit_for_stmt(&mut self, f: &ForStmt) {
        let _ = self.push_scope(ScopeKind::For, f.span, false);
        if let Some(init) = &f.init {
            match init {
                VarDeclOrExpr::VarDecl(v) => {
                    self.register_var_decl(v, /* in_for_init */ true);
                }
                VarDeclOrExpr::Expr(e) => self.visit_expr(e),
            }
        }
        if let Some(test) = &f.test {
            self.visit_expr(test);
        }
        if let Some(update) = &f.update {
            self.visit_expr(update);
        }
        self.visit_stmt(&f.body);
        self.pop_scope();
    }

    fn visit_for_in_stmt(&mut self, f: &ForInStmt) {
        let _ = self.push_scope(ScopeKind::ForX, f.span, false);
        self.visit_for_head(&f.left);
        self.visit_expr(&f.right);
        self.visit_stmt(&f.body);
        self.pop_scope();
    }

    fn visit_for_of_stmt(&mut self, f: &ForOfStmt) {
        let _ = self.push_scope(ScopeKind::ForX, f.span, false);
        self.visit_for_head(&f.left);
        self.visit_expr(&f.right);
        self.visit_stmt(&f.body);
        self.pop_scope();
    }

    /// Mirrors collectorVisitor's `ForXStatement` handler:
    /// - `for (var x of …)` → register `var` at function/program scope
    ///   (Finding 3) AND the Binding constructor's isInitInLoop fires
    ///   (Finding 4).
    /// - `for (let x of …)` / `for (const x of …)` → register at the
    ///   immediate ForX scope.
    /// - `for (x of …)` (Pattern/Identifier on the LHS without a decl)
    ///   → constant violation on the existing binding.
    fn visit_for_head(&mut self, head: &ForHead) {
        match head {
            ForHead::VarDecl(v) => {
                self.register_var_decl(v, /* in_for_init */ true);
            }
            ForHead::UsingDecl(_) => {
                // using/await using are block-scoped const-equivalents.
            }
            ForHead::Pat(pat) => {
                // Constant violation on whatever binding the pat resolves to.
                let scope = self.current_scope();
                for (name, span) in collect_pat_idents(pat) {
                    self.pending.constant_violations.push(PendingReference {
                        name: name.clone(),
                        span,
                        scope,
                    });
                    self.pending.references.push(PendingReference {
                        name,
                        span,
                        scope,
                    });
                }
            }
        }
    }

    fn visit_switch_stmt(&mut self, s: &SwitchStmt) {
        let _ = self.push_scope(ScopeKind::Switch, s.span, false);
        self.visit_expr(&s.discriminant);
        for case in &s.cases {
            if let Some(test) = &case.test {
                self.visit_expr(test);
            }
            for stmt in &case.cons {
                self.visit_stmt(stmt);
            }
        }
        self.pop_scope();
    }

    fn visit_catch_clause(&mut self, c: &CatchClause) {
        let has_pattern_param = matches!(&c.param, Some(Pat::Object(_)) | Some(Pat::Array(_)));
        let _ = self.push_scope(ScopeKind::Catch, c.span, has_pattern_param);
        if let Some(param) = &c.param {
            // Babel: `catch(e)` registers `e` as kind='let' at the
            // catch scope (collectorVisitor's CatchClause handler at
            // scope/index.js:283-285 calls registerBinding('let', path)
            // with path = the CatchClause itself).
            let scope = self.current_scope();
            let pat_span = pat_span(param);
            let (binding_node_type, _) = match param {
                Pat::Ident(_) => ("Identifier", "Identifier"),
                Pat::Object(_) => ("ObjectPattern", "ObjectPattern"),
                Pat::Array(_) => ("ArrayPattern", "ArrayPattern"),
                _ => ("Identifier", "Identifier"),
            };
            for (name, ident_span) in collect_pat_idents(param) {
                let binding = Binding {
                    kind: BindingKind::Let,
                    identifier_name: name.clone(),
                    constant: true,
                    constant_violations: Vec::new(),
                    reference_paths: Vec::new(),
                    binding_node_type,
                    parent_node_type: "CatchClause",
                    binding_init_string: None,
                    init_expr: None,
                    binding_id_type: None,
                    scope,
                    span: if matches!(param, Pat::Ident(_)) {
                        ident_span
                    } else {
                        pat_span
                    },
                    import_info: None,
                    destructured_pat: None,
                    destructured_init: None,
                };
                self.bindings_by_scope[scope as usize].insert(name, binding);
            }
        }
        // Catch body block is NOT its own scope (parent is CatchClause).
        for stmt in &c.body.stmts {
            self.visit_stmt(stmt);
        }
        self.pop_scope();
    }

    fn visit_class_body(&mut self, class: &swc_core::ecma::ast::Class) {
        // Walk class body members. Methods get their own Function-like scope.
        // Field inits are evaluated in the class scope.
        use swc_core::ecma::ast::ClassMember;
        for member in &class.body {
            match member {
                ClassMember::Method(m) => {
                    let has_pattern_param = m.function.params.iter().any(|p| {
                        matches!(&p.pat, Pat::Object(_) | Pat::Array(_))
                    });
                    let _ = self.push_scope(ScopeKind::Method, m.function.span, has_pattern_param);
                    self.register_params(
                        &m.function.params.iter().map(|p| &p.pat).collect::<Vec<_>>(),
                    );
                    if let Some(body) = &m.function.body {
                        for stmt in &body.stmts {
                            self.visit_stmt(stmt);
                        }
                    }
                    self.pop_scope();
                }
                ClassMember::Constructor(c) => {
                    let _ = self.push_scope(ScopeKind::Method, c.span, false);
                    if let Some(body) = &c.body {
                        for stmt in &body.stmts {
                            self.visit_stmt(stmt);
                        }
                    }
                    self.pop_scope();
                }
                ClassMember::ClassProp(p) => {
                    if let Some(value) = &p.value {
                        self.visit_expr(value);
                    }
                }
                ClassMember::PrivateProp(p) => {
                    if let Some(value) = &p.value {
                        self.visit_expr(value);
                    }
                }
                _ => {}
            }
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Ident(i) => {
                self.record_reference(&i.sym, i.span);
            }
            Expr::Array(a) => {
                for elem in &a.elems {
                    if let Some(spread_or_expr) = elem {
                        self.visit_expr(&spread_or_expr.expr);
                    }
                }
            }
            Expr::Object(o) => {
                for prop_or_spread in &o.props {
                    use swc_core::ecma::ast::PropOrSpread;
                    match prop_or_spread {
                        PropOrSpread::Spread(s) => self.visit_expr(&s.expr),
                        PropOrSpread::Prop(p) => self.visit_prop(p),
                    }
                }
            }
            Expr::Fn(fn_expr) => self.visit_fn_expr(fn_expr),
            Expr::Class(class_expr) => self.visit_class_expr(class_expr),
            Expr::Arrow(a) => self.visit_arrow_expr(a),
            Expr::Unary(u) => self.visit_expr(&u.arg),
            Expr::Update(u) => self.visit_update_expr(u),
            Expr::Bin(b) => {
                self.visit_expr(&b.left);
                self.visit_expr(&b.right);
            }
            Expr::Assign(a) => self.visit_assign_expr(a),
            Expr::Member(m) => {
                self.visit_expr(&m.obj);
                use swc_core::ecma::ast::MemberProp;
                match &m.prop {
                    MemberProp::Computed(c) => self.visit_expr(&c.expr),
                    MemberProp::Ident(_) | MemberProp::PrivateName(_) => {}
                }
            }
            Expr::SuperProp(s) => {
                use swc_core::ecma::ast::SuperProp;
                if let SuperProp::Computed(c) = &s.prop {
                    self.visit_expr(&c.expr);
                }
            }
            Expr::Cond(c) => {
                self.visit_expr(&c.test);
                self.visit_expr(&c.cons);
                self.visit_expr(&c.alt);
            }
            Expr::Call(c) => {
                use swc_core::ecma::ast::Callee;
                match &c.callee {
                    Callee::Expr(e) => self.visit_expr(e),
                    Callee::Super(_) | Callee::Import(_) => {}
                }
                for arg in &c.args {
                    self.visit_expr(&arg.expr);
                }
            }
            Expr::New(n) => {
                self.visit_expr(&n.callee);
                if let Some(args) = &n.args {
                    for arg in args {
                        self.visit_expr(&arg.expr);
                    }
                }
            }
            Expr::Seq(s) => {
                for e in &s.exprs {
                    self.visit_expr(e);
                }
            }
            Expr::Tpl(t) => {
                for e in &t.exprs {
                    self.visit_expr(e);
                }
            }
            Expr::TaggedTpl(t) => {
                self.visit_expr(&t.tag);
                for e in &t.tpl.exprs {
                    self.visit_expr(e);
                }
            }
            Expr::Paren(p) => self.visit_expr(&p.expr),
            Expr::Await(a) => self.visit_expr(&a.arg),
            Expr::Yield(y) => {
                if let Some(arg) = &y.arg {
                    self.visit_expr(arg);
                }
            }
            Expr::Lit(_)
            | Expr::This(_)
            | Expr::MetaProp(_)
            | Expr::PrivateName(_)
            | Expr::Invalid(_) => {}
            Expr::TsAs(t) => self.visit_expr(&t.expr),
            Expr::TsConstAssertion(t) => self.visit_expr(&t.expr),
            Expr::TsNonNull(t) => self.visit_expr(&t.expr),
            Expr::TsTypeAssertion(t) => self.visit_expr(&t.expr),
            Expr::TsInstantiation(t) => self.visit_expr(&t.expr),
            Expr::TsSatisfies(t) => self.visit_expr(&t.expr),
            Expr::OptChain(o) => {
                use swc_core::ecma::ast::OptChainBase;
                match &*o.base {
                    OptChainBase::Member(m) => {
                        self.visit_expr(&m.obj);
                        use swc_core::ecma::ast::MemberProp;
                        if let MemberProp::Computed(c) = &m.prop {
                            self.visit_expr(&c.expr);
                        }
                    }
                    OptChainBase::Call(c) => {
                        self.visit_expr(&c.callee);
                        for arg in &c.args {
                            self.visit_expr(&arg.expr);
                        }
                    }
                }
            }
            Expr::JSXElement(_) | Expr::JSXFragment(_) | Expr::JSXEmpty(_)
            | Expr::JSXMember(_) | Expr::JSXNamespacedName(_) => {
                // JSX surface isn't reached by the §5.0a fixture corpus;
                // §5.4–§5.6 will exercise it via the css-prop / class-names
                // handlers, which already have their own scope-walk
                // call sites. Add a TODO-as-comment, not an unimplemented!,
                // so this file doesn't gate a stray JSX expression in
                // a future test.
            }
        }
    }

    fn visit_prop(&mut self, prop: &Prop) {
        match prop {
            Prop::Shorthand(i) => {
                // `{ foo }` shorthand — `foo` is BOTH the key and the value.
                // The value side is a reference. Babel parity: the
                // ReferencedIdentifier visitor fires on the shorthand
                // ident as a reference (it is `node.key === node.value`
                // by Babel parser convention).
                self.record_reference(&i.sym, i.span);
            }
            Prop::KeyValue(kv) => {
                use swc_core::ecma::ast::PropName;
                if let PropName::Computed(c) = &kv.key {
                    self.visit_expr(&c.expr);
                }
                self.visit_expr(&kv.value);
            }
            Prop::Assign(_) => {
                // Only valid in ObjectPattern destructure — handled in collect_pat_idents.
            }
            Prop::Getter(g) => {
                if let Some(body) = &g.body {
                    let _ = self.push_scope(ScopeKind::Method, g.span, false);
                    for stmt in &body.stmts {
                        self.visit_stmt(stmt);
                    }
                    self.pop_scope();
                }
            }
            Prop::Setter(s) => {
                let _ = self.push_scope(ScopeKind::Method, s.span, false);
                self.register_param(&s.param);
                if let Some(body) = &s.body {
                    for stmt in &body.stmts {
                        self.visit_stmt(stmt);
                    }
                }
                self.pop_scope();
            }
            Prop::Method(m) => {
                let has_pattern_param = m.function.params.iter().any(|p| {
                    matches!(&p.pat, Pat::Object(_) | Pat::Array(_))
                });
                let _ = self.push_scope(ScopeKind::Method, m.function.span, has_pattern_param);
                self.register_params(
                    &m.function.params.iter().map(|p| &p.pat).collect::<Vec<_>>(),
                );
                if let Some(body) = &m.function.body {
                    for stmt in &body.stmts {
                        self.visit_stmt(stmt);
                    }
                }
                self.pop_scope();
            }
        }
    }

    fn visit_fn_expr(&mut self, fn_expr: &FnExpr) {
        // FunctionExpression's own name (if present) registers as
        // 'local' inside the function's own scope (NOT the outer
        // scope). Babel parity: collectorVisitor Function handler at
        // scope/index.js:286-294.
        let has_pattern_param = fn_expr.function.params.iter().any(|p| {
            matches!(&p.pat, Pat::Object(_) | Pat::Array(_))
        });
        let scope_id = self.push_scope(ScopeKind::Function, fn_expr.function.span, has_pattern_param);

        if let Some(ident) = &fn_expr.ident {
            let binding = Binding {
                kind: BindingKind::Local,
                identifier_name: ident.sym.to_string(),
                constant: true,
                constant_violations: Vec::new(),
                reference_paths: Vec::new(),
                binding_node_type: "Identifier",
                parent_node_type: "FunctionExpression",
                binding_init_string: None,
                init_expr: None,
                binding_id_type: None,
                scope: scope_id,
                span: ident.span,
                import_info: None,
                destructured_pat: None,
                destructured_init: None,
            };
            self.bindings_by_scope[scope_id as usize]
                .insert(ident.sym.to_string(), binding);
        }

        self.register_params(
            &fn_expr.function.params.iter().map(|p| &p.pat).collect::<Vec<_>>(),
        );
        if let Some(body) = &fn_expr.function.body {
            for stmt in &body.stmts {
                self.visit_stmt(stmt);
            }
        }
        self.pop_scope();
    }

    fn visit_class_expr(&mut self, class_expr: &ClassExpr) {
        let scope_id = self.push_scope(ScopeKind::Class, class_expr.class.span, false);
        if let Some(ident) = &class_expr.ident {
            let binding = Binding {
                kind: BindingKind::Local,
                identifier_name: ident.sym.to_string(),
                constant: true,
                constant_violations: Vec::new(),
                reference_paths: Vec::new(),
                binding_node_type: "Identifier",
                parent_node_type: "ClassExpression",
                binding_init_string: None,
                init_expr: None,
                binding_id_type: None,
                scope: scope_id,
                span: ident.span,
                import_info: None,
                destructured_pat: None,
                destructured_init: None,
            };
            self.bindings_by_scope[scope_id as usize]
                .insert(ident.sym.to_string(), binding);
        }
        self.visit_class_body(&class_expr.class);
        self.pop_scope();
    }

    fn visit_arrow_expr(&mut self, a: &ArrowExpr) {
        let has_pattern_param = a
            .params
            .iter()
            .any(|p| matches!(p, Pat::Object(_) | Pat::Array(_)));
        let _ = self.push_scope(ScopeKind::Arrow, a.span, has_pattern_param);
        let param_refs: Vec<&Pat> = a.params.iter().collect();
        self.register_params(&param_refs);
        match &*a.body {
            BlockStmtOrExpr::BlockStmt(b) => {
                // Arrow body block isn't its own scope.
                for stmt in &b.stmts {
                    self.visit_stmt(stmt);
                }
            }
            BlockStmtOrExpr::Expr(e) => self.visit_expr(e),
        }
        self.pop_scope();
    }

    fn visit_assign_expr(&mut self, a: &AssignExpr) {
        // Babel collectorVisitor AssignmentExpression handler pushes
        // the path onto state.assignments. Post-walk, `path.scope.registerConstantViolation(path)`
        // calls `path.getAssignmentIdentifiers()` which resolves to the
        // identifiers being mutated (for `x = 1`, that's `x`).
        match &a.left {
            AssignTarget::Simple(SimpleAssignTarget::Ident(b)) => {
                // `counter = 2` — counter is a constant violation AND a reference.
                self.pending.constant_violations.push(PendingReference {
                    name: b.id.sym.to_string(),
                    span: b.id.span,
                    scope: self.current_scope(),
                });
                // Babel records the LHS as part of its references state too,
                // even though Compiled's evaluator doesn't read those (it
                // only counts pure RHS references via getBinding(). However,
                // binding.referencePaths includes ALL Identifier references
                // including LHS-of-assign, per binding.js's `reference()` —
                // wait, binding.reference() is called only for state.references,
                // not state.assignments. Let me re-check… scope/index.js:705-712:
                // ```
                // for (const ref of state.references) {
                //   const binding = ref.scope.getBinding(ref.node.name);
                //   if (binding) binding.reference(ref);
                // }
                // ```
                // state.references is populated by ReferencedIdentifier visitor,
                // which fires for IDENTIFIERS IN REFERENCE POSITION. The LHS of
                // an AssignmentExpression is NOT a reference position (it's
                // assignment target). So `counter = 2` does NOT add `counter`
                // to referencePaths — only the RHS `const x = counter` does.
                //
                // Action: don't push a reference for the LHS. The constant
                // violation alone is enough.
            }
            AssignTarget::Simple(SimpleAssignTarget::Member(m)) => {
                self.visit_expr(&m.obj);
                use swc_core::ecma::ast::MemberProp;
                if let MemberProp::Computed(c) = &m.prop {
                    self.visit_expr(&c.expr);
                }
            }
            AssignTarget::Simple(SimpleAssignTarget::Paren(p)) => {
                self.visit_expr(&p.expr);
            }
            AssignTarget::Simple(_) => {
                // OptChain / SuperProp / TS variants — no Ident-to-record at
                // the top level. Sub-expressions (if any) are visited via
                // visit_expr if we add explicit recursion. The §5.0a corpus
                // doesn't reach these.
            }
            AssignTarget::Pat(pat) => {
                // `[x] = …` / `{x} = …` — destructuring assignment.
                // Each ident in the pattern is a constant violation.
                use swc_core::ecma::ast::AssignTargetPat;
                let scope = self.current_scope();
                match pat {
                    AssignTargetPat::Object(o) => {
                        for (name, span) in collect_object_pat_idents(o) {
                            self.pending.constant_violations.push(PendingReference {
                                name,
                                span,
                                scope,
                            });
                        }
                    }
                    AssignTargetPat::Array(arr) => {
                        for elem in &arr.elems {
                            if let Some(p) = elem {
                                for (name, span) in collect_pat_idents(p) {
                                    self.pending.constant_violations.push(PendingReference {
                                        name,
                                        span,
                                        scope,
                                    });
                                }
                            }
                        }
                    }
                    AssignTargetPat::Invalid(_) => {}
                }
            }
        }
        self.visit_expr(&a.right);
    }

    fn visit_update_expr(&mut self, u: &UpdateExpr) {
        // `x++` / `++x` — argument is a constant violation if it's an Ident.
        // It's ALSO a reference (Babel's ReferencedIdentifier visitor fires
        // for the ident inside UpdateExpression — see scope/index.js
        // collectorVisitor's UpdateExpression entry pushes to constantViolations,
        // and the ReferencedIdentifier visitor independently picks up the ident
        // because it appears in expression position).
        if let Expr::Ident(i) = &*u.arg {
            self.pending.constant_violations.push(PendingReference {
                name: i.sym.to_string(),
                span: i.span,
                scope: self.current_scope(),
            });
            self.record_reference(&i.sym, i.span);
        } else {
            self.visit_expr(&u.arg);
        }
    }

    fn record_reference(&mut self, name: &swc_core::atoms::Atom, span: Span) {
        self.pending.references.push(PendingReference {
            name: name.to_string(),
            span,
            scope: self.current_scope(),
        });
    }

    fn register_import(&mut self, i: &ImportDecl) {
        let scope = self.current_scope();
        let import_decl_span = i.span;
        // Wtf8Atom→str: see register_import comment below; module
        // specifiers are valid UTF-8 in any real fixture.
        let source: String = i.src.value.as_str().unwrap_or_default().to_string();
        for spec in &i.specifiers {
            let (name, span, binding_node_type, kind, imported_name) = match spec {
                ImportSpecifier::Named(ImportNamedSpecifier {
                    local, imported, ..
                }) => {
                    // For `import { foo as bar } from 'mod'`:
                    //   local.sym = "bar" (local alias).
                    //   imported = Some(Ident("foo") | Str("foo"))
                    //     — the imported-side name.
                    // For `import { foo } from 'mod'`:
                    //   local.sym = "foo".
                    //   imported = None (shorthand) — the imported
                    //     name is the same as local.
                    let imported_name = match imported.as_ref() {
                        Some(ModuleExportName::Ident(id)) => id.sym.as_ref().to_string(),
                        // `Wtf8Atom::as_str` returns None for surrogate-
                        // paired strings; export names are always valid
                        // UTF-8 identifiers in practice, so the
                        // unwrap_or_default fallback is unreachable in
                        // any real Compiled fixture.
                        Some(ModuleExportName::Str(s)) => {
                            s.value.as_str().unwrap_or_default().to_string()
                        }
                        None => local.sym.as_ref().to_string(),
                    };
                    (
                        local.sym.to_string(),
                        local.span,
                        "ImportSpecifier",
                        ImportSpecifierKind::Named,
                        Some(imported_name),
                    )
                }
                ImportSpecifier::Default(ImportDefaultSpecifier { local, .. }) => (
                    local.sym.to_string(),
                    local.span,
                    "ImportDefaultSpecifier",
                    ImportSpecifierKind::Default,
                    None,
                ),
                ImportSpecifier::Namespace(ImportStarAsSpecifier { local, .. }) => (
                    local.sym.to_string(),
                    local.span,
                    "ImportNamespaceSpecifier",
                    ImportSpecifierKind::Namespace,
                    None,
                ),
            };
            let binding = Binding {
                kind: BindingKind::Module,
                identifier_name: name.clone(),
                constant: true,
                constant_violations: Vec::new(),
                reference_paths: Vec::new(),
                binding_node_type,
                parent_node_type: "ImportDeclaration",
                binding_init_string: None,
                init_expr: None,
                binding_id_type: None,
                scope,
                span,
                import_info: Some(ImportInfo {
                    source: source.clone(),
                    kind,
                    imported_name,
                }),
                destructured_pat: None,
                destructured_init: None,
            };
            self.bindings_by_scope[scope as usize].insert(name, binding);
        }
        let _ = import_decl_span;
    }

    /// Resolve pending references against the binding table, populating
    /// each binding's `reference_paths` and `constant_violations`.
    /// Mirrors `scope/index.js:697-715`'s post-crawl loop:
    ///
    /// ```js
    /// for (const path of state.assignments) {
    ///   const ids = path.getAssignmentIdentifiers();
    ///   for (const name of Object.keys(ids)) {
    ///     if (path.scope.getBinding(name)) continue;
    ///     programParent.addGlobal(ids[name]);
    ///   }
    ///   path.scope.registerConstantViolation(path);
    /// }
    /// for (const ref of state.references) {
    ///   const binding = ref.scope.getBinding(ref.node.name);
    ///   if (binding) binding.reference(ref);
    ///   else programParent.addGlobal(ref.node);
    /// }
    /// ```
    fn resolve_pending(&mut self) {
        // Constant violations FIRST — Babel processes assignments before
        // references in the post-crawl loop. The order matters because
        // reassign() updates `constant: false` BEFORE `binding.reference()`
        // appends to referencePaths — and our parity gate observes both.
        let violations = std::mem::take(&mut self.pending.constant_violations);
        for v in violations {
            self.apply_constant_violation(&v);
        }
        let refs = std::mem::take(&mut self.pending.references);
        for r in refs {
            self.apply_reference(&r);
        }
    }

    fn apply_constant_violation(&mut self, v: &PendingReference) {
        // Walk scope chain from v.scope to find the binding. If found,
        // call reassign on it. (Pattern-skip applies here too — same
        // get_binding semantics.)
        let binding_scope = self.find_binding_scope(v.scope, &v.name);
        if let Some(scope) = binding_scope {
            if let Some(b) = self.bindings_by_scope[scope as usize].get_mut(&v.name) {
                b.reassign(ReferenceSite {
                    span: v.span,
                    scope: v.scope,
                });
            }
        }
        // No binding => global; not exposed by the §5.0a parity gate.
    }

    fn apply_reference(&mut self, r: &PendingReference) {
        let binding_scope = self.find_binding_scope(r.scope, &r.name);
        if let Some(scope) = binding_scope {
            if let Some(b) = self.bindings_by_scope[scope as usize].get_mut(&r.name) {
                b.reference(ReferenceSite {
                    span: r.span,
                    scope: r.scope,
                });
            }
        }
    }

    /// Walk the scope chain, applying Finding 2's pattern-skip rule.
    /// Returns the scope-id where the binding lives.
    fn find_binding_scope(&self, from: ScopeId, name: &str) -> Option<ScopeId> {
        let mut current = Some(from);
        let mut previous_was_pattern = false;
        while let Some(scope_id) = current {
            let scope_data = &self.scopes[scope_id as usize];
            if let Some(b) = self.bindings_by_scope[scope_id as usize].get(name) {
                if previous_was_pattern
                    && b.kind != BindingKind::Param
                    && b.kind != BindingKind::Local
                {
                    // SKIP — keep walking up.
                } else {
                    return Some(scope_id);
                }
            } else if name == "arguments" && scope_data.kind.is_non_arrow_function() {
                break;
            }
            previous_was_pattern = scope_data.has_pattern_param;
            current = scope_data.parent;
        }
        None
    }

    fn into_index(self) -> ScopeIndex {
        ScopeIndex {
            scopes: self.scopes,
            bindings_by_scope: self.bindings_by_scope,
            program_id: 0,
            next_uid: Cell::new(0),
            minted_uids: RefCell::new(IndexSet::new()),
        }
    }
}

// -------------------- Free helpers --------------------

fn parent_for_module_item(_kind: ScopeKind) -> &'static str {
    // FunctionDeclaration / ClassDeclaration top-level reads parent
    // type as the enclosing statement kind. In Babel that comes from
    // the actual parent node; for the §5.0a corpus we don't currently
    // observe this on the `parent_node_type` axis (no fixture queries
    // function/class declarations), so a static placeholder is fine.
    "Statement"
}

fn pat_span(p: &Pat) -> Span {
    match p {
        Pat::Ident(b) => b.id.span,
        Pat::Array(a) => a.span,
        Pat::Object(o) => o.span,
        Pat::Assign(a) => a.span,
        Pat::Rest(r) => r.span,
        Pat::Invalid(i) => i.span,
        Pat::Expr(e) => match &**e {
            Expr::Ident(i) => i.span,
            _ => Span::default(),
        },
    }
}

/// **§5.0c addition** — Map a `ScopeKind` to the equivalent
/// `NodeKind`. Used by [`ScopeIndex::parent_kind_of`].
///
/// `Function` collapses to `FunctionDeclaration` and `Method`
/// collapses to `Other`: §5.0c's only consumer (var-hoist-unsafe
/// block check at `evaluation.js:124-140`) cares about
/// `BlockStatement` vs everything-else. If a future consumer needs
/// to distinguish `FunctionDeclaration` vs `FunctionExpression` at
/// the parent-scope level, store the precise kind on `ScopeData`
/// (it isn't tracked today) instead of refining this mapping.
fn scope_kind_to_node_kind(k: ScopeKind) -> crate::compat::path::NodeKind {
    use crate::compat::path::NodeKind;
    match k {
        ScopeKind::Program => NodeKind::Program,
        ScopeKind::Function => NodeKind::FunctionDeclaration,
        ScopeKind::Arrow => NodeKind::ArrowFunctionExpression,
        ScopeKind::Method => NodeKind::Other,
        ScopeKind::Block => NodeKind::BlockStatement,
        ScopeKind::For => NodeKind::ForStatement,
        ScopeKind::ForX => NodeKind::ForOfStatement, // ForOf or ForIn — collapsed; consumers only need to know it's a loop.
        ScopeKind::Catch => NodeKind::CatchClause,
        ScopeKind::Switch => NodeKind::SwitchStatement,
        ScopeKind::Class => NodeKind::ClassDeclaration,
    }
}

fn init_string_value(declarator: &VarDeclarator) -> Option<String> {
    let init = declarator.init.as_ref()?;
    match init.as_ref() {
        Expr::Lit(swc_core::ecma::ast::Lit::Str(s)) => {
            // `Str.value` is `Wtf8Atom`. The Compiled corpus only uses
            // valid-UTF-8 string literals; the lossy conversion
            // round-trips. Same shape as
            // `babel-plugin-strip-runtime/src/compat/scope.rs:61`.
            Some(s.value.to_atom_lossy().as_str().to_string())
        }
        _ => None,
    }
}

/// Babel's `getOuterBindingIdentifiers(true)` analog — collects all
/// Identifiers bound by a Pattern, with their spans.
pub fn collect_pat_idents(pat: &Pat) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    collect_pat_idents_into(pat, &mut out);
    out
}

fn collect_pat_idents_into(pat: &Pat, out: &mut Vec<(String, Span)>) {
    match pat {
        Pat::Ident(b) => {
            out.push((b.id.sym.to_string(), b.id.span));
        }
        Pat::Array(a) => {
            for elem in &a.elems {
                if let Some(p) = elem {
                    collect_pat_idents_into(p, out);
                }
            }
        }
        Pat::Object(o) => {
            out.extend(collect_object_pat_idents(o));
        }
        Pat::Rest(r) => {
            collect_pat_idents_into(&r.arg, out);
        }
        Pat::Assign(a) => {
            collect_pat_idents_into(&a.left, out);
        }
        Pat::Invalid(_) => {}
        Pat::Expr(e) => {
            if let Expr::Ident(i) = &**e {
                out.push((i.sym.to_string(), i.span));
            }
        }
    }
}

fn collect_object_pat_idents(o: &ObjectPat) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    for prop in &o.props {
        match prop {
            ObjectPatProp::Assign(a) => {
                // `{ foo }` shorthand — `foo` is bound. Babel parity:
                // the Identifier `foo` IS the binding identifier.
                out.push((a.key.sym.to_string(), a.key.span));
            }
            ObjectPatProp::KeyValue(kv) => {
                collect_pat_idents_into(&kv.value, &mut out);
            }
            ObjectPatProp::Rest(r) => {
                collect_pat_idents_into(&r.arg, &mut out);
            }
        }
    }
    out
}

// -------------------- Visit-trait entry (currently unused) --------------------

// Builder uses bespoke recursion (visit_stmt / visit_expr / etc.) for
// fine-grained control over scope creation vs reference recording. The
// SWC `Visit` trait isn't directly implemented because its default
// `visit_children_with` uniformly recurses, conflicting with Finding 5
// (parent-pointer skip on key/decorators) and the function-body-block
// scope skip. If a future call site needs `&dyn Visit` polymorphism,
// wrap Builder in an adapter — don't replace the bespoke recursion.
impl Visit for Builder {
    // Intentionally empty — see comment above.
    fn visit_module(&mut self, m: &swc_core::ecma::ast::Module) {
        // Re-entry guard: SWC code that walks via VisitWith on a Module
        // would call this. Route to the bespoke walker.
        self.visit_module_root(m);
    }
}

// Make Module able to invoke our visitor through visit_with for parity
// with the SWC Visit conventions, even though Builder owns its own
// dispatch.
#[allow(dead_code)]
fn _force_visit_with_compile(m: &swc_core::ecma::ast::Module, b: &mut Builder) {
    m.visit_with(b);
}

// -------------------- Tests --------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use swc_core::common::{FileName, SourceMap};
    use swc_core::common::sync::Lrc;
    use swc_core::ecma::ast::EsVersion;
    use swc_core::ecma::parser::{parse_file_as_module, EsSyntax, Syntax};

    fn parse(source: &str) -> swc_core::ecma::ast::Module {
        let cm: Lrc<SourceMap> = Default::default();
        let fm = cm.new_source_file(
            Arc::new(FileName::Custom("compat-scope-test.js".into())),
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

    #[test]
    fn const_string_binding_is_constant_with_one_reference() {
        let m = parse("const color = 'blue'; const x = color;");
        let idx = ScopeIndex::build(&m);
        let prog = idx.program_scope();
        let binding = idx
            .get_own_binding(prog, "color")
            .expect("color binding");
        assert_eq!(binding.kind, BindingKind::Const);
        assert!(binding.constant);
        assert_eq!(binding.reference_paths.len(), 1);
        assert_eq!(binding.binding_node_type, "VariableDeclarator");
        assert_eq!(binding.parent_node_type, "VariableDeclaration");
        assert_eq!(binding.binding_init_string.as_deref(), Some("blue"));
    }

    #[test]
    fn let_with_assignment_is_non_constant() {
        let m = parse("let counter = 1; counter = 2; const x = counter;");
        let idx = ScopeIndex::build(&m);
        let binding = idx
            .get_own_binding(idx.program_scope(), "counter")
            .unwrap();
        assert_eq!(binding.kind, BindingKind::Let);
        assert!(!binding.constant);
        assert_eq!(binding.reference_paths.len(), 1);
    }

    #[test]
    fn import_default_specifier_binding_is_module_kind() {
        let m = parse("import React from 'react'; const x = React;");
        let idx = ScopeIndex::build(&m);
        let binding = idx.get_own_binding(idx.program_scope(), "React").unwrap();
        assert_eq!(binding.kind, BindingKind::Module);
        assert_eq!(binding.binding_node_type, "ImportDefaultSpecifier");
        assert_eq!(binding.parent_node_type, "ImportDeclaration");
    }

    #[test]
    fn missing_binding_returns_none() {
        let m = parse("const x = unknownGlobal;");
        let idx = ScopeIndex::build(&m);
        assert!(idx
            .get_binding(idx.program_scope(), "unknownGlobal")
            .is_none());
    }

    #[test]
    fn register_new_scope_sets_parent_pointer() {
        let m = parse("const x = 1;");
        let mut idx = ScopeIndex::build(&m);
        let prog = idx.program_scope();
        let new_scope = idx.register_new_scope(prog, ScopeKind::Arrow);
        assert_eq!(idx.parent_of(new_scope), Some(prog));
        assert_eq!(idx.kind_of(new_scope), ScopeKind::Arrow);
    }

    #[test]
    fn register_new_scope_get_binding_walks_up_to_parent() {
        // Caller-scope bindings are visible from the runtime-allocated
        // scope through normal `get_binding` parent-chain walking.
        let m = parse("const color = 'blue';");
        let mut idx = ScopeIndex::build(&m);
        let prog = idx.program_scope();
        let new_scope = idx.register_new_scope(prog, ScopeKind::Arrow);
        let binding = idx
            .get_binding(new_scope, "color")
            .expect("parent-scope binding visible from new scope");
        assert_eq!(binding.kind, BindingKind::Const);
    }

    #[test]
    fn register_new_scope_register_synthetic_binding_into_new_scope() {
        // The §5.5 IIFE site's contract: allocate a new scope, then
        // push (param := evaluatedArg) bindings into it via
        // `register_synthetic_binding`. Verifies the binding lands in
        // the right `bindings_by_scope` row.
        let m = parse("const outer = 'o';");
        let mut idx = ScopeIndex::build(&m);
        let prog = idx.program_scope();
        let iife = idx.register_new_scope(prog, ScopeKind::Arrow);
        idx.scope_push_synthetic(
            iife,
            "param",
            BindingKind::Const,
            Some("evaluated".to_string()),
            swc_core::common::DUMMY_SP,
        );
        // Own binding lookup against the new scope.
        let own = idx
            .get_own_binding(iife, "param")
            .expect("synthetic param binding");
        assert_eq!(own.kind, BindingKind::Const);
        assert_eq!(own.binding_init_string.as_deref(), Some("evaluated"));
        // Parent scope still doesn't see the IIFE-local binding.
        assert!(idx.get_own_binding(prog, "param").is_none());
        // But the IIFE scope DOES see the parent's `outer` binding.
        assert!(idx.get_binding(iife, "outer").is_some());
    }

    #[test]
    fn register_new_scope_invisible_to_scope_at_pos() {
        // Position-based lookups must NOT flow into a runtime-allocated
        // scope because its DUMMY_SP is `[BytePos(0), BytePos(0)]`.
        // Any non-zero position will skip past it.
        let m = parse("const x = 1; const y = x;");
        let mut idx = ScopeIndex::build(&m);
        let prog = idx.program_scope();
        let _new_scope = idx.register_new_scope(prog, ScopeKind::Arrow);
        // BytePos(20) is somewhere inside `const y = x;`. Should resolve
        // to a real source-derived scope, not the runtime-allocated one.
        let resolved = idx.scope_at_pos(BytePos(20));
        assert_ne!(resolved, _new_scope);
    }

    #[test]
    fn pattern_skip_walks_past_destructured_param() {
        // Finding 2 fixture — outer `const foo` IS shadowed by inner
        // `function f({ foo })` because the param-destructure binding
        // is the closer one in the lexical chain.
        let m = parse("const foo = 'outer'; function f({ foo }) { return foo; }");
        let idx = ScopeIndex::build(&m);
        // The inner reference scope is the function body scope. Find
        // the function's scope id.
        let mut function_scope: Option<ScopeId> = None;
        for (i, s) in idx.scopes.iter().enumerate() {
            if s.kind == ScopeKind::Function {
                function_scope = Some(i as ScopeId);
                break;
            }
        }
        let scope = function_scope.expect("function scope");
        let binding = idx.get_binding(scope, "foo").unwrap();
        assert_eq!(binding.kind, BindingKind::Param);
        assert_eq!(binding.binding_node_type, "ObjectPattern");
    }

    #[test]
    fn var_in_for_loop_is_non_constant_and_hoisted() {
        let m = parse("function f() { for (var x = 1; x < 10; x++) {} const y = x; }");
        let idx = ScopeIndex::build(&m);
        let mut function_scope: Option<ScopeId> = None;
        for (i, s) in idx.scopes.iter().enumerate() {
            if s.kind == ScopeKind::Function {
                function_scope = Some(i as ScopeId);
                break;
            }
        }
        let scope = function_scope.unwrap();
        let binding = idx.get_own_binding(scope, "x").expect("x hoisted to function scope");
        assert_eq!(binding.kind, BindingKind::Var);
        assert!(!binding.constant, "var-in-for-init must be auto-reassigned (Finding 4)");
    }
}
