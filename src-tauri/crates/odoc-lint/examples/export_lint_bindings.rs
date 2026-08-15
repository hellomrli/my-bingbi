use std::fs;
use std::path::PathBuf;

fn main() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../src/lib/generated/lint.ts");
    fs::write(&output, odoc_lint::typescript_bindings())
        .expect("failed to export TypeScript lint bindings");
    println!("exported {}", output.display());
}
