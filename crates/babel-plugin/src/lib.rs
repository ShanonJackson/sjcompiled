//! crates/babel-plugin
//! Byte-for-byte port of `packages/babel-plugin/`.
//! See `plugins/PLAN.md` — do not deviate from upstream behaviour.
//!
//! Phase 2 §2.3 status:
//!   * Skeleton (prior session): Compiled-import recognition. Output
//!     stays pass-through.
//!   * §2.3(a) (this checkpoint): JSX-pragma recognition (classic
//!     `import { jsx }` site + `@jsx` / `@jsxImportSource` comment
//!     scan). Recognition only — no AST mutations, no comment store
//!     mutations. State writes are allowed.
//!
//! What's still to land:
//! - §2.3(b): the deferred mutations paired with §2.4 MutationRecorder
//!   — `path.remove()` of the classic-pragma `jsx` specifier;
//!   filtering the matched JSX-pragma comment from the comment store
//!   so `@babel/plugin-transform-react-jsx`'s SWC analog ignores it.
//! - §2.4: state encapsulation + `MutationRecorder::apply` as the only
//!   mutator (per `STATE_MUTATIONS.md` / PLAN.md §3.9.8).
//! - `Program::exit` `appendRuntimeImports` + banner + cleanup loop
//!   (Phase 6, alongside the first real handler).
//! - `ImportDeclaration` specifier removal (§2.4 MutationRecorder).
//! - Per-API stub handlers — placeholder bodies live in
//!   `babel_plugin.rs`.

pub mod babel_plugin;
pub mod cache_schema;
pub mod class_names;
pub mod compat;
pub mod constants;
pub mod css;
pub mod css_map;
pub mod css_prop;
pub mod keyframes;
pub mod mutation_recorder;
pub mod resolver;
pub mod state;
pub mod styled;
pub mod types;
pub mod utils;
pub mod xcss_prop;

use std::sync::Arc;

use serde::Deserialize;
use swc_core::common::comments::Comments;
use swc_core::ecma::ast::Program;
use swc_core::ecma::visit::VisitMutWith;
use swc_core::plugin::metadata::TransformPluginMetadataContextKind;
use swc_core::plugin::plugin_transform;
use swc_core::plugin::proxies::{PluginCommentsProxy, TransformPluginProgramMetadata};

use crate::babel_plugin::BabelPluginVisitor;
use crate::resolver::{build_default, build_from_config, ResolverConfig};
use crate::types::PluginOptions;
use crate::utils::comments::collect_line_comments;

