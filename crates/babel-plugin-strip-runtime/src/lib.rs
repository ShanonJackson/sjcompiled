//! crates/babel-plugin-strip-runtime
//! Byte-for-byte port of `packages/babel-plugin-strip-runtime/`.
//! See `plugins/PLAN.md` — do not deviate from upstream behaviour.
//!
//! Phase 1 §1.4 status: dispatcher visitor implemented (CC/CS removal,
//! ImportSpecifier filter, `styleSheetPath` require injection, scope
//! cleanup). The two filesystem-side outputs — `compiledRequireExclude`
//! sidecar JSON and `extractStylesToDirectory` `.compiled.css` writes
//! — are §1.5 work.

pub mod compat;
pub mod utils;

use serde::Deserialize;
use swc_core::common::comments::Comments;
use swc_core::common::errors::HANDLER;
use swc_core::common::{BytePos, Spanned, Span, SyntaxContext, DUMMY_SP};
use swc_core::ecma::ast::Expr;
use swc_core::ecma::ast::{
    CallExpr, Callee, ExprOrSpread, ExprStmt, Ident, ImportDecl, ImportPhase, ImportSpecifier,
    JSXElement, JSXElementChild, JSXElementName, JSXExpr, Lit, ModuleDecl, ModuleExportName,
    ModuleItem, Program, Prop, PropName, PropOrSpread, Stmt, Str,
};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};
use swc_core::plugin::metadata::TransformPluginMetadataContextKind;
use swc_core::plugin::plugin_transform;
use swc_core::plugin::proxies::{PluginCommentsProxy, TransformPluginProgramMetadata};

use crate::compat::scope::ModuleScope;
use crate::utils::is_automatic_runtime::{is_automatic_runtime, JsxFunc};
use crate::utils::is_cc_component::is_cc_component;
use crate::utils::is_create_element::is_create_element;
use crate::utils::remove_style_declarations::remove_style_declarations;
use crate::utils::to_uri_component::to_uri_component;

/// Plugin options. Shape matches `PluginOptions` from
/// `packages/babel-plugin-strip-runtime/src/types.ts`. Field names use
/// camelCase on the wire (Babel/JS convention).
///
/// `call_scratch` and `source_file_name` extend the JS shape — they
/// reflect plugin-config inputs the host wraps around the existing
/// options (PLAN.md §3.9.6 + §7). Babel reads `file.opts.generatorOpts.sourceFileName`
/// directly off the AST file; SWC has no equivalent, so the host
/// threads it in.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginOptions {
    #[serde(default)]
    pub style_sheet_path: Option<String>,
    #[serde(default)]
    pub compiled_require_exclude: bool,
    #[serde(default)]
    pub extract_styles_to_directory: Option<ExtractToDirOpts>,
    #[serde(default)]
    pub sort_at_rules: Option<bool>,
    #[serde(default)]
    pub sort_shorthand: Option<bool>,
    /// Per-call scratch directory for sidecars. Host-translated
    /// `/cwd/<rel>`-prefixed path. Plugin writes `style-rules.json`
    /// here when `compiled_require_exclude=true`.
    #[serde(default)]
    pub call_scratch: Option<String>,
    /// Babel reads this from `file.opts.generatorOpts.sourceFileName`
    /// at the `extractStylesToDirectory` site. SWC has no metadata
    /// channel; the host threads it through plugin config.
    #[serde(default)]
    pub source_file_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractToDirOpts {
    pub source: String,
    pub dest: String,
}

/// `<callScratch>/style-rules.json` writer.
///
/// Schema source of truth: `plugins/SIDECAR_SCHEMA.md` §2 (locked in
/// Phase 1 §1.6). Any change to field names, types, or version MUST
/// update that doc in the same commit; the JS host parser reads from
/// the same spec. See PLAN.md §7 for the cross-plugin overview.
#[derive(Debug, serde::Serialize)]
struct StyleRulesSidecar<'a> {
    /// Hard-coded `1`. A bump is a coordinated plugin/host release;
    /// see SIDECAR_SCHEMA.md "Versioning policy".
    version: u32,
    /// Atomic CSS rule strings, in visitor accumulation order.
    rules: &'a [String],
}

