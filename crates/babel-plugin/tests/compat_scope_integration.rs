//! Phase 5 §5.0a/b — Rust `compat::scope` + `compat::path` parity gate.
//!
//! Reads the JS-locked corpus at `tests/compat_scope_corpus.json`
//! (regenerable via `bun parity-harness/compat-scope/oracle.mjs`)
//! and asserts that the Rust pre-indexed scope walker produces the
//! same binding/path-shape observables as upstream
//! `@babel/traverse@7.29.0` for every entry.
//!
//! ## Why this gate exists
//!
//! `packages/babel-plugin/src/utils/{evaluate-expression,resolve-binding}.ts`
//! and `utils/traverse-expression/*.ts` (the §5.4–§5.6 ports)
//! depend on `path.scope.getBinding(name)`,
//! `path.scope.getOwnBinding(name)`, `path.scope.push(...)`,
//! `binding.path.node`, `binding.constant`, `binding.referencePaths`,
//! `path.parentPath`, and `path.listKey`. None of those exist in
//! SWC's plugin runtime; `crates/babel-plugin/src/compat/{scope,path}.rs`
//! provides 1:1 analogues against the pre-indexed scope tree built
//! at `Program::enter`.
//!
//! Drift in any of those observables silently produces a wrong
//! evaluator output, which silently produces a wrong CSS class hash
//! (per §3 / §4.4 hash-call-shape sites), which silently renames a
//! production class. Same blast radius as `compat::generator`'s
//! byte-parity gate.
//!
//! ## Status (Phase 5 §5.0a closed)
//!
//! `compat::scope` ships in §5.0a. `compat::path` (`PathHandle`,
//! `replace_with`, `traverse`, `get(field)`, `parent_path`) is §5.0b
//! and is NOT yet exposed; the §5.0a gate uses the scope-only API
//! plus a small per-call-site reference-finder visitor (mirroring
//! the oracle's `findFirstReferenceOnRhs` etc.) to drive the
//! query-axis dispatcher.
//!
//! See `plugins/STATUS.md` Phase 5 row §5.0,
//! `plugins/COMPAT_SCOPE_AUDIT.md` for the surface enumeration +
//! Q1/Q2/Q3 lock, and
//! `parity-harness/compat-scope/{oracle.mjs,fixtures.json}` for
//! the JS oracle producing this corpus.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;
use serde_json::{json, Value};

use babel_plugin::compat::scope::{BindingKind, ScopeId, ScopeIndex, ScopeKind};
use swc_core::common::sync::Lrc;
use swc_core::common::{BytePos, FileName, SourceMap, Span, DUMMY_SP};
use swc_core::ecma::ast::{
    EsVersion, Expr, MemberExpr, Module, ModuleItem, Pat, Stmt, VarDeclarator,
};
use swc_core::ecma::parser::{parse_file_as_module, EsSyntax, Syntax};

// AFM-pinned versions; mirror the constants in
// `parity-harness/compat-scope/oracle.mjs` and the row in
// `crates/PARITY_VERSIONS.md`.
const EXPECTED_TRAVERSE_VERSION: &str = "7.29.0";
const EXPECTED_PARSER_VERSION: &str = "7.29.2";

// Six query axes mirrored from
// `parity-harness/compat-scope/oracle.mjs`'s `QUERIES` table.
// Adding a new axis = changing this list AND adding a Rust
// query-runner branch in `run_observation` AND in oracle.mjs's table.
const EXPECTED_CALL_SITES: &[&str] = &[
    "binding-lookup-from-reference",
    "generate-uid",
    "has-own-binding",
    "list-key-arguments",
    "path-predicate-via-binding",
    "scope-push-iife",
];

#[derive(Deserialize)]
struct Corpus {
    version: u32,
    babel_traverse_version: String,
    babel_parser_version: String,
    #[allow(dead_code)] // read for documentation; not asserted (peer-dep slot).
    babel_types_version: String,
    entry_count: usize,
    call_site_counts: BTreeMap<String, usize>,
    entries: Vec<Entry>,
}

#[derive(Deserialize)]
struct Entry {
    label: String,
    call_site: String,
    input_source: String,
    #[allow(dead_code)] // future-proof: oracle may evolve to use it
    lookup_name: Option<String>,
    #[allow(dead_code)]
    lookup_from: Option<String>,
    expected: Value,
    #[allow(dead_code)]
    observed: Value,
}

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("compat_scope_corpus.json")
}

