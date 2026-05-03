# Plugin Implementation Guide

Read this **before** opening any plugin file. It tells you the contract,
the AST surface you have to work with, the helpers you must use (not roll
your own), and the parity rules that are non-negotiable.

> **The stakes.** This code runs against a 60-90 GB monorepo with ~10M LOC.
> Every plugin's output bytes are part of a hash that becomes a class name
> in production. One byte of drift renames every class in a consumer's
> build. Re-read `crates/PARITY_VERSIONS.md` if that doesn't feel real yet.

---

## 1. Where your plugin lives

Each plugin maps 1:1 from a TypeScript file to a Rust file:

| Upstream | Rust |
|---|---|
| `packages/css/src/plugins/atomicify-rules.ts` | `crates/compiled-css/src/plugins/atomicify_rules.rs` |
| `packages/css/src/plugins/discard-empty-rules.ts` | `crates/compiled-css/src/plugins/discard_empty_rules.rs` |
| `packages/css/src/plugins/expand-shorthands/margin.ts` | `crates/compiled-css/src/plugins/expand_shorthands/margin.rs` |
| ...etc | ...etc |

**Do not move files. Do not invent new module names.** The 1:1 layout is how
maintainers compare old and new line-by-line during code review. If your
plugin needs a new helper, put it in the same module unless it's genuinely
shared with another plugin (in which case `compiled-css/src/utils/`).

The orchestrator that splices your plugin into the pipeline lives at
`crates/css/src/transform.rs`. **Don't edit `transform.rs` until your
plugin's tests pass standalone.**

---

## 2. The contract

Every plugin's public entry has this shape:

```rust
use postcss_core::{PluginResult, Root};

pub fn my_plugin(root: &mut Root, opts: &MyOpts) -> PluginResult {
    // walk root, mutate in place, return Ok(()).
    Ok(())
}
```

Some plugins also accept a callback or a mutable output collector (e.g.
`atomicifyRules` collects class names, `extractStyleSheets` collects
sheets). Mirror upstream's exact callback signature in `MyOpts`:

```rust
#[derive(Debug, Clone, Default)]
pub struct AtomicifyRulesOpts {
    pub class_name_compression_map: Option<IndexMap<String, String>>,
    pub class_hash_prefix: Option<String>,
    /// Push class names here in order of generation.
    pub class_names: Vec<String>,
}
```

The struct field names use snake_case (`class_hash_prefix`); the option
*shape* (Optional vs default-true vs default-false) must match upstream's
TypeScript exactly. If upstream's `flattenMultipleSelectors` defaults to
`true` when undefined, your `Option<bool>` resolution path must do the
same.

---

## 3. The AST you walk

### postcss-core (rules / decls / at-rules / comments)

```rust
use postcss_core::{Node, NodeKind, container};
use postcss_core::container::{Mutation, Visit, WalkCtx};

container::walk_decls_mut(&mut root.root, &mut |node, _ctx| {
    if let NodeKind::Declaration(d) = &mut node.kind {
        if d.prop == "color" && d.value == "transparent" {
            return Mutation::Remove;
        }
    }
    Mutation::Keep
});
```

#### Walking (read-only)

- `each(parent, |node, idx| -> Visit)` — direct children only.
- `walk(parent, |node| -> Visit)` — every descendant.
- `walk_decls`, `walk_rules`, `walk_at_rules`, `walk_comments` — filtered.

`Visit::Continue` keeps walking, `Visit::Stop` aborts, `Visit::SkipChildren`
descends past this node's subtree.

#### Mutating

- `each_mut`, `walk_mut`, `walk_decls_mut`, `walk_rules_mut`,
  `walk_at_rules_mut`, `walk_comments_mut` — receive `WalkCtx { index,
  parent_len }`, return `Mutation`.

`Mutation` variants:

