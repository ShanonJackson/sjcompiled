// Resolver-matrix axis-11 fixture consumer. Tests the §5.4d
// PreferFirstDispatcher end-to-end:
//
// - With NO preferFirst rule: default-config resolver doesn't know
//   about `af:exports`, falls back to `main` → main-entry.js.
// - With a preferFirst rule matching `@matched/` and
//   use.exportsFields = ["af:exports", "exports"]: matched resolver
//   walks af:exports first → af-entry.js.
// - With a preferFirst rule matching `@nomatch/` (no overlap with
//   the actual specifier prefix): dispatcher returns None,
//   resolution falls through to base → main-entry.js.
require('@matched/pkg-with-af-exports');
