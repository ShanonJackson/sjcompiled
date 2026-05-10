// `napi-build` wires the cdylib name + symbol exports for napi-rs's
// runtime loader. Same boilerplate as `crates/compiled-css-napi/build.rs`.
fn main() {
    napi_build::setup();
}