| Variant | Effect |
|---|---|
| `Keep` | Leave the node in place; descend into its children. |
| `Remove` | Drop the node. Cursor points at next sibling. |
| `Replace(node)` | Swap the node and descend into the replacement. |
| `ReplaceMany(vec)` | 1-to-N substitution; cursor advances past inserts. |
| `InsertBefore(vec)` | Splice nodes in front; cursor still on this node. |
| `InsertAfter(vec)` | Splice nodes after; cursor advances past inserts. |

#### Direct mutation primitives

If you've got a `Node` you know is a container, the bare functions on
`postcss_core::container` work too: `append`, `prepend`, `insert_before`,
`insert_after`, `remove_at`, `replace_at`, `remove_all`.

#### `raws` — the load-bearing detail

Every Node carries `Raws { before, after, between, semicolon, after_name,
important, left, right, value, selector, params, own_semicolon, .. }`.
**These bytes are the difference between byte-identical output and a
production hash rotation.** When you create a new node from scratch
(rather than mutating one the parser produced), set `raws.before` /
`raws.between` to match what upstream's normalizer would emit. When in
doubt, clone an existing node and mutate, rather than constructing one
from defaults.

### postcss-selector-parser

```rust
use postcss_selector_parser::{Processor, NodeKind};

let result = Processor::new().process(".foo .bar", |root| {
    for selector in &mut root.nodes {
        for node in &mut selector.nodes {
            if node.kind == NodeKind::ClassName {
                let renamed = format!("hashed-{}", node.value);
                node.set_value(renamed);  // Clears raw_value automatically.
                selector.raw_value = None; // Tell the stringifier to re-emit.
            }
        }
    }
})?;
```

Typed kinds you'll hit: `Root`, `Selector`, `ClassName`, `Identifier`
(id), `Tag`, `Universal`, `Nesting` (`&`), `Pseudo`, `Attribute`,
`Combinator`, `Comment`, `String`.

**`node.set_value(new_value)` clears `raw_value` for you.** If you mutate
a child without going through `set_value`, also clear the parent
`Selector`'s and `Root`'s `raw_value` so the stringifier walks the typed
shape instead of the cached source bytes.

### postcss-value-parser (decl values, function args)

Tokenizer produces `Node { kind: Function | String | Div | Space | Word
| Comment | UnicodeRange, value, before, after, quote, unclosed, nodes,
.. }`. Walk with `postcss_value_parser::walk`.

### postcss-values-parser (the plural — for expand-shorthands)

Different AST: `Numeric { value, unit }`, `Word`, `Func { name, nodes }`,
`Quoted`, `Punctuation`, `Operator`, `UnicodeRange`, `AtWord`,
`Comment`. **Round-trip parity is locked** (9 tests in
`postcss-values-parser/src/lib.rs::roundtrip_tests`) — your mutations
must drop `node.raws_before` / `node.raws_after` if they aren't valid for
the new shape, otherwise the original bytes get re-emitted.

### Per-node attribute bag (`Node.attrs`)

Every `postcss_core::Node` carries an `attrs: NodeAttrs` field — an
insertion-ordered map of plugin-private state. Mirrors the JS pattern of
stashing state directly on the AST (`node._autoprefixerPrefix`,
`decl._autoprefixerCascade`, etc.).

```rust
use postcss_core::{AttrValue, NodeKind};

// Memo: cache the prefix decision on first visit, read on subsequent.
if let Some(b) = node.attrs.get_bool("_autoprefixerPrefix") {
    return b;
}
let answer = compute_prefix(node);
node.attrs.set("_autoprefixerPrefix", AttrValue::Bool(answer));
```

**Conventions plugin authors must follow:**

1. **Namespace your keys** with your plugin name: `_<plugin>_<field>`.
   Bare keys like `cache` will collide with another plugin tomorrow.
2. **Use `IndexMap` shapes** (`AttrValue::StringMap`, `NestedStringMap`)
   when iteration order reaches output bytes. `HashMap` is banned (the
   cardinal-rule check still applies).
