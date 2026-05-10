# Repro: SIGSEGV in `@atlassian/swc-native` on a real AFM file

## TL;DR

`@atlassian/swc-native` (the native, non-WASI host wrapping the Rust
SWC port of `@compiled/babel-plugin`) hard-crashes with `SIGSEGV` (exit
139) when transforming
`platform/packages/editor/editor-core/src/ui/EditorContentContainer/EditorContentContainer-compiled.tsx`.

The same file is processed cleanly by:

1. **Stock `@swc/core` with the same options and no compiled plugin loaded** — proves the crash isn't an SWC parser/transform issue.
2. **The JS oracle (`@compiled/babel-plugin@0.39.1` via `@babel/core`)** with the real Jira plugin order (`@atlaskit/tokens/babel-plugin` first, then `@compiled/babel-plugin` with `@jira-dev/compiled-resolver`) — proves the file IS valid Compiled input.
3. **The real Jira jest pipeline with `IS_SWC_NATIVE_ENABLED = false`** — the otherwise-failing test (`jira/src/packages/admin-pages/announcement-banner-editor/tests/AnnouncementBannerEditor.test.tsx`) goes from segfault to **3/3 pass in 63 s**.

So the SIGSEGV is exclusive to the Rust/SWC port. This is a port-side
bug, not a "garbage input" issue.

### Important: plugin ordering matters

If you run `@compiled/babel-plugin` **without** `@atlaskit/tokens/babel-plugin` running first, the JS plugin throws `SyntaxError: Found a mix of an indirect selector and a dynamic variable` at line 294 (the `cssMap({...})` call). That's because `cssMap` keys reference `token('color.text')` etc., and unresolved `token(...)` calls trigger Compiled's "indirect selector + dynamic variable" guard. The real Jira pipeline avoids this by running `@atlaskit/tokens` first, which collapses every `token(...)` to a static literal before Compiled ever sees it. The repro driver here mirrors that ordering.

## How to reproduce

```bash
node platform/crates/sjcompiled/tmp_rovodev_repro_segv_editor_content_container/run.mjs
```

Output:

```
ENGINE 2 — stock @swc/core (no compiled plugin)
✅ survived. output length = 124561 bytes

ENGINE 3 — @atlassian/swc-native + compiled SWC port
(if this is the last line printed, the process SIGSEGVd inside the addon)
Segmentation fault   (exit 139)
```

Engine 1 (Babel oracle) ✅ succeeds in ~1.5 seconds with output ~165 KB.
Engine 2 (stock SWC) ✅ succeeds.
Engine 3 (swc-native + compiled port) ❌ exits 139.

## Files

| File | Purpose |
|---|---|
| `input.tsx`              | Verbatim copy of the failing AFM source file (~110 KB / 2602 lines). |
| `run.mjs`                | Standalone driver — runs three engines and reports SIGSEGV. |
| `bisect.mjs`             | Linear bisection that finds the smallest *contiguous* slice that still SIGSEGVs. |
| `bisect-middle.mjs`      | Middle-bisection — holds the component (bottom), shrinks the giant `cssMap` (top). |
| `bisect-bottom.mjs`      | Bottom-bisection — holds the 36-line top prefix, shrinks the component. |
| `test-filename.mjs`      | Confirms the SIGSEGV is independent of the `filename` option. |
| `minimal-repro.tsx`      | Result of `bisect.mjs` (~2599 lines — barely shrinks because crash spans the whole file). |
| `minimal-repro-2.tsx`    | Result of `bisect-bottom.mjs` (~462 lines — keeps imports + the entire component). |
| `minimal-repro-middle.tsx` | Result of `bisect-middle.mjs` (~466 lines — same shape). |

## What we know

1. **Not path-dependent.** `test-filename.mjs` reruns the same input
   with `filename` set to the real AFM path, the local-cached path,
   `/tmp/doesnotexist.tsx`, `<anon>`, and `""`. **All five** SIGSEGV.
   So the crash is NOT in the `oxc_resolver`-via-`host_to_wasi`
   pathway that the AFM-side comment in
   `jira/dev-tooling/packages/jest-common/src/babel-transformer.js:421-428`
   speculates about.

