// Standard napi-rs build wiring — emits the platform-appropriate
// linker flags so `cdylib` produces a `.node`-loadable artifact.
extern crate napi_build;

fn main() {
    napi_build::setup();
}