3. **`AttrValue` variants are extensible** but only by changing the
   `postcss-core` enum — file an issue first if you need a shape that
   isn't `Bool`/`String`/`Int`/`StringMap`/`NestedStringMap`.

When deep-cloning a node, plugins like autoprefixer drop their
private keys to prevent stale memoization on the clone:

```rust
// Mirrors `prefixer.js::clone` upstream.
let clone = node.clone_without(&["_autoprefixerPrefix", "_autoprefixerValues"]);
```

`Node::clone_without(&[])` is identical to `node.clone()`.

### Parent-aware visitors (`walk_*_mut_with_parent`)

Most plugins only need the simple `walk_*_mut` family from § 3 above.
Some plugins (autoprefixer, anything that does `node.parent.parent`)
need to walk *up* the tree or call methods on arbitrary ancestors.

For those, use the parent-aware family:

```rust
use postcss_core::{
    walk_decls_mut_with_parent, DeferredMutation,
    node_at_path, parent_some, walk_up_with,
};

walk_decls_mut_with_parent(&mut root.root, |root, path, ctx| {
    // `root: &mut Node` — entire tree, you choose when to take a
    //                    mutable borrow vs an immutable borrow.
    // `path: &[usize]`  — index path from root to the current node.
    // `ctx: WalkCtx`    — { index, parent_len } for the current visit.

    // READ the current node:
    let is_color_red = match node_at_path(root, path).map(|n| &n.kind) {
        Some(NodeKind::Declaration(d)) => d.prop == "color" && d.value == "red",
        _ => false,
    };

    // READ siblings — by predicate:
    let has_bg_sibling = parent_some(root, path, |s| {
        matches!(&s.kind, NodeKind::Declaration(sd) if sd.prop == "background")
    });
    let no_display_sibling = parent_every(root, path, |s| match &s.kind {
        NodeKind::Declaration(d) => d.prop != "display",
        _ => true,
    });

    // READ siblings — by index (for `selector.js::already`-style backward scans):
    let prev = sibling_relative(root, path, -1);     // previous sibling, or None
    let first = sibling_at(root, path, 0);           // absolute index 0
    let all_siblings = parent_nodes(root, path);     // full Vec<Node>, or None at root

    // WALK UP through ancestors:
    walk_up_with(root, path, |anc| {
        // return false to stop early
        true
    });

    // MUTATE: defer via DeferredMutation. The visitor adjusts cursor.
    DeferredMutation::Keep
});
```

**Architectural drift you need to know about.** Upstream JS uses
`node.parent` back-pointers; this port uses **index paths**. Reasons:

- `Weak<Node>` back-pointers turn ownership into a fight with the
  borrow checker — every existing plugin would have to wrap nodes in
  `Rc<RefCell<...>>` and lose static borrow safety.
- Index paths cost an extra `Vec<usize>` per visit but compose with
  the existing simple `walk_*_mut` family without breaking it.
- The closure receives `&mut Node` (root) and `&[usize]` (path) instead
  of `&mut Node` (current) so it can re-borrow root immutably to call
  `parent_some` etc. — Rust can't prove the alias-safety statically;
  this signature side-steps it.

**Mutations during a parent-aware walk** must use `DeferredMutation`:

| Variant | Effect |
|---|---|
| `Keep` | Leave; descend into children. |
| `Remove` | Drop the node. Cursor stays. |
| `Replace(node)` | Swap; descend into replacement. |
| `ReplaceMany(vec)` | 1-to-N substitution; cursor advances past inserts. |
| `InsertBefore(vec)` | Splice before this node; cursor advances. |
| `InsertAfter(vec)` | Splice after; cursor advances past original + inserts. |

These map 1:1 to the `Mutation` enum used by `walk_*_mut`. The cursor
adjustment is identical — nothing gets skipped or double-visited.

For raw `node.parent.insertBefore(node, cloned)` style calls outside
the visitor's normal mutation flow, use:

```rust
use postcss_core::insert_before_at_path;
insert_before_at_path(&mut root.root, &path, new_node);
```

