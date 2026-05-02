//! Phase 0 sandbox / WASI / postcard probes — PLAN.md §3.9.14.
//!
//! One plugin, multiple probe modes selected via plugin config:
//!
//!   { "mode": "wasi-io",            "callScratch": "<absPath>" }
//!   { "mode": "wasi-mtime",         "callScratch": "<absPath>" }
//!   { "mode": "instance-teardown",  "callScratch": "<absPath>" }
//!   { "mode": "scratch-reach",      "workerScratchDir": "<a>", "callScratch": "<b>" }
//!   { "mode": "postcard-roundtrip", "workerScratchDir": "<absPath>" }
//!
//! Each probe writes a result JSON to `<callScratch>/probe-result.json`
//! (or for postcard, `<workerScratchDir>/probe.bin`). The host test
//! reads it back and asserts.
//!
//! Probe 8 (byte-cap eviction) is a pure Rust unit test — see
//! `tests/byte_cap_eviction.rs` in the babel-plugin crate (when added).
//! It does not need a probe plugin.
//!
//! Probe 5 (cache-file race) is a JS-side negative test in
//! `phase0-probes/probes.test.ts`. It does not need a special plugin.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use swc_core::ecma::ast::Program;
use swc_core::plugin::metadata::TransformPluginMetadataContextKind;
use swc_core::plugin::plugin_transform;
use swc_core::plugin::proxies::TransformPluginProgramMetadata;

// Static counter for probe 4 (instance-teardown). If the wasm instance
// is torn down per transform() call as PLAN.md §3.9.0 claims, this
// always reads as 0 on entry.
static INSTANCE_LIVE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Deserialize)]
#[serde(tag = "mode", rename_all = "kebab-case")]
enum ProbeConfig {
    WasiIo {
        #[serde(rename = "callScratch")]
        call_scratch: String,
    },
    WasiMtime {
        #[serde(rename = "callScratch")]
        call_scratch: String,
    },
    InstanceTeardown {
        #[serde(rename = "callScratch")]
        call_scratch: String,
    },
    ScratchReach {
        #[serde(rename = "workerScratchDir")]
        worker_scratch_dir: String,
        #[serde(rename = "callScratch")]
        call_scratch: String,
    },
    PostcardRoundtrip {
        #[serde(rename = "workerScratchDir")]
        worker_scratch_dir: String,
    },
}