fn load_corpus() -> Corpus {
    let path = corpus_path();
    let raw = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "could not read compat-scope corpus at {}: {}\n\
             Regenerate with: bun parity-harness/compat-scope/oracle.mjs",
            path.display(),
            e
        )
    });
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!("compat-scope corpus has invalid shape: {}", e)
    })
}

#[test]
fn corpus_shape_lock() {
    let corpus = load_corpus();

    assert_eq!(corpus.version, 1, "corpus schema version mismatch");
    assert_eq!(
        corpus.babel_traverse_version, EXPECTED_TRAVERSE_VERSION,
        "@babel/traverse pin drift in corpus — regenerate via \
         `bun parity-harness/compat-scope/oracle.mjs` after \
         confirming the pin in package.json#overrides AND \
         devDependencies matches crates/PARITY_VERSIONS.md"
    );
    assert_eq!(
        corpus.babel_parser_version, EXPECTED_PARSER_VERSION,
        "@babel/parser pin drift in corpus — same fix as above"
    );

    assert_eq!(
        corpus.entry_count,
        corpus.entries.len(),
        "corpus.entry_count != corpus.entries.len()"
    );
    assert!(
        corpus.entry_count > 0,
        "corpus is empty — fixtures.json or oracle.mjs is broken"
    );

    for axis in EXPECTED_CALL_SITES {
        let count = corpus.call_site_counts.get(*axis).copied().unwrap_or(0);
        assert!(
            count > 0,
            "call_site axis `{axis}` has no entries — fixtures.json \
             must seed at least one fixture per axis. Either add a \
             fixture or remove the axis from EXPECTED_CALL_SITES."
        );
    }

    for entry in &corpus.entries {
        assert!(
            EXPECTED_CALL_SITES.contains(&entry.call_site.as_str()),
            "entry `{}` has unexpected call_site `{}` — update \
             EXPECTED_CALL_SITES + run_observation + oracle.mjs together",
            entry.label,
            entry.call_site
        );
    }
}

#[test]
fn corpus_observed_matches_expected_oracle_self_consistency() {
    // Sanity-checks the oracle-side self-consistency assertion that
    // `parity-harness/compat-scope/oracle.mjs` already enforces:
    // every key in `expected` must equal the corresponding key in
    // `observed`. If this fires, the oracle script silently regressed
    // — re-run it and check why.
    let corpus = load_corpus();
    for entry in &corpus.entries {
        let expected = entry.expected.as_object().unwrap_or_else(|| {
            panic!("entry `{}`: expected is not an object", entry.label)
        });
        let observed = entry.observed.as_object().unwrap_or_else(|| {
            panic!("entry `{}`: observed is not an object", entry.label)
        });
        for (k, v) in expected {
            let got = observed.get(k).unwrap_or_else(|| {
                panic!(
                    "entry `{}`: expected key `{}` missing from observed",
                    entry.label, k
                )
            });
            assert_eq!(
                got, v,
                "entry `{}`: oracle self-consistency violated on key `{}` (corpus is stale)",
                entry.label, k
            );
        }
    }
}

