use std::fs;
use std::path::PathBuf;

fn main() {
    let output =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../src/lib/generated/llm.ts");
    fs::write(&output, odoc_llm::typescript_bindings())
        .expect("failed to export LLM TypeScript bindings");
    println!("exported {}", output.display());
}