/// Visitor state. Mirrors Babel's `PluginPass`:
/// `style_rules` is the per-file accumulator the upstream `pre()`
/// hook initialises to `[]`.
struct StripRuntimeVisitor {
    scope: ModuleScope,
    style_rules: Vec<String>,
    /// SWC stores comments in a side-channel keyed by BytePos. Babel
    /// stores them on the AST node itself; the upstream plugin clears
    /// `path.node.leadingComments = null` before replacing CC-wrapped
    /// nodes so the inner-node's `/*#__PURE__*/` doesn't get stacked
    /// with the outer's. We do the analogue here: `take_leading` on
    /// the outer span before swapping in the inner expression.
    comments: PluginCommentsProxy,
}

impl StripRuntimeVisitor {
    fn drop_leading_comments_at(&self, pos: BytePos) {
        let _ = self.comments.take_leading(pos);
    }
}

impl StripRuntimeVisitor {
    /// `<CC><CS>{[...]}</CS><userland /></CC>` → `<userland />`.
    /// Children layout (Babel destructure `[, compiledStyles, , nodeToReplace]`):
    /// 0 = whitespace JSXText
    /// 1 = `<CS>{[...]}</CS>`
    /// 2 = whitespace JSXText
    /// 3 = userland JSX or expression container
    fn try_replace_cc_jsx(&mut self, jsx: &JSXElement) -> Option<Expr> {
        let JSXElementName::Ident(id) = &jsx.opening.name else {
            return None;
        };
        if id.sym != *"CC" {
            return None;
        }

        if let Some(JSXElementChild::JSXElement(cs_jsx)) = jsx.children.get(1) {
            let cs_expr = Expr::JSXElement(cs_jsx.clone());
            remove_style_declarations(&cs_expr, &mut self.scope, &mut self.style_rules);
        }

        let third = jsx.children.get(3)?;
        match third {
            JSXElementChild::JSXExprContainer(c) => match &c.expr {
                JSXExpr::Expr(e) => Some((**e).clone()),
                _ => None,
            },
            JSXElementChild::JSXElement(e) => Some(Expr::JSXElement(e.clone())),
            JSXElementChild::JSXFragment(f) => Some(Expr::JSXFragment(f.clone())),
            _ => None,
        }
    }

    /// `React.createElement(CC, ..., compiledStyles, nodeToReplace)` → `nodeToReplace`,
    /// or `_jsxs(CC, { children: [compiledStyles, nodeToReplace] })` → `nodeToReplace`.
    fn try_replace_cc_call(&mut self, call: &CallExpr) -> Option<Expr> {
        // ── classic: React.createElement(CC, ..., compiledStyles, nodeToReplace) ──
        if let Callee::Expr(callee) = &call.callee {
            if is_create_element(callee.as_ref()) {
                let component = call.args.first()?.expr.as_ref();
                if !is_cc_component(component) {
                    return None;
                }
                if let Some(s) = call.args.get(2) {
                    if s.spread.is_none() {
                        remove_style_declarations(
                            s.expr.as_ref(),
                            &mut self.scope,
                            &mut self.style_rules,
                        );
                    }
                }
                let node_to_replace = call.args.get(3)?;
                if node_to_replace.spread.is_some() {
                    return None;
                }
                return Some((*node_to_replace.expr).clone());
            }
        }

        // ── automatic: _jsxs(CC, { children: [compiledStyles, nodeToReplace] }) ──
        let outer = Expr::Call(call.clone());
        if is_automatic_runtime(&outer, JsxFunc::Jsxs) {
            let component = call.args.first()?.expr.as_ref();
            if !is_cc_component(component) {
                return None;
            }
            let props = call.args.get(1)?.expr.as_ref();
            let Expr::Object(obj) = props else {
                return None;
            };
            let children_value: Option<&Expr> = obj.props.iter().find_map(|p| {
                if let PropOrSpread::Prop(prop) = p {
                    if let Prop::KeyValue(kv) = prop.as_ref() {
                        let key_name = match &kv.key {
                            PropName::Ident(id) => Some(id.sym.as_ref()),
                            _ => None,
                        };
                        if key_name == Some("children") {
                            return Some(kv.value.as_ref());
                        }
                    }
                }
                None
            });
            let Some(Expr::Array(arr)) = children_value else {
                return None;
            };
            let compiled_styles = arr.elems.first().and_then(|e| e.as_ref())?;
            let node_to_replace = arr.elems.get(1).and_then(|e| e.as_ref())?;
            if compiled_styles.spread.is_some() || node_to_replace.spread.is_some() {
                return None;
            }
            remove_style_declarations(
                compiled_styles.expr.as_ref(),
                &mut self.scope,
                &mut self.style_rules,
            );
            return Some((*node_to_replace.expr).clone());
        }

        None
    }
}

