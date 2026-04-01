#![allow(clippy::unwrap_used, clippy::expect_used)]
//! Generate Python and TypeScript SDK clients to disk.
//!
//!   cargo run -p argentor-cli --example generate_sdks
//!
//! Outputs to `./generated-sdks/python/` and `./generated-sdks/typescript/`.

use argentor_builtins::sdk_generator::SdkConfig;
use argentor_builtins::SdkGenerator;
use std::path::Path;

fn main() {
    let config = SdkConfig {
        base_url: "http://localhost:3000".to_string(),
        package_name: "argentor_client".to_string(),
        version: "0.1.0".to_string(),
        include_async: true,
        include_streaming: true,
    };

    let generator = SdkGenerator::new();
    let output_dir = Path::new("generated-sdks");

    // Python SDK
    let python = generator.generate_python(&config);
    let python_dir = output_dir.join("python");
    std::fs::create_dir_all(&python_dir).expect("Failed to create python dir");

    for file in &python.files {
        let path = python_dir.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&path, &file.content).expect("Failed to write python file");
        println!("  ✓ python/{}", file.path);
    }

    // TypeScript SDK
    let typescript = generator.generate_typescript(&config);
    let ts_dir = output_dir.join("typescript");
    std::fs::create_dir_all(&ts_dir).expect("Failed to create typescript dir");

    for file in &typescript.files {
        let path = ts_dir.join(&file.path);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&path, &file.content).expect("Failed to write typescript file");
        println!("  ✓ typescript/{}", file.path);
    }

    println!();
    println!("SDKs generated:");
    println!(
        "  Python:     {}/python/ ({} files)",
        output_dir.display(),
        python.files.len()
    );
    println!(
        "  TypeScript: {}/typescript/ ({} files)",
        output_dir.display(),
        typescript.files.len()
    );
    println!();
    println!("Install Python SDK:");
    println!("  cd generated-sdks/python && pip install -e .");
    println!();
    println!("Install TypeScript SDK:");
    println!("  cd generated-sdks/typescript && npm install && npm run build");
}
