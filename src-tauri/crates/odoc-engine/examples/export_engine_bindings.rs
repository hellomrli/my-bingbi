use std::fs;
use std::path::PathBuf;

fn main() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../src/lib/generated/engine.ts");
    fs::write(&output, odoc_engine::typescript_bindings())
        .expect("failed to export engine bindings");
}