#[plugin_transform]
pub fn process(program: Program, meta: TransformPluginProgramMetadata) -> Program {
    let opts: PluginOptions = meta
        .get_transform_plugin_config()
        .as_deref()
        .and_then(|s| PluginOptions::deserialize(&mut serde_json::Deserializer::from_str(s)).ok())
        .unwrap_or_default();

    // Mirror `babel-plugin-strip-runtime`'s comment-proxy wiring: real
    // proxy in production, fallback unit-struct (no-op outside the
    // plugin runtime). Keeps a single SWC-comment idiom across both
    // plugins.
    let comments: PluginCommentsProxy = meta.comments.clone().unwrap_or(PluginCommentsProxy);

    // §4.6 bridge: SWC exposes the absolute source filename via the
    // metadata context. `resolve_binding.rs` reads
    // `meta.state.filename()` to anchor cross-file resolution; without
    // injection the cross-file branch silently no-ops. Empty string
    // when the host omits the context — `resolve_binding` treats
    // `Some("")` the same as `None` because the upstream JS plugin
    // also bails on missing filename.
    let raw_filename: String = meta
        .get_context(&TransformPluginMetadataContextKind::Filename)
        .unwrap_or_default();

    // WASI sandbox path translation. SWC threads the host-absolute
    // path here; the WASI runtime only grants access to a single
    // preopen mounted at `/cwd` (per
    // `crates/babel-plugin/PHASE0_FINDINGS.md`). Every downstream
    // `std::fs::*` / `oxc_resolver` call has to see the path in
    // `/cwd/<rel>` form or it ENOTCAPABLES out and cross-file
    // resolution silently deopts. Native callers (`run_dispatcher`
    // tests, in-process integration) pass `opts.root = None`, so
    // `host_to_wasi` is a no-op there. See `compat::wasi_path`.
    //
    // No upstream Babel analogue: Node has no equivalent sandbox,
    // so `packages/babel-plugin` has nothing to port from. This is
    // a pure host-environment compat shim per `compat/` discipline.
    let host_root = opts.root.as_deref().unwrap_or("");
    let filename: String = crate::compat::wasi_path::host_to_wasi(&raw_filename, host_root);

    // §4.6 bridge: build the Compiled resolver and stash it on
    // `state` so `resolve_binding::resolve_request` can reach it.
    //
    // Three input shapes (mirrors `PluginOptions::resolver` doc):
    //
    // 1. `resolver: { ... }` — declarative JSON config per
    //    `plugins/RESOLVER_SPEC.md`. Parsed via `ResolverConfig::parse_value`
    //    and built via `build_from_config`. Empty `{}` is a valid
    //    object that yields the §5.4b stock default-config resolver
    //    (parity with Babel's `typeof resolver === 'object'` branch
    //    storing `this.resolver = {}` and never invoking it for
    //    inputs that don't need cross-file resolution).
    // 2. `resolver: "..."` (string) / unsupported shape — Babel
    //    `require()`s the module; the WASI plugin can't load JS
    //    (PLAN.md §1 constraint 1). Fall back to `build_default`.
    //    The host wrapper is documented as the strip-point for
    //    string-form resolver values, but we don't hard-fail here
    //    because some pipelines (incl. the parity harness) still
    //    pass the raw config through.
    // 3. Absent / `null` — `build_default`.
    //
    // If `ResolverConfig::parse_value` errors on a malformed object
    // (unknown field, type mismatch, etc.), fall back to
    // `build_default` rather than poisoning the whole plugin
    // invocation. Surfacing the schema error to the host requires a
    // diagnostics channel we don't yet have at the plugin boundary.
    let resolver = match opts.resolver.as_ref() {
        Some(v) => match ResolverConfig::parse_value(v) {
            Ok(Some(cfg)) => {
                // Resolve relative `fromFile` paths in `preferFirst`
                // against `opts.root` if present, else the filename's
                // dir, else `/` (the same fallback `set_filename`
                // already uses for the cross-file resolver anchor).
                //
                // Translate the host root through the same `/cwd`
                // mount used for `filename` above, so the resolver
                // walks paths visible to the WASI preopen. Native
                // callers pass `host_root = ""` and `host_to_wasi` is
                // a no-op.
                let config_dir: std::path::PathBuf = opts
                    .root
                    .as_deref()
                    .map(|r| {
                        std::path::PathBuf::from(crate::compat::wasi_path::host_to_wasi(
                            r, host_root,
                        ))
                    })
                    .or_else(|| {
                        std::path::Path::new(&filename)
                            .parent()
                            .map(std::path::Path::to_path_buf)
                    })
                    .unwrap_or_else(|| std::path::PathBuf::from("/"));
                match build_from_config(&cfg, &config_dir) {
                    Ok(r) => Arc::new(r),
                    Err(_) => Arc::new(build_default(opts.extensions.as_deref())),
                }
            }
            // String / unsupported / null / parse error → default.
            _ => Arc::new(build_default(opts.extensions.as_deref())),
        },
        None => Arc::new(build_default(opts.extensions.as_deref())),
    };

    let mut visitor = BabelPluginVisitor::new(opts, comments);
    if !filename.is_empty() {
        visitor.state.set_filename(filename);
    }
    visitor.state.set_resolver(resolver);
    // §6.8i — bridge SWC's `unresolved_mark` from plugin metadata into
    // the visitor so the `Program::exit` React-import injection can
    // colour its local Ident with the same hygiene context downstream
    // free references (e.g. the react-classic JSX transform's
    // `React.createElement(...)` Idents) carry. Without this, fixtures
    // with no top-level user bindings fall back to an empty
    // `SyntaxContext` and SWC's hygiene pass renames our import to
    // `React1`. See `babel_plugin.rs::build_react_namespace_import`.
    visitor.unresolved_mark = Some(meta.unresolved_mark);

    // §6.5 bridge: walk the program once with `meta.source_map`
    // (`PluginSourceMapProxy`) + `meta.comments` to build the
    // line-indexed comment store + span→line index. The css-prop
    // disable-directive gate (`is_css_prop_disabled`) reads from
    // both; without them, upstream's `getNodeComments` per-line
    // filter has nothing to match against. See `utils/comments.rs`
    // module doc and the §6.5 closure note in `plugins/STATUS.md`.
    let mut p = program;
    // Babel-parser parity: SWC parser preserves CRLF in
    // `TplElement.raw`; Babel's parser normalises CR/CRLF → LF per
    // ECMAScript §12.8.6 TRV rules. Keyframes naming and every other
    // raw-quasi consumer hashes the byte sequence, so a CR/LF
    // mismatch flips class names on a CRLF source checkout. One-shot
    // pre-pass aligns the SWC AST to Babel's shape before any
    // visitor runs. See `crates/babel-plugin/src/compat/template_literal_raw.rs`.
    crate::compat::template_literal_raw::normalize_template_literal_raw(&mut p);
    let line_index = collect_line_comments(&p, &visitor.comments, &meta.source_map);
    visitor.state.set_comment_lines(line_index.comments);
    visitor.state.set_span_lines(line_index.spans);

    // Install the ambient comments handle for `compat::generator::generate`.
    // Babel's @babel/generator reads `node.leadingComments` /
    // `trailingComments` directly off the AST; SWC's parser stores
    // them out-of-band. Without an ambient handle, every
    // `generate(&Expr)` call site drops comments, and hash inputs
    // computed from `generate(node).code` (CSS-variable / class-name
    // hashing in `utils/css_builders.rs`) diverge whenever the
    // hashed expression carries inline comments — see
    // `fixtures/ct-styled-token-nested-ternary` for a reproduction.
    //
    // SAFETY: `visitor.comments` (a `PluginCommentsProxy`) lives for
    // the entirety of the `visit_mut_with` call, and the cleanup
    // call below clears the thread-local before the function returns.
    crate::compat::generator::set_ambient_comments(&visitor.comments);
    p.visit_mut_with(&mut visitor);
    crate::compat::generator::clear_ambient_comments();
    p
}

/// In-process entry for workspace integration tests. Drives the
/// dispatcher without going through the SWC plugin transport so we
/// can inspect `state` after the run.
///
/// Generic over `C: Comments` — tests typically pass
/// `swc_common::comments::SingleThreadedComments::default()` (an
/// in-process empty store) so the dispatcher's pragma scan reads
/// safely without the SWC plugin runtime's thread-locals being
/// initialised. Production paths go through `process` above.
pub fn run_dispatcher<C: Comments>(
    program: &mut Program,
    opts: PluginOptions,
    comments: C,
) -> BabelPluginVisitor<C> {
    let mut visitor = BabelPluginVisitor::new(opts, comments);
    program.visit_mut_with(&mut visitor);
    visitor
}
