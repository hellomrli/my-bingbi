use std::fs;
use std::path::PathBuf;

fn main() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../src/lib/generated/store.ts");
    fs::write(&output, odoc_store::typescript_bindings())
        .expect("failed to export store TypeScript bindings");
    println!("exported {}", output.display());
}
