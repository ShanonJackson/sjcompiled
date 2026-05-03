//! Port of `crates/_vendor/autoprefixer-10.4.14/package/lib/info.js`.
//!
//! `info.js`'s exported function produces a human-readable diagnostic
//! string for the `npx autoprefixer --info` CLI. **It is not on the
//! hashing path** — `transformCss` never calls it, no plugin in the
//! AFM pipeline consults it, and porting it would consume agent
//! budget for zero byte-equality benefit.
//!
//! The bare module is preserved (rather than removed) because:
//! - `lib/autoprefixer.js` does `require('./info')` at module-init
//!   time (line 8). A missing module would change the load-time
//!   error surface, and a future agent porting the postcss-plugin
//!   shape may want to mirror the import shape.
//! - The 1:1 file-mapping doc in `lib.rs` lists `info.rs`.
//!
//! See HANDOVER §10 for the "what we do NOT need to port" list.

#[allow(dead_code)]
pub(crate) fn _stub_marker() {
    // Keeps the module non-empty for module-tree tests.
}
