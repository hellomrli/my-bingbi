use std::fs;
use std::path::PathBuf;

fn main() {
    let output = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../src/lib/generated/kb.ts");
    fs::write(&output, odoc_kb::typescript_bindings())
        .expect("failed to export knowledge-base bindings");
    println!("exported {}", output.display());
}
