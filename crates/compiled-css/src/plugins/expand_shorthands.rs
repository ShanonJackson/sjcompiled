//! Port of `packages/css/src/plugins/expand-shorthands/*.ts`.
//!
//! Folder/file mapping (1:1 with upstream `expand-shorthands/`):
//!   - `index.ts`            -> `expand_shorthands.rs` (this file — entry)
//!   - `background.ts`       -> `background.rs`
//!   - `flex.ts`             -> `flex.rs`
//!   - `flex-flow.ts`        -> `flex_flow.rs`
//!   - `margin.ts`           -> `margin.rs`
//!   - `outline.ts`          -> `outline.rs`
//!   - `overflow.ts`         -> `overflow.rs`
//!   - `padding.ts`          -> `padding.rs`
//!   - `place-content.ts`    -> `place_content.rs`
//!   - `place-items.ts`      -> `place_items.rs`
//!   - `place-self.ts`       -> `place_self.rs`
//!   - `text-decoration.ts`  -> `text_decoration.rs`
//!   - `utils.ts`            -> `utils.rs`
//!   - `types.ts`            -> `types.rs`

pub mod background;
pub mod flex;
pub mod flex_flow;
pub mod margin;
pub mod outline;
pub mod overflow;
pub mod padding;
pub mod place_content;
pub mod place_items;
pub mod place_self;
pub mod text_decoration;
pub mod utils;
pub mod types;

use postcss_core::Root;

/// `expandShorthands()` factory — entry point for the plugin. Phase 4e.
pub fn expand_shorthands(_root: &mut Root) {
    unimplemented!("Phase 4e — port plugins/expand-shorthands/index.ts");
}