This calls into `insert_before_with_normalize` so the Root's
raws-transfer fires correctly when inserting at index 0.

**When NOT to use the parent-aware family:** if your plugin only needs
to look at the current node and its direct children (most local
plugins fall in this bucket), stay on `walk_*_mut` from § 3 — simpler,
slightly faster, no path arithmetic.

---

## 4. Helpers you must use

### Number-to-string: `postcss_core::js_number_to_string(n)`

Any time you emit a number to a CSS string, **call this**, not
`format!("{}", n)`. Rust's f64 Display agrees with V8 for most values but
diverges on negative zero and at the edges of scientific notation. The
helper is byte-tested against `String(n)` in JS.

```rust
use postcss_core::js_number_to_string;
let css = format!("{}px", js_number_to_string(0.0));   // "0px", not "-0px"
let css = format!("{}", js_number_to_string(0.1+0.2)); // "0.30000000000000004"
```

### Hash: `compiled_utils::hash(s)`

The class-name hash. **Bit-identical to JS** — verified against 33
test vectors at `crates/compiled-utils/tests/hash_vectors.json`.
If you regenerate the vectors (don't, unless upstream changes), also
re-run `node crates/compiled-utils/scripts/hash-vectors.mjs > ...`.

### Color manipulation: `colord::colord(input)`

Full port: hex/rgb/hsl/named parsing, `to_hex` / `to_rgb_string` /
`to_hsl_string`, `lighten` / `darken` / `saturate` / `desaturate` /
`invert` / `rotate`, named-color reverse lookup with `closest`.

### Browser support queries: `caniuse_api::is_supported(feature, query)`

Backed by the pinned `caniuse-lite@1.0.30001690` snapshot. Strict-y
matching is upstream behavior — a feature with `"y #1"` (note attached)
won't satisfy `is_supported`. Don't try to "fix" this; it's the spec.

### Browserslist: `browserslist_shim::resolve(query, ignore_unknown)`

The default query is locked to `browserslist@4.24.4`'s
`["> 0.5%", "last 2 versions", "Firefox ESR", "not dead"]`.

### List splitting: `postcss_core::list::space(s)` / `comma(s)`

Match upstream `list.space()` and `list.comma()` for splitting decl
values that respect parens and quotes. Don't write your own splitter.

### CSS-aware unique: `compiled_utils::unique(arr)`

`atomicifyRules` calls `unique(classNames)` to dedupe before returning.
Use this exact helper — Vec dedup behavior differs.

### Kebab case: `compiled_utils::kebab_case(s)`

Same regex as upstream (`[A-Z\u00C0-\u00D6\u00D8-\u00DE]`). Don't roll
your own.

---

## 5. Errors

Use `postcss_core::PluginError`:

```rust
use postcss_core::{PluginError, PluginResult};

pub fn my_plugin(root: &mut Root, _opts: &MyOpts) -> PluginResult {
    if some_condition {
        return Err(PluginError::generic("my-plugin", "config invalid"));
    }
    let bad_node = /* ... */;
    return Err(PluginError::from_node("my-plugin", "specific failure", bad_node));
}
```

`PluginError::from_node` reads `node.source.start` for line/col info.
NEVER `panic!()` in plugin code — panics in the NAPI shim become opaque
JS exceptions and you lose the line number.

---

## 6. The non-negotiable rules

1. **Bytes are the contract, not behaviour.** If upstream emits `0.5em`
   and your port emits `.5em`, that's a hash rotation. Both might be
   "valid CSS" — it doesn't matter.

2. **Bugs are features.** If upstream `postcss-nested@5.0.6` mishandles
   `:starting-style`, your port mishandles it the same way. Don't fix.
   File the bug, link the upstream commit that introduced it, move on.

3. **`raws` is sacred.** Whenever you mutate a node, ask "does this
   change which `raws` field gets emitted?" If you remove a declaration,
   what happens to the `raws.before` of the next sibling? (Answer:
   upstream merges them. So must you.) Read upstream's stringifier when
   in doubt.

4. **Iteration order matters.** Use `IndexMap`, never `HashMap`, for any
   collection whose iteration order reaches output bytes. The clippy
   lint will yell if you slip.

5. **No `format!("{}", f64)` for output bytes.** Use
   `postcss_core::js_number_to_string`.

6. **No `hashbrown`/`fxhash`/etc. for output paths.** They're
   non-deterministic across runs.

7. **Test against upstream JS for any plugin that emits numbers, hashes,
   or color strings.** Generate JS reference vectors with a small Node
   script next to your tests (`scripts/<plugin>-vectors.mjs`), check
   them in, load via `include_str!` like
   `crates/compiled-utils/tests/hash_parity.rs`.

8. **No "improvements" to upstream behaviour.** Period. If you spot a
   bug in upstream, file it; do not fix it.

---

## 7. The TODO body in your file

Each scaffolded plugin file has this shape:

```rust
//! Port of `packages/css/src/plugins/<file>.ts`.

use postcss_core::Root;

pub fn discard_empty_rules(_root: &mut Root) {
    unimplemented!("Phase 4a — port discard-empty-rules.ts");
}
```

Your job is to:

1. **Read the upstream `.ts`** end to end before writing a single line.
   Then read it again with the JS source for any imports it touches.
2. Replace the body with the real logic, walking the AST per Section 3.
3. Update the function signature to take `opts: &XYZOpts` if upstream
   takes config, AND `-> PluginResult` if it can fail.
4. Wire the plugin into `crates/css/src/transform.rs` at the canonical
   pipeline position (the doc comment in `transform.rs` lists the order).
5. Re-export the plugin's public surface from
   `crates/compiled-css/src/lib.rs` if external callers need it.

---

## 8. Tests are not optional

Every plugin gets at minimum:

1. **Unit tests** for the plugin's own logic — happy path + edge cases.
   Mirror the test inputs from `packages/css/src/plugins/__tests__/<file>.test.ts`
   verbatim where they exist.
2. **Round-trip test** — `transform_css(input).sheets[0]` for an input
   the plugin doesn't touch should round-trip byte-identically.
3. **JS-parity test** — for plugins that emit numbers, hashes, or color
   strings, generate JS reference outputs and lock them via
   `include_str!`.

Test file lives next to the source: `crates/compiled-css/src/plugins/<plugin>.rs`
in a `#[cfg(test)] mod tests` block. Don't create separate test files
unless the test count makes the source file noisy (>50 tests).

---

## 9. Worked example: `discard-empty-rules`

Upstream:

```ts
// packages/css/src/plugins/discard-empty-rules.ts
import { Plugin } from 'postcss';
export const discardEmptyRules = (): Plugin => ({
  postcssPlugin: 'discard-empty-rules',
  Once(root) {
    root.walkRules(rule => {
      if (rule.nodes.length === 0) rule.remove();
    });
  },
});
```

Rust port:

```rust
//! Port of `packages/css/src/plugins/discard-empty-rules.ts`.

use postcss_core::container::{walk_rules_mut, Mutation};
use postcss_core::{NodeKind, PluginResult, Root};

pub fn discard_empty_rules(root: &mut Root) -> PluginResult {
    walk_rules_mut(&mut root.root, &mut |node, _ctx| {
        if let NodeKind::Rule(rule) = &node.kind {
            if rule.nodes.is_empty() { return Mutation::Remove; }
        }
        Mutation::Keep
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use postcss_core::{parse, stringify};

    #[test]
    fn drops_empty_rule() {
        let mut root = parse("a {} b { color: red; }").unwrap();
        discard_empty_rules(&mut root).unwrap();
        let out = stringify(&root);
        assert!(!out.contains("a {}"));
        assert!(out.contains("b { color: red; }"));
    }

    #[test]
    fn keeps_non_empty_rule() {
        let css = "a { color: red; }";
        let mut root = parse(css).unwrap();
        discard_empty_rules(&mut root).unwrap();
        // Round-trip on input the plugin doesn't touch.
        assert_eq!(stringify(&root), css);
    }
}
```

That's the minimum. Each plugin's code review will check:
- 1:1 file mapping
- `raws` preservation on un-touched paths
- Real upstream test inputs ported as Rust test cases
- No new helpers in the wrong crate

---

## 10. When you're stuck

The vendored upstream source lives at `crates/_vendor/`:

```
crates/_vendor/postcss-8.4.31/package/lib/parser.js
crates/_vendor/postcss-selector-parser-6.0.13/package/dist/parser.js
crates/_vendor/postcss-values-parser-6.0.2/package/lib/ValuesParser.js
crates/_vendor/colord-2.9.1/package/index.mjs
...
```

Read these, not Stack Overflow, not the GitHub `main` branch. Pinned
versions only.

For postcss-internal questions (how does `raws.between` flow?), the
parser+stringifier in `crates/postcss-core/src/{parser,stringifier}.rs`
already handle every edge case the test suite exercises — read those
first before asking.

---

## 11. Phase order

Phases per `crates/EXECUTION_PLAN.md`:

| Phase | Plugins | Crate |
|---|---|---|
| 4a | `discard-empty-rules`, `discard-duplicates` (local), `extract-stylesheets` | `compiled-css` |
| 4b | `parent-orphaned-pseudos`, `flatten-multiple-selectors`, `increase-specificity` | `compiled-css` |
| 4c | `merge-duplicate-at-rules`, `sort-atomic-style-sheet`, `normalize-current-color`, `sort-pseudo-selectors`, `sort-shorthand-declarations` | `compiled-css` |
| 4d | `atomicify-rules` ← **single most important plugin; uses compiled_utils::hash** | `compiled-css` |
| 4e | `expand-shorthands/*` (13 files) | `compiled-css` |
| 5a | `postcss-nested` | new crate `postcss-nested` |
| 5b | `postcss-normalize-whitespace` | new crate |
| 5c | `postcss-discard-duplicates` (v6, npm) | new crate |
| 6 | 14 cssnano plugins (preset + sub-plugins) | new crates |
| 7 | `autoprefixer` | new crate `autoprefixer` |

**Concurrency:** Phases 4a-c can be parallelized. Phase 4d (`atomicify-
rules`) blocks 4e because expand-shorthands runs before atomicify in the
pipeline but they share zero state — they can also run in parallel as
long as you don't touch `transform.rs`'s pipeline wiring until BOTH
land.

---

## 12. Foundation status (what's already done)

You are **not** porting these — they exist, are tested, and you depend
on them:

| Crate | Status | Tests |
|---|---|---|
| `postcss-core` | Real parser+stringifier+container API+walks+mutations+js_number_to_string+plugin_error | 31 |
| `postcss-selector-parser` | Tokenizer + typed AST (ClassName/Identifier/Pseudo/Attribute/Combinator/etc.) | 26 |
| `postcss-value-parser` | Full parse+stringify+walk+unit | 17 |
| `postcss-values-parser` | Tokenize+classify+round-trip | 15 |
| `colord` | Full color manipulation + 7 plugins | 39 |
| `fraction-js` | All public methods | 10 |
| `cssnano-utils` | getArguments / rawCache / sameParent | 5 |
| `caniuse-db` | 579 features loaded from frozen 1.0.30001690 snapshot | 5 |
| `caniuse-api` | features / find / isSupported / getSupport | 6 |
| `browserslist-shim` | parseConfig / parsePackage / 4.24.4 defaults | 9 |
| `compiled-utils` | hash (bit-parity vs JS) / unique / flatten / kebab / shorthand tables | 25 |
| `css` | transform_css / sort signatures locked, identity passthrough | 4 |

Total: **196 tests passing, 0 failing.**

If a helper you need isn't in this list, **stop and ask** before rolling
your own. Likely it should go in `compiled-utils` or `postcss-core`,
not in your plugin.