#[test]
fn rust_compat_scope_matches_js_corpus() {
    // Phase 5 §5.0a unblock: parses each corpus `input_source` via
    // swc_core, builds the pre-indexed scope tree, dispatches on
    // `entry.call_site` to the matching Rust query, and asserts the
    // observed output structurally equals `entry.expected` for every
    // entry. Drift here = Rust port deviates from
    // `@babel/traverse@7.29.0`. Per CLAUDE.md / COMPAT_SCOPE_AUDIT.md,
    // diagnose and fix the root-cause divergence; never patch the
    // corpus or downgrade the assertion.
    let corpus = load_corpus();
    let mut failures: Vec<String> = Vec::new();
    for entry in &corpus.entries {
        let module = parse_module(&entry.input_source);
        let observed = match entry.call_site.as_str() {
            "binding-lookup-from-reference" => run_binding_lookup(&module, entry),
            "path-predicate-via-binding" => run_path_predicate(&module, entry),
            "has-own-binding" => run_has_own_binding(&module, entry),
            "scope-push-iife" => run_scope_push_iife(&module, entry),
            "generate-uid" => run_generate_uid(&module, entry),
            "list-key-arguments" => run_list_key_arguments(&module, entry),
            other => panic!("unsupported call_site `{}`", other),
        };
        if let Err(msg) = assert_observed_matches_expected(entry, &observed) {
            failures.push(format!("[{}] {}", entry.label, msg));
        }
    }
    assert!(
        failures.is_empty(),
        "{} fixture(s) diverged from the JS oracle:\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}

// -------------------- Rust query runners --------------------
// One per oracle query in `parity-harness/compat-scope/oracle.mjs`.
// Each must mirror its oracle counterpart's logic 1:1 — the gate is
// "same observed shape", not "Rust intrinsic correctness".

fn run_binding_lookup(module: &Module, entry: &Entry) -> Value {
    let lookup_name = entry
        .lookup_name
        .as_ref()
        .expect("binding-lookup-from-reference fixture must declare lookup_name");
    let scope = ScopeIndex::build(module);

    // Find the first identifier reference on the RHS matching name —
    // mirrors oracle's findFirstReferenceOnRhs.
    let ref_pos = match find_first_reference_on_rhs(module, lookup_name) {
        Some(span) => span.lo,
        None => return json!({ "found": false, "_why": format!("no reference to {} found in program", lookup_name) }),
    };

    let scope_at_ref = scope.scope_at_pos(ref_pos);
    let binding = match scope.get_binding(scope_at_ref, lookup_name) {
        Some(b) => b,
        None => return json!({ "found": false }),
    };

    let mut out = serde_json::Map::new();
    out.insert("found".into(), Value::Bool(true));
    out.insert(
        "binding_node_type".into(),
        Value::String(binding.binding_node_type.to_string()),
    );
    out.insert(
        "binding_kind".into(),
        Value::String(binding.kind.as_str().to_string()),
    );
    out.insert("constant".into(), Value::Bool(binding.constant));
    out.insert(
        "reference_paths_count".into(),
        Value::Number(serde_json::Number::from(binding.reference_paths.len())),
    );
    out.insert(
        "parent_path_type".into(),
        Value::String(binding.parent_node_type.to_string()),
    );

    // Optional secondary observables — emit only when the fixture's
    // expected map names them, mirroring the oracle's per-row gating.
    if entry
        .expected
        .as_object()
        .map(|m| m.contains_key("binding_init_string"))
        .unwrap_or(false)
    {
        out.insert(
            "binding_init_string".into(),
            binding
                .binding_init_string
                .as_ref()
                .map(|s| Value::String(s.clone()))
                .unwrap_or(Value::Null),
        );
    }
    if entry
        .expected
        .as_object()
        .map(|m| m.contains_key("binding_id_type"))
        .unwrap_or(false)
    {
        out.insert(
            "binding_id_type".into(),
            binding
                .binding_id_type
                .map(|s| Value::String(s.to_string()))
                .unwrap_or(Value::Null),
        );
    }

    Value::Object(out)
}

fn run_path_predicate(module: &Module, entry: &Entry) -> Value {
    let lookup_name = entry
        .lookup_name
        .as_ref()
        .expect("path-predicate-via-binding fixture must declare lookup_name");
    let scope = ScopeIndex::build(module);
    let ref_pos = match find_first_reference_on_rhs(module, lookup_name) {
        Some(span) => span.lo,
        None => return json!({ "found": false }),
    };
    let scope_at_ref = scope.scope_at_pos(ref_pos);
    let binding = match scope.get_binding(scope_at_ref, lookup_name) {
        Some(b) => b,
        None => return json!({ "found": false }),
    };

    // Mirror oracle's predicate fan-out:
    //   is_import_declaration_parent: binding.path.parentPath.isImportDeclaration()
    //   is_export_named_declaration:  binding.path.isExportNamedDeclaration()
    //   is_object_pattern:            binding.path.isObjectPattern()
    //   is_variable_declarator:       t.isVariableDeclarator(binding.path.node)
    //
    // For our compat::scope, `binding.parent_node_type` carries the
    // parent's type and `binding_node_type` carries the binding's own.
    let is_import_declaration_parent = binding.parent_node_type == "ImportDeclaration";
    let is_export_named_declaration = binding.binding_node_type == "ExportNamedDeclaration";
    let is_object_pattern = binding.binding_node_type == "ObjectPattern";
    let is_variable_declarator = binding.binding_node_type == "VariableDeclarator";

    json!({
        "found": true,
        "is_import_declaration_parent": is_import_declaration_parent,
        "is_export_named_declaration": is_export_named_declaration,
        "is_object_pattern": is_object_pattern,
        "is_variable_declarator": is_variable_declarator,
    })
}

fn run_has_own_binding(module: &Module, entry: &Entry) -> Value {
    let lookup_name = entry
        .lookup_name
        .as_ref()
        .expect("has-own-binding fixture must declare lookup_name");
    let scope = ScopeIndex::build(module);

    // Oracle's findFirstFunctionBodyBlock: first BlockStatement whose
    // parent is a Function/Arrow. In our scope tree the function body
    // block ISN'T its own scope (matches Babel's isScope rule), so
    // we resolve to the enclosing function/arrow scope itself.
    let function_scope = match find_first_function_scope(&scope) {
        Some(s) => s,
        None => {
            return json!({
                "has_own_binding": false,
                "has_binding": false,
                "_why": "no function body block",
            })
        }
    };

    json!({
        "has_own_binding": scope.has_own_binding(function_scope, lookup_name),
        "has_binding": scope.has_binding(function_scope, lookup_name, false),
    })
}

fn run_scope_push_iife(module: &Module, entry: &Entry) -> Value {
    let lookup_name = entry
        .lookup_name
        .as_ref()
        .expect("scope-push-iife fixture must declare lookup_name");
    let mut scope = ScopeIndex::build(module);

    // Oracle: find the first ArrowFunctionExpression, push a
    // synthetic `const param = "val"` into its scope, then check
    // both the arrow scope (own binding) and the module scope
    // (no binding) for the new name.
    let arrow_scope = find_first_arrow_scope(&scope)
        .expect("fixture promised an arrow function but none was found");

    scope.scope_push_synthetic(
        arrow_scope,
        lookup_name,
        BindingKind::Const,
        Some("val".into()),
        DUMMY_SP,
    );

    let own_after = scope.get_own_binding(arrow_scope, lookup_name);
    // Walk to module-scope-equivalent: walk the parent chain to the
    // outermost (Program) scope. Then check whether the synthetic
    // binding leaked there. It mustn't.
    let mut s = arrow_scope;
    while let Some(p) = scope.parent_of(s) {
        s = p;
    }
    let module_has_it = scope.get_binding(s, lookup_name).is_some();

    json!({
        "after_push_has_own_binding_in_arrow_scope": own_after.is_some(),
        "after_push_has_binding_in_module_scope": module_has_it,
        "binding_node_type_after_push": own_after.map(|b| b.binding_node_type.to_string()),
        "binding_kind_after_push": own_after.map(|b| b.kind.as_str().to_string()),
    })
}

fn run_generate_uid(module: &Module, _entry: &Entry) -> Value {
    let scope = ScopeIndex::build(module);
    let prog = scope.program_scope();
    let a = scope.generate_uid_identifier("");
    let b = scope.generate_uid_identifier("");
    let existing_x = scope.get_binding(prog, "x").is_some();

    json!({
        "first_uid_name_starts_with_underscore": a.starts_with('_'),
        "second_uid_name_differs_from_first": a != b,
        "neither_collides_with_existing_x": a != "x" && b != "x" && existing_x,
    })
}

fn run_list_key_arguments(module: &Module, _entry: &Entry) -> Value {
    // Oracle: find first MemberExpression in the AST and report its
    // listKey. In `foo(bar.baz)`, the MemberExpression `bar.baz`
    // appears inside the call's arguments list, so listKey ===
    // "arguments".
    //
    // For our compat::scope, we don't yet model `path.listKey` as a
    // first-class field — that's §5.0b's `PathHandle` territory.
    // The §5.0a parity gate covers this axis with a tiny synthetic
    // walker that mirrors the upstream "first MemberExpression's
    // surrounding container" semantic against the parsed Module.
    let list_key = find_first_member_expr_list_key(module);
    json!({ "member_expr_list_key": list_key })
}

// -------------------- Helpers (visitor-style scans) --------------------

fn parse_module(source: &str) -> Module {
    let cm: Lrc<SourceMap> = Default::default();
    let fm = cm.new_source_file(
        Arc::new(FileName::Custom("compat-scope-fixture.js".into())),
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

/// Mirrors oracle's `findFirstReferenceOnRhs` (parity-harness/compat-scope/oracle.mjs:100-133).
/// Walks the AST in source order, returning the span of the first
/// Identifier matching `name` that's in a reference position
/// (NOT a VariableDeclarator id, NOT an object-property key,
/// NOT an import specifier local, NOT a function param,
/// NOT the LHS of an AssignmentExpression).
fn find_first_reference_on_rhs(module: &Module, name: &str) -> Option<Span> {
    let mut state = RefFinder {
        target: name,
        result: None,
    };
    state.walk_module(module);
    state.result
}

struct RefFinder<'a> {
    target: &'a str,
    result: Option<Span>,
}

impl RefFinder<'_> {
    fn done(&self) -> bool {
        self.result.is_some()
    }

    fn record(&mut self, ident: &swc_core::ecma::ast::Ident) {
        if self.done() {
            return;
        }
        if ident.sym == self.target {
            self.result = Some(ident.span);
        }
    }

    fn walk_module(&mut self, m: &Module) {
        for item in &m.body {
            if self.done() {
                return;
            }
            self.walk_module_item(item);
        }
    }

    fn walk_module_item(&mut self, item: &ModuleItem) {
        match item {
            ModuleItem::ModuleDecl(_) => {
                // Imports / exports — local idents on these are bindings,
                // not references. Skip per the oracle's exclusions.
            }
            ModuleItem::Stmt(stmt) => self.walk_stmt(stmt),
        }
    }

    fn walk_stmt(&mut self, stmt: &Stmt) {
        if self.done() {
            return;
        }
        match stmt {
            Stmt::Block(b) => {
                for s in &b.stmts {
                    self.walk_stmt(s);
                }
            }
            Stmt::Decl(d) => self.walk_decl(d),
            Stmt::Expr(e) => self.walk_expr(&e.expr),
            Stmt::Return(r) => {
                if let Some(arg) = &r.arg {
                    self.walk_expr(arg);
                }
            }
            Stmt::If(i) => {
                self.walk_expr(&i.test);
                self.walk_stmt(&i.cons);
                if let Some(alt) = &i.alt {
                    self.walk_stmt(alt);
                }
            }
            Stmt::For(f) => {
                if let Some(init) = &f.init {
                    use swc_core::ecma::ast::VarDeclOrExpr;
                    match init {
                        VarDeclOrExpr::VarDecl(v) => {
                            for d in &v.decls {
                                if let Some(init) = &d.init {
                                    self.walk_expr(init);
                                }
                            }
                        }
                        VarDeclOrExpr::Expr(e) => self.walk_expr(e),
                    }
                }
                if let Some(test) = &f.test {
                    self.walk_expr(test);
                }
                if let Some(update) = &f.update {
                    self.walk_expr(update);
                }
                self.walk_stmt(&f.body);
            }
            Stmt::ForIn(f) => {
                self.walk_expr(&f.right);
                self.walk_stmt(&f.body);
            }
            Stmt::ForOf(f) => {
                self.walk_expr(&f.right);
                self.walk_stmt(&f.body);
            }
            Stmt::While(w) => {
                self.walk_expr(&w.test);
                self.walk_stmt(&w.body);
            }
            Stmt::DoWhile(d) => {
                self.walk_stmt(&d.body);
                self.walk_expr(&d.test);
            }
            Stmt::Switch(s) => {
                self.walk_expr(&s.discriminant);
                for case in &s.cases {
                    if let Some(test) = &case.test {
                        self.walk_expr(test);
                    }
                    for stmt in &case.cons {
                        self.walk_stmt(stmt);
                    }
                }
            }
            Stmt::Throw(t) => self.walk_expr(&t.arg),
            Stmt::Try(t) => {
                for s in &t.block.stmts {
                    self.walk_stmt(s);
                }
                if let Some(handler) = &t.handler {
                    for s in &handler.body.stmts {
                        self.walk_stmt(s);
                    }
                }
                if let Some(finalizer) = &t.finalizer {
                    for s in &finalizer.stmts {
                        self.walk_stmt(s);
                    }
                }
            }
            Stmt::Labeled(l) => self.walk_stmt(&l.body),
            Stmt::With(w) => {
                self.walk_expr(&w.obj);
                self.walk_stmt(&w.body);
            }
            _ => {}
        }
    }

    fn walk_decl(&mut self, decl: &swc_core::ecma::ast::Decl) {
        use swc_core::ecma::ast::Decl;
        match decl {
            Decl::Var(v) => {
                for d in &v.decls {
                    self.walk_var_declarator(d);
                }
            }
            Decl::Fn(fn_decl) => {
                // Skip ident (binding LHS); descend into params and body.
                self.walk_function_body(&fn_decl.function);
            }
            Decl::Class(class_decl) => {
                self.walk_class(&class_decl.class);
            }
            _ => {}
        }
    }

    fn walk_var_declarator(&mut self, d: &VarDeclarator) {
        // Skip d.name (binding LHS). Recurse into init.
        if let Some(init) = &d.init {
            self.walk_expr(init);
        }
    }

    fn walk_function_body(&mut self, function: &swc_core::ecma::ast::Function) {
        // Params: skip identifier names (they're bindings); recurse
        // only into Assign default values which CAN have references.
        for param in &function.params {
            self.walk_pat_for_default_exprs(&param.pat);
        }
        if let Some(body) = &function.body {
            for stmt in &body.stmts {
                self.walk_stmt(stmt);
            }
        }
    }

    fn walk_pat_for_default_exprs(&mut self, pat: &Pat) {
        match pat {
            Pat::Assign(a) => {
                self.walk_pat_for_default_exprs(&a.left);
                self.walk_expr(&a.right);
            }
            Pat::Object(o) => {
                use swc_core::ecma::ast::ObjectPatProp;
                for p in &o.props {
                    match p {
                        ObjectPatProp::KeyValue(kv) => {
                            self.walk_pat_for_default_exprs(&kv.value);
                        }
                        ObjectPatProp::Assign(a) => {
                            if let Some(value) = &a.value {
                                self.walk_expr(value);
                            }
                        }
                        ObjectPatProp::Rest(r) => {
                            self.walk_pat_for_default_exprs(&r.arg);
                        }
                    }
                }
            }
            Pat::Array(arr) => {
                for elem in &arr.elems {
                    if let Some(p) = elem {
                        self.walk_pat_for_default_exprs(p);
                    }
                }
            }
            Pat::Rest(r) => self.walk_pat_for_default_exprs(&r.arg),
            _ => {}
        }
    }

    fn walk_class(&mut self, class: &swc_core::ecma::ast::Class) {
        use swc_core::ecma::ast::ClassMember;
        for member in &class.body {
            match member {
                ClassMember::Method(m) => self.walk_function_body(&m.function),
                ClassMember::Constructor(c) => {
                    if let Some(body) = &c.body {
                        for stmt in &body.stmts {
                            self.walk_stmt(stmt);
                        }
                    }
                }
                ClassMember::ClassProp(p) => {
                    if let Some(value) = &p.value {
                        self.walk_expr(value);
                    }
                }
                _ => {}
            }
        }
    }

    fn walk_expr(&mut self, expr: &Expr) {
        if self.done() {
            return;
        }
        match expr {
            Expr::Ident(i) => self.record(i),
            Expr::Array(a) => {
                for elem in &a.elems {
                    if let Some(spread_or_expr) = elem {
                        self.walk_expr(&spread_or_expr.expr);
                    }
                }
            }
            Expr::Object(o) => {
                use swc_core::ecma::ast::PropOrSpread;
                use swc_core::ecma::ast::Prop;
                use swc_core::ecma::ast::PropName;
                for prop_or_spread in &o.props {
                    match prop_or_spread {
                        PropOrSpread::Spread(s) => self.walk_expr(&s.expr),
                        PropOrSpread::Prop(p) => match &**p {
                            Prop::Shorthand(_) => {
                                // Skip shorthand: the oracle's
                                // findFirstReferenceOnRhs explicitly
                                // skips ObjectProperty key positions.
                                // The shorthand IS both key and value.
                                // For the §5.0a corpus the shorthand
                                // shape isn't reached by the
                                // first-reference-on-rhs queries.
                            }
                            Prop::KeyValue(kv) => {
                                if let PropName::Computed(c) = &kv.key {
                                    self.walk_expr(&c.expr);
                                }
                                self.walk_expr(&kv.value);
                            }
                            Prop::Method(m) => self.walk_function_body(&m.function),
                            Prop::Getter(g) => {
                                if let Some(body) = &g.body {
                                    for stmt in &body.stmts {
                                        self.walk_stmt(stmt);
                                    }
                                }
                            }
                            Prop::Setter(s) => {
                                if let Some(body) = &s.body {
                                    for stmt in &body.stmts {
                                        self.walk_stmt(stmt);
                                    }
                                }
                            }
                            Prop::Assign(_) => {}
                        },
                    }
                }
            }
            Expr::Fn(fn_expr) => self.walk_function_body(&fn_expr.function),
            Expr::Class(class_expr) => self.walk_class(&class_expr.class),
            Expr::Arrow(a) => {
                use swc_core::ecma::ast::BlockStmtOrExpr;
                for param in &a.params {
                    self.walk_pat_for_default_exprs(param);
                }
                match &*a.body {
                    BlockStmtOrExpr::BlockStmt(b) => {
                        for stmt in &b.stmts {
                            self.walk_stmt(stmt);
                        }
                    }
                    BlockStmtOrExpr::Expr(e) => self.walk_expr(e),
                }
            }
            Expr::Unary(u) => self.walk_expr(&u.arg),
            Expr::Update(u) => self.walk_expr(&u.arg),
            Expr::Bin(b) => {
                self.walk_expr(&b.left);
                self.walk_expr(&b.right);
            }
            Expr::Assign(a) => {
                // Oracle skips AssignmentExpression.left == path.node
                // for ident-LHS — i.e. the LHS isn't a "reference"
                // for the find-first-reference query. Don't walk left.
                self.walk_expr(&a.right);
            }
            Expr::Member(m) => {
                self.walk_expr(&m.obj);
                use swc_core::ecma::ast::MemberProp;
                if let MemberProp::Computed(c) = &m.prop {
                    self.walk_expr(&c.expr);
                }
            }
            Expr::Cond(c) => {
                self.walk_expr(&c.test);
                self.walk_expr(&c.cons);
                self.walk_expr(&c.alt);
            }
            Expr::Call(c) => {
                use swc_core::ecma::ast::Callee;
                if let Callee::Expr(e) = &c.callee {
                    self.walk_expr(e);
                }
                for arg in &c.args {
                    self.walk_expr(&arg.expr);
                }
            }
            Expr::New(n) => {
                self.walk_expr(&n.callee);
                if let Some(args) = &n.args {
                    for arg in args {
                        self.walk_expr(&arg.expr);
                    }
                }
            }
            Expr::Seq(s) => {
                for e in &s.exprs {
                    self.walk_expr(e);
                }
            }
            Expr::Tpl(t) => {
                for e in &t.exprs {
                    self.walk_expr(e);
                }
            }
            Expr::TaggedTpl(t) => {
                self.walk_expr(&t.tag);
                for e in &t.tpl.exprs {
                    self.walk_expr(e);
                }
            }
            Expr::Paren(p) => self.walk_expr(&p.expr),
            Expr::Await(a) => self.walk_expr(&a.arg),
            Expr::Yield(y) => {
                if let Some(arg) = &y.arg {
                    self.walk_expr(arg);
                }
            }
            _ => {}
        }
    }
}

/// First Function/Arrow scope in the index — used for has-own-binding
/// queries that operate against a function body.
fn find_first_function_scope(scope: &ScopeIndex) -> Option<ScopeId> {
    // The scope index doesn't expose iteration; walk via
    // `parent_of` from the program scope is the wrong direction. The
    // §5.0a integration test only needs to find the first
    // function-or-arrow scope in build-order — which is always
    // assigned a sequential ScopeId starting from 1 (Program is 0).
    // We probe sequentially from id=1 upward, asking `kind_of`.
    let mut id: ScopeId = 1;
    loop {
        // Bounded probe: build cap of 1024 scopes is plenty for the
        // §5.0a corpus (largest fixture = 23 lines ≈ ~5 scopes).
        if id > 1024 {
            return None;
        }
        // Test scope existence by checking parent_of via a sentinel
        // probe. There's no public scope-count getter; use a local
        // helper that's stable across the public API surface: ask
        // for a binding lookup at this id; if `parent_of(id)` is None
        // and id != 0, we've walked off the end.
        match scope.parent_of(id) {
            Some(_) => {
                if matches!(scope.kind_of(id), ScopeKind::Function | ScopeKind::Arrow) {
                    return Some(id);
                }
                id += 1;
            }
            None => return None,
        }
    }
}

fn find_first_arrow_scope(scope: &ScopeIndex) -> Option<ScopeId> {
    let mut id: ScopeId = 1;
    loop {
        if id > 1024 {
            return None;
        }
        match scope.parent_of(id) {
            Some(_) => {
                if matches!(scope.kind_of(id), ScopeKind::Arrow) {
                    return Some(id);
                }
                id += 1;
            }
            None => return None,
        }
    }
}

/// Oracle: traverse the module looking for the first MemberExpression
/// and report its `listKey`. In Babel, when the MemberExpression is
/// inside `CallExpression.arguments`, listKey === "arguments".
fn find_first_member_expr_list_key(module: &Module) -> Option<String> {
    // Walk the module looking for the first MemberExpression. Track
    // whether the immediate parent container is a CallExpression
    // arguments list. Mirrors the oracle's
    // `traverse(...) { MemberExpression(path) { ... } }`'s `listKey`
    // observation, which is "arguments" iff we entered from a
    // call.args slot.
    fn walk_expr(e: &Expr, container_list: Option<&str>) -> Option<String> {
        match e {
            Expr::Member(_) => container_list.map(String::from),
            Expr::Call(c) => {
                use swc_core::ecma::ast::Callee;
                // Visit callee then args. The first MemberExpression
                // we see in args has list_key = "arguments".
                if let Callee::Expr(callee) = &c.callee {
                    if let Some(k) = walk_expr(callee, None) {
                        return Some(k);
                    }
                }
                for arg in &c.args {
                    if let Some(k) = walk_expr(&arg.expr, Some("arguments")) {
                        return Some(k);
                    }
                }
                None
            }
            Expr::New(n) => {
                if let Some(k) = walk_expr(&n.callee, None) {
                    return Some(k);
                }
                if let Some(args) = &n.args {
                    for arg in args {
                        if let Some(k) = walk_expr(&arg.expr, Some("arguments")) {
                            return Some(k);
                        }
                    }
                }
                None
            }
            Expr::Paren(p) => walk_expr(&p.expr, container_list),
            Expr::Bin(b) => walk_expr(&b.left, None).or_else(|| walk_expr(&b.right, None)),
            Expr::Cond(c) => walk_expr(&c.test, None)
                .or_else(|| walk_expr(&c.cons, None))
                .or_else(|| walk_expr(&c.alt, None)),
            Expr::Array(a) => {
                for elem in &a.elems {
                    if let Some(spread_or_expr) = elem {
                        if let Some(k) = walk_expr(&spread_or_expr.expr, Some("elements")) {
                            return Some(k);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    for item in &module.body {
        if let ModuleItem::Stmt(Stmt::Expr(e)) = item {
            if let Some(k) = walk_expr(&e.expr, None) {
                return Some(k);
            }
        }
    }
    None
}

// -------------------- Per-entry assertion --------------------

fn assert_observed_matches_expected(entry: &Entry, observed: &Value) -> Result<(), String> {
    let expected = entry
        .expected
        .as_object()
        .ok_or_else(|| format!("entry `{}`: expected is not an object", entry.label))?;
    let observed = observed
        .as_object()
        .ok_or_else(|| format!("entry `{}`: observed is not an object", entry.label))?;
    for (k, v) in expected {
        let got = observed
            .get(k)
            .ok_or_else(|| format!("missing key `{}` in observed; observed = {:?}", k, observed))?;
        if got != v {
            return Err(format!(
                "key `{}`: observed = {}, expected = {}",
                k, got, v
            ));
        }
    }
    Ok(())
}

// Suppress unused-import warnings in CI for items that some test
// build profiles might mark dead-code (the function-finder helpers
// only fire on certain call_site axes).
#[allow(dead_code)]
fn _force_link(span: Span, _bp: BytePos, _m: &MemberExpr) {
    let _ = span;
}