impl VisitMut for StripRuntimeVisitor {
    fn visit_mut_expr(&mut self, expr: &mut Expr) {
        // Visit children first so nested CC/CS sites are stripped
        // before the outer one needs to inspect them.
        expr.visit_mut_children_with(self);

        // Capture the outer span's start BEFORE we mutate. If we
        // recognise this as a CC-wrapped node we'll drop its leading
        // comments (mirrors Babel's `path.node.leadingComments = null`).
        let outer_pos = match expr {
            Expr::JSXElement(jsx) => Some(jsx.span.lo),
            Expr::Call(call) => Some(call.span.lo),
            _ => None,
        };

        let replacement = match expr {
            Expr::JSXElement(jsx) => self.try_replace_cc_jsx(jsx),
            Expr::Call(call) => self.try_replace_cc_call(call),
            _ => None,
        };
        if let Some(new) = replacement {
            if let Some(pos) = outer_pos {
                self.drop_leading_comments_at(pos);
            }
            *expr = new;
        }
    }

    fn visit_mut_module_decl(&mut self, decl: &mut ModuleDecl) {
        decl.visit_mut_children_with(self);

        // Drop `CC` / `CS` named-import specifiers. The parent
        // ImportDeclaration is preserved even if its specifier list
        // becomes empty — upstream's `path.remove()` on the specifier
        // does not propagate up.
        if let ModuleDecl::Import(import) = decl {
            import.specifiers.retain(|s| match s {
                ImportSpecifier::Named(n) => {
                    let imported_name = match &n.imported {
                        Some(ModuleExportName::Ident(id)) => id.sym.as_ref().to_string(),
                        Some(ModuleExportName::Str(s)) => {
                            s.value.to_atom_lossy().as_str().to_string()
                        }
                        None => n.local.sym.as_ref().to_string(),
                    };
                    !matches!(imported_name.as_str(), "CC" | "CS")
                }
                _ => true,
            });
        }
    }
}

/// `extractStylesToDirectory.dest` must resolve inside the WASI `/cwd`
/// preopen. Babel has no such constraint — its `path.join(cwd, dest,
/// ...)` happily lands at host-absolute paths. The SWC plugin runs
/// behind cap-std; an out-of-preopen write fails with a cryptic EACCES
/// at the syscall level. Validate up-front for a clear error.
///
/// Rejects: absolute paths (`/dist`, `C:\dist`), `..` escape segments.
/// Accepts: relative paths (`dist/`, `dist/css/`, `./dist`).
fn validate_dest_under_cwd(dest: &str) -> Result<(), String> {
    if dest.is_empty() {
        return Err("extractStylesToDirectory.dest must not be empty".to_string());
    }
    let normalised = dest.replace('\\', "/");
    if normalised.starts_with('/') {
        return Err(format!(
            "extractStylesToDirectory.dest must be relative to cwd, got absolute path '{}'",
            dest
        ));
    }
    // Windows-style drive-letter prefix (`C:`, `D:`, ...).
    let bytes = normalised.as_bytes();
    if bytes.len() >= 2
        && bytes[1] == b':'
        && bytes[0].is_ascii_alphabetic()
    {
        return Err(format!(
            "extractStylesToDirectory.dest must be relative to cwd, got drive-prefixed path '{}'",
            dest
        ));
    }
    for segment in normalised.split('/') {
        if segment == ".." {
            return Err(format!(
                "extractStylesToDirectory.dest must not escape cwd via '..', got '{}'",
                dest
            ));
        }
    }
    Ok(())
}

