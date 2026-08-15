use std::fs;
use std::path::PathBuf;

fn main() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../src/lib/generated/pack.ts");
    fs::write(&output, odoc_pack::typescript_bindings())
        .expect("failed to export TypeScript pack bindings");
}
