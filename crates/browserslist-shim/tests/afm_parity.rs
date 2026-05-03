//! AFM parity gate — closes the previously-open
//! `browserslist_shim_matches_js_oracle_for_canonical_queries` gate
//! (see `crates/autoprefixer/HANDOVER.md` §6).
//!
//! Loads the AFM-pinned `.browserslistrc` from
//! `tests/fixtures/afm/.browserslistrc` (byte-identical to
//! `jira/.browserslistrc` per `BROWSER_LIST_FROM_AFM.md`, SHA256
//! `08c8e1bf56ad773621c9b264971365f66f78a808d6d369a4ea9584a02da459cb`)
//! and asserts that `resolve_with("", { path: fixtures_dir })` returns
//! the frozen 14-entry list AFM's runtime instrumentation captured.
//!
//! ## Why this gate exists
//!
//! The autoprefixer port runs `Browsers::new(query, opts)` →
//! `browserslist_shim::resolve_with`. With `query = ""` and
//! `path = "<jira>"` (mirroring autoprefixer's `browserslist(null, { path })`
//! call), the shim must walk up to AFM's `.browserslistrc`, parse it,
//! and resolve every atom against `caniuse-db@1.0.30001766`. Drift
//! anywhere along that chain rotates downstream prefix decisions.
//!
//! See `crates/browserslist-shim/AFM_PORT_NOTES.md` for the full
//! architecture and the rationale for the AFM-fast-path / oxc-fallback
//! split.

use browserslist_shim::index::{resolve_with, ResolveOpts};
use std::path::PathBuf;
use std::sync::OnceLock;

/// Frozen oracle: the exact 14-entry list AFM's runtime instrumentation
/// captured (see `BROWSER_LIST_FROM_AFM.md`). Drift here is a hash
/// rotation event — investigate the caniuse-db pin or the resolver
/// before "fixing" the test.
const AFM_EXPECTED: &[&str] = &[
    "and_chr 144",
    "chrome 144",
    "chrome 143",
    "chrome 142",
    "chrome 141",
    "chrome 140",
    "edge 144",
    "edge 143",
    "firefox 147",
    "firefox 146",
    "ios_saf 26.2",
    "ios_saf 26.1",
    "safari 26.2",
    "safari 26.1",
];

/// Asserts the AFM `.browserslistrc` fixture is byte-identical to the
/// version AFM's dependency engineer reported. Drift here means the
/// fixture file was edited locally — the resolver test below is only
/// meaningful if the input bytes match AFM's.
#[test]
fn afm_browserslistrc_fixture_sha256_matches() {
    use std::io::Read;
    let path = fixture_dir().join(".browserslistrc");
    let mut f = std::fs::File::open(&path)
        .unwrap_or_else(|e| panic!("open AFM fixture {path:?}: {e}"));
    let mut bytes = Vec::new();
    f.read_to_end(&mut bytes).expect("read AFM fixture");

    // Pure-Rust SHA-256 via std would be ideal but std doesn't ship one.
    // Compute it here with the tiny inline impl below to avoid a dev-dep.
    let got = sha256_hex(&bytes);
    let expected = "08c8e1bf56ad773621c9b264971365f66f78a808d6d369a4ea9584a02da459cb";
    assert_eq!(
        got, expected,
        "AFM .browserslistrc fixture drifted. Expected SHA256 {expected}, got {got}.\n\
         The AFM dependency engineer reported this exact hash in BROWSER_LIST_FROM_AFM.md.\n\
         Either restore the fixture from AFM or coordinate a re-pin (which is a \
         hash-rotation event for every consumer of the Rust port)."
    );
}

/// **Closure of the previously-open browserslist parity gate.**
///
/// Resolves the AFM fixture's `.browserslistrc` via the shim and asserts
/// the output matches AFM's runtime instrumentation byte-for-byte.
///
/// If this fails, in priority order:
/// 1. Did the caniuse-db pin move? Check `caniuse_db::CANIUSE_LITE_VERSION`.
/// 2. Did the AFM-fast-path resolver in `crates/browserslist-shim/src/index.rs`
///    regress? Check the `afm_fast_path_*` unit tests there.
/// 3. Did the AFM `.browserslistrc` change? The previous test should
///    catch that — if BOTH this AND the SHA256 test fail, AFM updated
///    their config and we need to coordinate.
#[test]
fn afm_browserslistrc_resolves_to_frozen_oracle() {
    let dir = fixture_dir();
    let opts = ResolveOpts {
        path: Some(&dir),
        env: None,
        ignore_unknown_versions: true,
    };
    let got = resolve_with("", &opts);
    let expected: Vec<String> = AFM_EXPECTED.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        got, expected,
        "AFM browserslist resolution drifted. See doc-comment for triage steps."
    );
}

fn fixture_dir() -> PathBuf {
    static CACHE: OnceLock<PathBuf> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures")
                .join("afm")
        })
        .clone()
}

// ---------------------------------------------------------------------
// Inline SHA-256 (FIPS 180-4) — avoids pulling a `sha2` dev-dep just
// for one fixture-integrity check. Tested implicitly by the
// `afm_browserslistrc_fixture_sha256_matches` assertion above.
// ---------------------------------------------------------------------

fn sha256_hex(input: &[u8]) -> String {
    let digest = sha256(input);
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn sha256(input: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1,
        0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
        0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
        0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147,
        0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
        0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
        0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
        0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c,
        0x1f83d9ab, 0x5be0cd19,
    ];

    let bit_len: u64 = (input.len() as u64) * 8;
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0x00);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = [0u8; 32];
    for (i, word) in h.iter().enumerate() {
        out[i * 4..(i + 1) * 4].copy_from_slice(&word.to_be_bytes());
    }
    out
}

#[cfg(test)]
mod sha256_self_test {
    use super::sha256_hex;

    #[test]
    fn empty_string_vector() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn abc_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