/// Index of the first body item that ISN'T a directive prologue (a
/// bare string-literal `ExprStmt` at the start of the module). Babel's
/// `unshiftContainer` skips directives natively; SWC has no separate
/// Directive list so we mirror the behaviour here.
fn first_non_directive_index(body: &[ModuleItem]) -> usize {
    let mut i = 0;
    for item in body {
        let ModuleItem::Stmt(Stmt::Expr(es)) = item else {
            break;
        };
        let Expr::Lit(Lit::Str(_)) = es.expr.as_ref() else {
            break;
        };
        i += 1;
    }
    i
}

/// JS `path.parse(filename).name` — filename without extension.
/// `'/base/src/app.tsx'` → `'app'`. `'app'` → `'app'`. `'.gitignore'` →
/// `''`. Mirrors Node's `path.parse` rules: name = basename minus the
/// final dot-segment (if any), but a leading dot doesn't count as an
/// extension separator.
fn parse_name(filename: &str) -> &str {
    // Use forward slash only — both Babel and SWC normalise on /.
    // Take basename.
    let basename = match filename.rsplit_once('/') {
        Some((_, tail)) => tail,
        None => filename,
    };
    // path.parse('.foo') → { name: '.foo', ext: '' }: a leading dot
    // is part of the name, not an extension separator.
    if basename.starts_with('.') {
        if let Some(rest) = basename.get(1..) {
            if let Some(dot) = rest.rfind('.') {
                return &basename[..1 + dot];
            }
        }
        return basename;
    }
    match basename.rsplit_once('.') {
        Some((name, _ext)) => name,
        None => basename,
    }
}

/// JS `path.dirname(p)`. Empty string for '.', single-segment, or empty input.
fn dirname(p: &str) -> &str {
    match p.rsplit_once('/') {
        Some((head, _)) => head,
        None => "",
    }
}

/// `path.join(a, b, c, ...)` semantics — concatenate with single `/`,
/// drop empty segments, never produce `//` runs. Trailing slashes on
/// inputs are dropped; leading on the first input is preserved.
fn path_join(parts: &[&str]) -> String {
    let mut out = String::new();
    for &p in parts {
        if p.is_empty() {
            continue;
        }
        let trimmed = p.trim_end_matches('/');
        // Special-case: keep leading slash from the very first segment.
        if out.is_empty() {
            out.push_str(trimmed);
            continue;
        }
        if !out.ends_with('/') {
            out.push('/');
        }
        out.push_str(trimmed.trim_start_matches('/'));
    }
    out
}

/// Build `import "./<spec>";` (side-effect-only, no specifiers) as a
/// `ModuleItem`. Mirrors Babel's
/// `t.importDeclaration([], t.stringLiteral(spec))`.
///
/// When `attach_comments_at` is `Some(span)`, the import's outer span
/// is set to that span — same trick as `make_require_stmt` to route
/// the displaced first-statement's leading comments onto the injected
/// import. Babel's `unshiftContainer` triggers comment relocation
/// natively; SWC's codegen emits comments off `BytePos`, so we have
/// to re-anchor them manually.
fn make_side_effect_import(spec: &str, attach_comments_at: Option<Span>) -> ModuleItem {
    let span = attach_comments_at.unwrap_or(DUMMY_SP);
    ModuleItem::ModuleDecl(ModuleDecl::Import(ImportDecl {
        span,
        specifiers: Vec::new(),
        src: Box::new(Str {
            span: DUMMY_SP,
            value: spec.into(),
            raw: None,
        }),
        type_only: false,
        with: None,
        phase: ImportPhase::Evaluation,
    }))
}

/// Build `require("<url>");` as a `ModuleItem`. The outer span uses
/// `attach_comments_at` (or `DUMMY_SP` if `None`) so we can route the
/// file-level leading comment to the first injected require —
/// otherwise the comment would stay anchored to the original first
/// body item and end up BELOW the new requires.
fn make_require_stmt(url: &str, attach_comments_at: Option<swc_core::common::Span>) -> ModuleItem {
    let span = attach_comments_at.unwrap_or(DUMMY_SP);
    ModuleItem::Stmt(Stmt::Expr(ExprStmt {
        span,
        expr: Box::new(Expr::Call(CallExpr {
            span,
            ctxt: SyntaxContext::empty(),
            callee: Callee::Expr(Box::new(Expr::Ident(Ident::new(
                "require".into(),
                DUMMY_SP,
                SyntaxContext::empty(),
            )))),
            args: vec![ExprOrSpread {
                spread: None,
                expr: Box::new(Expr::Lit(Lit::Str(Str {
                    span: DUMMY_SP,
                    value: url.into(),
                    raw: None,
                }))),
            }],
            type_args: None,
        })),
    }))
}

