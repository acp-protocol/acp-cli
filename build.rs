//! Build script to copy schema files from acp-spec submodule to vendored location.
//!
//! This ensures the schemas are available for include_str!() at compile time
//! and get packaged into the crates.io release.

use std::fs;
use std::path::Path;

fn main() {
    let src_dir = Path::new("acp-spec/schemas/v1");
    let dst_dir = Path::new("schemas/v1");

    // Only copy if source (submodule) exists
    if src_dir.exists() {
        fs::create_dir_all(dst_dir).expect("Failed to create schemas/v1 directory");

        for entry in fs::read_dir(src_dir).expect("Failed to read acp-spec/schemas/v1") {
            let entry = entry.expect("Failed to read directory entry");
            let src_path = entry.path();

            if src_path.extension().map(|e| e == "json").unwrap_or(false) {
                let dst_path = dst_dir.join(entry.file_name());
                fs::copy(&src_path, &dst_path).expect("Failed to copy schema file");
            }
        }
    }

    // Tell Cargo to rerun build.rs if submodule schemas change
    println!("cargo:rerun-if-changed=acp-spec/schemas/v1");
}