#[derive(Serialize)]
struct ProbeResult {
    probe: &'static str,
    ok: bool,
    detail: serde_json::Value,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct CacheFileMin {
    version: u32,
    schema_hash: [u8; 32],
    layer2: Vec<(u64, Vec<u8>)>, // simplified for the round-trip probe
}

fn write_result(call_scratch: &str, result: &ProbeResult) {
    let path = PathBuf::from(call_scratch).join("probe-result.json");
    let bytes = serde_json::to_vec(result).expect("serialize result");
    fs::write(&path, &bytes).expect("write probe-result.json");
}

fn run_wasi_io(call_scratch: &str) {
    let cwd = std::env::current_dir();
    eprintln!("[probe wasi-io] cwd={:?}", cwd);

    let mut attempts = Vec::new();

    // READ FIRST — output capacity is limited; we want reads in the window.
    let read_candidates: Vec<(&str, PathBuf)> = vec![
        ("read-rel-pkg",         PathBuf::from("package.json")),
        ("read-abs-pkg",         PathBuf::from("/package.json")),
        ("read-rel-pv",          PathBuf::from("crates/PARITY_VERSIONS.md")),
        ("read-abs-pv",          PathBuf::from("/crates/PARITY_VERSIONS.md")),
    ];
    for (label, path) in &read_candidates {
        let res = fs::read(path);
        let len = res.as_ref().ok().map(|b| b.len());
        let err = res.as_ref().err().map(|e| e.to_string());
        let ok = res.is_ok();
        eprintln!("[probe wasi-io] {} {:?} ok={} len={:?} err={:?}",
                  label, path, ok, len, err);
        attempts.push(serde_json::json!({
            "label": label, "kind": "read",
            "path":  path.display().to_string(), "ok": ok, "err": err, "len": len,
        }));
    }

    // List cwd to see what the sandbox exposes
    match fs::read_dir(".") {
        Ok(rd) => {
            let names: Vec<String> = rd.take(20).filter_map(|e| e.ok())
                .map(|e| e.file_name().to_string_lossy().to_string()).collect();
            eprintln!("[probe wasi-io] read_dir(.) => {} entries: {:?}", names.len(), names);
            attempts.push(serde_json::json!({ "label": "read-dir-dot", "kind": "list", "ok": true, "names": names }));
        }
        Err(e) => {
            eprintln!("[probe wasi-io] read_dir(.) ERR {}", e);
            attempts.push(serde_json::json!({ "label": "read-dir-dot", "kind": "list", "ok": false, "err": e.to_string() }));
        }
    }

    // WRITE second.
    let write_candidates: Vec<(&str, PathBuf)> = vec![
        ("write-rel-cwd",        PathBuf::from("probe-cwd.bin")),
        ("write-rel-dot",        PathBuf::from("./probe-dot.bin")),
        ("write-abs-root",       PathBuf::from("/probe-root.bin")),
        ("write-abs-callscr",    PathBuf::from(call_scratch).join("probe.bin")),
    ];
    for (label, path) in &write_candidates {
        let res = fs::write(path, b"x");
        eprintln!("[probe wasi-io] {} {:?} => {:?}", label, path, res);
        attempts.push(serde_json::json!({
            "label": label, "kind": "write",
            "path": path.display().to_string(),
            "ok":   res.is_ok(),
            "err":  res.err().map(|e| e.to_string()),
        }));
    }

    //   read attempts (hosts pre-creates a known file at <cwd>/package.json)
    let read_candidates: Vec<(&str, PathBuf)> = vec![
        ("read-rel-pkgjson",     PathBuf::from("package.json")),
        ("read-abs-pkgjson",     PathBuf::from("/package.json")),
        ("read-rel-callscr",     PathBuf::from(call_scratch).join("call-marker.txt")),
        ("read-rel-srcfile",     PathBuf::from("crates/PARITY_VERSIONS.md")),
    ];
    for (label, path) in &read_candidates {
        let res = fs::read(path);
        let len = res.as_ref().ok().map(|b| b.len());
        let err = res.as_ref().err().map(|e| e.to_string());
        let ok = res.is_ok();
        eprintln!("[probe wasi-io] {} {:?} => ok={} len={:?} err={:?}",
                  label, path, ok, len, err);
        attempts.push(serde_json::json!({
            "label": label,
            "path":  path.display().to_string(),
            "ok":    ok,
            "err":   err,
            "len":   len,
        }));
    }

    //   list cwd attempt
    let listing = fs::read_dir(".");
    let listing_summary = match &listing {
        Ok(_) => "ok",
        Err(e) => {
            eprintln!("[probe wasi-io] read_dir(.) err = {}", e);
            "err"
        }
    };
    eprintln!("[probe wasi-io] read_dir(.) => {}", listing_summary);

    // best-effort write of result somewhere the test can read it
    let result_bytes = serde_json::to_vec(&ProbeResult {
        probe: "wasi-io",
        ok: attempts.iter().any(|a| a["ok"].as_bool() == Some(true)),
        detail: serde_json::json!({
            "cwd_observed": cwd.ok().map(|p| p.display().to_string()),
            "attempts": attempts,
            "read_dir_dot": listing_summary,
        }),
    })
    .unwrap();
    for p in &[
        "phase0-probes/wasi-io-result.json",
        "/phase0-probes/wasi-io-result.json",
    ] {
        let _ = fs::write(p, &result_bytes);
    }
    let _ = fs::write(
        PathBuf::from(call_scratch).join("probe-result.json"),
        &result_bytes,
    );
}

fn run_wasi_mtime(call_scratch: &str) {
    let probe_path = PathBuf::from(call_scratch).join("probe-mtime.bin");
    let _ = fs::write(&probe_path, b"x");
    let mtime = fs::metadata(&probe_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos());
    write_result(
        call_scratch,
        &ProbeResult {
            probe: "wasi-mtime",
            ok: mtime.is_some_and(|n| n > 0),
            detail: serde_json::json!({ "mtime_ns": mtime.map(|n| n.to_string()) }),
        },
    );
}

fn run_instance_teardown(call_scratch: &str) {
    let observed = INSTANCE_LIVE_COUNTER.load(Ordering::SeqCst);
    INSTANCE_LIVE_COUNTER.store(observed + 1, Ordering::SeqCst);
    write_result(
        call_scratch,
        &ProbeResult {
            probe: "instance-teardown",
            ok: observed == 0,
            detail: serde_json::json!({ "observed_counter_on_entry": observed }),
        },
    );
}

fn run_scratch_reach(worker: &str, call: &str) {
    let worker_path = PathBuf::from(worker).join("probe-worker.bin");
    let call_path = PathBuf::from(call).join("probe-call.bin");
    let payload: &[u8] = &[1, 2, 3, 4, 5, 6, 7, 8];
    let worker_w = fs::write(&worker_path, payload);
    let worker_r = fs::read(&worker_path).ok();
    let call_w = fs::write(&call_path, payload);
    let call_r = fs::read(&call_path).ok();
    let ok = worker_w.is_ok()
        && call_w.is_ok()
        && worker_r.as_deref() == Some(payload)
        && call_r.as_deref() == Some(payload);
    write_result(
        call,
        &ProbeResult {
            probe: "scratch-reach",
            ok,
            detail: serde_json::json!({
                "worker_dir":   worker,
                "call_dir":     call,
                "worker_write": worker_w.is_ok(),
                "worker_read":  worker_r.is_some(),
                "call_write":   call_w.is_ok(),
                "call_read":    call_r.is_some(),
            }),
        },
    );
}

fn run_postcard_roundtrip(worker: &str) {
    let path = PathBuf::from(worker).join("probe.bin");
    let fixture = CacheFileMin {
        version: 1,
        schema_hash: [42; 32],
        layer2: vec![(0xDEAD_BEEF_CAFE, vec![1, 2, 3, 4])],
    };
    let bytes = postcard::to_allocvec(&fixture).expect("encode");
    fs::write(&path, &bytes).expect("write");
    let read = fs::read(&path).expect("read");
    let decoded: CacheFileMin = postcard::from_bytes(&read).expect("decode");
    let ok = decoded == fixture;
    // Postcard probe writes its result to the worker scratch since it
    // is itself testing worker-scratch durability.
    let result = ProbeResult {
        probe: "postcard-roundtrip",
        ok,
        detail: serde_json::json!({
            "encoded_bytes": bytes.len(),
        }),
    };
    let result_path = PathBuf::from(worker).join("probe-result.json");
    fs::write(&result_path, serde_json::to_vec(&result).unwrap()).expect("write result");
}

#[plugin_transform]
pub fn process(program: Program, meta: TransformPluginProgramMetadata) -> Program {
    let raw_config = meta
        .get_transform_plugin_config()
        .expect("plugin config absent");
    let cfg: ProbeConfig =
        serde_json::from_str(&raw_config).expect("plugin config not parseable");
    // Touch the metadata context once so warnings about unused don't fire.
    let _ = meta.get_context(&TransformPluginMetadataContextKind::Filename);
    match cfg {
        ProbeConfig::WasiIo { call_scratch } => run_wasi_io(&call_scratch),
        ProbeConfig::WasiMtime { call_scratch } => run_wasi_mtime(&call_scratch),
        ProbeConfig::InstanceTeardown { call_scratch } => run_instance_teardown(&call_scratch),
        ProbeConfig::ScratchReach {
            worker_scratch_dir,
            call_scratch,
        } => run_scratch_reach(&worker_scratch_dir, &call_scratch),
        ProbeConfig::PostcardRoundtrip { worker_scratch_dir } => {
            run_postcard_roundtrip(&worker_scratch_dir)
        }
    }
    program
}
