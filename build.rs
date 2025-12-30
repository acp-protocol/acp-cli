//! Build script to copy files from acp-spec submodule to vendored locations.
//!
//! This ensures schemas and primers are available for include_str!() at compile time
//! and get packaged into the crates.io release.

use std::fs;
use std::path::Path;

fn copy_json_files(src_dir: &Path, dst_dir: &Path) {
    if src_dir.exists() {
        fs::create_dir_all(dst_dir)
            .unwrap_or_else(|e| panic!("Failed to create {:?} directory: {}", dst_dir, e));

        for entry in
            fs::read_dir(src_dir).unwrap_or_else(|e| panic!("Failed to read {:?}: {}", src_dir, e))
        {
            let entry = entry.expect("Failed to read directory entry");
            let src_path = entry.path();

            if src_path.extension().map(|e| e == "json").unwrap_or(false) {
                let dst_path = dst_dir.join(entry.file_name());
                fs::copy(&src_path, &dst_path).unwrap_or_else(|e| {
                    panic!("Failed to copy {:?} to {:?}: {}", src_path, dst_path, e)
                });
            }
        }
    }
}

fn main() {
    // Copy schema files
    copy_json_files(Path::new("acp-spec/schemas/v1"), Path::new("schemas/v1"));

    // Copy primer files
    copy_json_files(Path::new("acp-spec/primers"), Path::new("primers"));

    // Tell Cargo to rerun build.rs if submodule files change
    println!("cargo:rerun-if-changed=acp-spec/schemas/v1");
    println!("cargo:rerun-if-changed=acp-spec/primers");
}
