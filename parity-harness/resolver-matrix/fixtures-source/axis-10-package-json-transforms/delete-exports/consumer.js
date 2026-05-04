// Resolver-matrix axis-10 fixture consumer. Tests the §5.4c
// TransformingFileSystem end-to-end: when the consumer config has
// a `deleteKey "exports"` transform, the resolver should NOT see
// the `exports` field on this package and should fall back to
// `main`. Without the transform, modern Node-style resolution
// honours `exports` first.
require('parity-pkg-with-both-main-and-exports');