2. **Bisection summary.**
   - The minimal-contiguous slice is 2599/2602 lines — nearly the full
     file. The first 2 lines and the last 1 line can be removed; nothing
     else.
   - The minimal **top-prefix + component** repro is 462 lines: the 36
     lines of `import` + JSX-pragma preamble, plus the 426-line
     `EditorContentContainerCompiled` `React.forwardRef` body
     (lines 2174..end of original).
   - The cssMap (lines 291..2168, ~1900 lines of CSS-in-JS) is **NOT**
     required for the crash. The component body alone, with imports
     and references to (now-undefined) `editorContentStyles.<XXX>`
     keys, is enough to trigger the SIGSEGV.
   - Removing **any single line** from the 462-line minimal repro
     stops the crash, which suggests the crash is driven by some
     interaction inside the JSX `className={[...]}` array that
     references ~120 `editorContentStyles.*` member-expressions plus
     several conditional / ternary `expValEquals`-gated styles plus
     spread `editorExperiment(...) && [...]` arrays.

3. **The compiled port is required.** Removing the
   `'@atlassian/swc-plugin-compiled'` entry from
   `experimental.plugins` in `run.mjs` (effectively running stock
   `@swc/core`) makes the crash disappear. The remaining
   `'@atlaskit/tokens'` plugin alone does not crash.

4. **Plugin options are mostly orthogonal.** Removing `addComponentName`,
   `extract`, the `resolver` config, etc. one at a time does NOT make
   the crash go away (not exhaustively bisected, but spot-checked).

## What we tried that does NOT crash

Synthetic minimal inputs (built by `synthesize.mjs`) of up to 233 lines
exercising plausible suspect shapes:

| Shape | Result |
|---|---|
| `(props) => <div className={[styles.base × N]}/>`, N up to 120     | ✅ ok |
| `forwardRef((props, ref) => <div className={[styles.base × N]}/>)`, N up to 120 | ✅ ok |
| Same + 5 ternaries `cond ? styles.a : styles.b` | ✅ ok |
| Same + spread array `cond && [styles.a, styles.b, …]` | ✅ ok |
| All of the above combined, 200 members | ✅ ok |

So a "many member-accesses + ternaries + spreads" shape is not enough on
its own. The trigger is more specific to the real file's combination of
imports, type aliases, prop destructuring, and JSX classname graph.

## Suggested investigation path

The crash bisects to the JSX body that does:

```jsx
<div className={[
  editorContentStyles.baseStyles,
  editorContentStyles.maxModeReizeFixStyles,
  // ~120 more `editorContentStyles.<XXX>` member accesses,
  // some inside ternaries:
  expValEquals('flag', 'isEnabled', true)
    ? editorContentStyles.someStyles
    : editorContentStyles.otherStyles,
  // some inside spread arrays:
  editorExperiment('flag', true) && [
    editorContentStyles.aStyles,
    editorContentStyles.bStyles,
    /* … */
  ],
]} />
```

…where `editorContentStyles` is bound to a `cssMap({...})` call. In
the minimal repro the cssMap is *deleted*, so every member-access
resolves to `undefined.<XXX>` at runtime — but the **plugin runs at
compile time** and shouldn't care. Suspect the visitor that walks the
classname-array literal:

* either dereferences a stale `cssMap`-binding lookup that returns
  `None` and then unwraps it,
* or recurses through the conditional/ternary/spread structure and
  hits a stack overflow or an unchecked index, manifesting as SIGSEGV.

Worth a quick `RUST_BACKTRACE=1` / `lldb` run on the host to capture
the actual stack — the addon's panic hook (if any) is being bypassed.

## Why this matters for AFM

This file is one of dozens of `*-compiled.tsx` files in the editor
package. Any test that imports the editor will pull this file into
the Jest module graph and crash the whole worker process. We
currently lose entire test suites (e.g. `AnnouncementBannerEditor.test.tsx`)
to this SIGSEGV with no Jest-level error message — the worker just
exits 139.

## Repro driver requirements

Standalone — only requires:
* `@swc/core` (engine 2)
* `@atlassian/swc-native` (engine 3, freshly built from
  `platform/crates/swc-native`)
* `@babel/core` + `@compiled/babel-plugin` + presets (engine 1)

All resolved from the AFM root `node_modules`. No imports of any AFM
source packages except the one input file we're testing.
