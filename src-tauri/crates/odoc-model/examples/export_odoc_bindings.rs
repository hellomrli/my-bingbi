use std::fs;
use std::path::PathBuf;

fn main() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../src/lib/generated/bindings.ts");
    fs::write(&output, odoc_model::typescript_bindings())
        .expect("failed to export TypeScript bindings");
    println!("exported {}", output.display());
}