#[plugin_transform]
pub fn process(program: Program, meta: TransformPluginProgramMetadata) -> Program {
    let opts: PluginOptions = meta
        .get_transform_plugin_config()
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    // Babel reads the filename off `state.filename`; SWC exposes it
    // through the metadata context. Both default to a synthetic path
    // when the host omits one, which the upstream `index.ts` rejects
    // for `extractStylesToDirectory` via the explicit-filename check.
    let filename: String = meta
        .get_context(&TransformPluginMetadataContextKind::Filename)
        .unwrap_or_default();

    // PLAN.md §3 step 6: validate `extractStylesToDirectory.dest` is
    // inside the WASI preopen at plugin entry. Failing fast here gives
    // a readable error; deferring to the cap-std write would surface
    // as opaque EACCES/ENOENT mid-extract.
    if let Some(extract) = &opts.extract_styles_to_directory {
        if let Err(msg) = validate_dest_under_cwd(&extract.dest) {
            panic!("babel-plugin-strip-runtime: {}", msg);
        }
    }

    let Program::Module(mut module) = program else {
        // Strip-runtime is module-only; scripts pass through.
        return program;
    };

    let scope = ModuleScope::from_module(&module);
    let mut visitor = StripRuntimeVisitor {
        scope,
        style_rules: Vec::new(),
        comments: meta.comments.clone().unwrap_or(PluginCommentsProxy),
    };

    module.visit_mut_with(&mut visitor);

    let StripRuntimeVisitor {
        scope,
        style_rules,
        comments: _,
    } = visitor;

    // ── Program::exit ordering ──
    //
    // Upstream:
    //   1. compiledRequireExclude → write to file.metadata, return early.
    //   2. styleSheetPath → preserveLeadingComments + unshift `require(...)` per rule.
    //   3. extractStylesToDirectory → write file + unshift `import './x.compiled.css'`.
    //
    // The contract is "never two." Per `plugins/STATUS.md` §1.4 lock.

    // Apply scope removals BEFORE any body-prepend mutation. The
    // scope's BindingLocations are indexed against `module.body`
    // pre-mutation; injecting requires up front would shift those
    // indices and `apply_removals` would clip the wrong declarators.
    scope.apply_removals(&mut module);

    if opts.compiled_require_exclude {
        // <callScratch>/style-rules.json. Schema: SIDECAR_SCHEMA.md §2
        // (PLAN.md §7 cross-reference). Mirrors `file.metadata.styleRules`
        // Babel writes — the host re-exposes as `result.styleRules` and
        // Parcel assigns to `asset.meta.styleRules` (§3.4 cross-mapping).
        //
        // Only write when there's something to write. Babel's analogue:
        // `if (!file.metadata?.styleRules) file.metadata.styleRules = []; this.styleRules.forEach(push)`.
        // An empty `styleRules` writes no sidecar (matches PLAN.md §7's
        // "if compiledRequireExclude=true and styleRules non-empty").
        if !style_rules.is_empty() {
            if let Some(scratch) = opts.call_scratch.as_deref() {
                let path = format!("{}/style-rules.json", scratch.trim_end_matches('/'));
                let payload = StyleRulesSidecar {
                    version: 1,
                    rules: &style_rules,
                };
                // serde_json::to_string is byte-deterministic for our
                // shape; failures here mean a runtime corruption we'd
                // rather surface than swallow.
                let json = serde_json::to_string(&payload)
                    .expect("style-rules.json serialization failed");
                if let Err(e) = std::fs::write(&path, json) {
                    panic!(
                        "babel-plugin-strip-runtime: failed to write sidecar {}: {}",
                        path, e
                    );
                }
            }
            // No callScratch provided: silently skip (matches "in-process
            // tests can omit the host wrapper" path; production wiring
            // always sets it).
        }
    } else if let Some(style_sheet_path) = &opts.style_sheet_path {
        // Insert AFTER any leading directives (`'use strict';` etc).
        // Babel's `path.unshiftContainer('body', require)` knows to
        // skip directives; in SWC there's no Module.directives field,
        // so we have to find the boundary manually.
        let insert_at = first_non_directive_index(&module.body);

        // Mirror Babel's `preserveLeadingComments`: route whatever
        // leading comments sit on the body item we're about to
        // displace onto the FIRST injected require by giving it the
        // same span.lo. SWC's codegen takes leading comments at that
        // BytePos when emitting the require, so the file's banner
        // comment ends up ABOVE the require chain (matching Babel)
        // instead of being pinned to whatever statement now sits at
        // position N+1.
        let banner_span = module.body.get(insert_at).map(|item| match item {
            ModuleItem::Stmt(s) => s.span(),
            ModuleItem::ModuleDecl(m) => m.span(),
        });

        // Babel calls `unshiftContainer('body', require)` once per
        // rule, in iteration order. Each unshift goes to the FRONT,
        // so the final order is REVERSED relative to iteration.
        let mut requires: Vec<ModuleItem> = Vec::with_capacity(style_rules.len());
        for rule in &style_rules {
            let params = to_uri_component(rule);
            let url = format!("{}?style={}", style_sheet_path, params);
            requires.push(make_require_stmt(&url, None));
        }
        requires.reverse();
        if let (Some(first), Some(span)) = (requires.first_mut(), banner_span) {
            if let ModuleItem::Stmt(Stmt::Expr(ref mut es)) = first {
                es.span = span;
                if let Expr::Call(ref mut call) = *es.expr {
                    call.span = span;
                }
            }
        }
        // Splice `requires` into module.body at position `insert_at`.
        let tail: Vec<ModuleItem> = module.body.drain(insert_at..).collect();
        module.body.extend(requires);
        module.body.extend(tail);
    } else if let Some(extract) = opts.extract_styles_to_directory.as_ref() {
        if !style_rules.is_empty() {
            // Mirror packages/babel-plugin-strip-runtime/src/index.ts:61-101.
            // Babel uses the full host filename to derive the basename;
            // we do the same off the SWC filename context. The upstream
            // `Source filename was not defined` throw fires when
            // generatorOpts.sourceFileName is unset; SWC's analogue is
            // host-threaded `source_file_name` plugin config.
            let basename = parse_name(&filename);
            let css_filename = format!("{}.compiled.css", basename);

            let source_file_name = match opts.source_file_name.as_deref() {
                Some(s) => s,
                None => {
                    HANDLER.with(|h| {
                        h.struct_span_err(DUMMY_SP, "Source filename was not defined")
                            .emit();
                    });
                    return Program::Module(module);
                }
            };

            // Babel: `if (!sourceFileName.includes(opts.source)) throw`.
            // We surface the same message via SWC's HANDLER so it
            // propagates through the plugin runner as a clean diagnostic.
            // (A raw panic gets wrapped to the opaque "failed to invoke
            // plugin" string and the original message is lost.)
            let source_seg = &extract.source;
            let Some(idx) = source_file_name.find(source_seg.as_str()) else {
                HANDLER.with(|h| {
                    h.struct_span_err(
                        DUMMY_SP,
                        &format!(
                            "{}: Source directory '{}' was not found relative to source file ('{}')",
                            filename, source_seg, source_file_name
                        ),
                    )
                    .emit();
                });
                return Program::Module(module);
            };

            // `relativePath = sourceFileName.slice(idx + source.length)`.
            let relative_path = &source_file_name[idx + source_seg.len()..];

            // `cssFilePath = join(cwd, dest, dirname(relativePath), cssFilename)`.
            // Inside the WASI sandbox `cwd` is `/cwd`; everything must
            // resolve under that preopen.
            let css_file_path = path_join(&[
                "/cwd",
                &extract.dest,
                dirname(relative_path),
                &css_filename,
            ]);

            // `mkdirSync(dirname(cssFilePath), { recursive: true })`.
            let parent = dirname(&css_file_path);
            if !parent.is_empty() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    panic!(
                        "babel-plugin-strip-runtime: failed to mkdir {}: {}",
                        parent, e
                    );
                }
            }

            // `writeFileSync(cssFilePath, sort(styleRules.sort().join('\n'), sortConfig))`.
            //
            // The byte-correct CSS sort lives in `crates/css::sort::sort`
            // (Phase 4 wires the full CSS Rust port into the babel-plugin).
            // For §1.5 we mirror the JS-level pre-sort
            // (`styleRules.sort().join('\n')`) so the file is non-empty
            // and rule-set complete; the postcss-level
            // `sort(stylesheet, sortConfig)` will land alongside the
            // Phase 4 babel-plugin CSS work.
            //
            // The harness gate for §1.5 is JS-side parity (the
            // `import './<file>.compiled.css'` injection); .compiled.css
            // contents are not yet diffed against Babel.
            let mut sorted = style_rules.clone();
            sorted.sort();
            let css_body = sorted.join("\n");

            if let Err(e) = std::fs::write(&css_file_path, css_body) {
                panic!(
                    "babel-plugin-strip-runtime: failed to write {}: {}",
                    css_file_path, e
                );
            }

            // `path.unshiftContainer('body', t.importDeclaration([], t.stringLiteral('./'+cssFilename)))`.
            //
            // Same banner-routing trick as the styleSheetPath branch —
            // adopt the displaced first-statement's outer span so its
            // leading comments emit ABOVE the injected import.
            let banner_span = module.body.first().map(|item| match item {
                ModuleItem::Stmt(s) => s.span(),
                ModuleItem::ModuleDecl(m) => m.span(),
            });
            let spec = format!("./{}", css_filename);
            let import_item = make_side_effect_import(&spec, banner_span);
            module.body.insert(0, import_item);
        }
    }

    Program::Module(module)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_name_tsx_basename() {
        assert_eq!(parse_name("/base/src/app.tsx"), "app");
    }

    #[test]
    fn parse_name_no_extension() {
        assert_eq!(parse_name("/base/src/app"), "app");
        assert_eq!(parse_name("app"), "app");
    }

    #[test]
    fn parse_name_double_extension() {
        // path.parse('foo.test.tsx').name → 'foo.test'.
        assert_eq!(parse_name("/x/foo.test.tsx"), "foo.test");
    }

    #[test]
    fn parse_name_dotfile() {
        // path.parse('.gitignore').name → '.gitignore'.
        assert_eq!(parse_name(".gitignore"), ".gitignore");
        // path.parse('.foo.bar').name → '.foo'.
        assert_eq!(parse_name(".foo.bar"), ".foo");
    }

    #[test]
    fn dirname_strips_basename() {
        assert_eq!(dirname("/base/src/app.tsx"), "/base/src");
        assert_eq!(dirname("app.tsx"), "");
        assert_eq!(dirname("/x"), "");
    }

    #[test]
    fn path_join_basic() {
        assert_eq!(path_join(&["/cwd", "dist", "app.compiled.css"]), "/cwd/dist/app.compiled.css");
        assert_eq!(path_join(&["/cwd", "dist/", "", "app.compiled.css"]), "/cwd/dist/app.compiled.css");
        assert_eq!(path_join(&["/cwd", "dist/", "/leading", "app.compiled.css"]), "/cwd/dist/leading/app.compiled.css");
    }

    #[test]
    fn validate_dest_accepts_relative() {
        assert!(validate_dest_under_cwd("dist/").is_ok());
        assert!(validate_dest_under_cwd("dist").is_ok());
        assert!(validate_dest_under_cwd("./dist").is_ok());
        assert!(validate_dest_under_cwd("dist/css").is_ok());
    }

    #[test]
    fn validate_dest_rejects_absolute_unix() {
        assert!(validate_dest_under_cwd("/dist").is_err());
    }

    #[test]
    fn validate_dest_rejects_absolute_windows() {
        assert!(validate_dest_under_cwd("C:\\dist").is_err());
        assert!(validate_dest_under_cwd("D:/dist").is_err());
    }

    #[test]
    fn validate_dest_rejects_dotdot_escape() {
        assert!(validate_dest_under_cwd("../dist").is_err());
        assert!(validate_dest_under_cwd("dist/../etc").is_err());
    }

    #[test]
    fn validate_dest_rejects_empty() {
        assert!(validate_dest_under_cwd("").is_err());
    }
}
