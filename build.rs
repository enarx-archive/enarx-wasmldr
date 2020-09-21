// SPDX-License-Identifier: Apache-2.0

use std::path::Path;
use std::process::Command;

fn main() {
    let in_dir = Path::new("fixtures");
    let out_dir =
        std::env::var_os("OUT_DIR").expect("The OUT_DIR environment variable must be set");
    let out_dir = Path::new(&out_dir).join("fixtures");
    std::fs::create_dir_all(&out_dir).expect("Can't create output directory");

    for entry in in_dir.read_dir().unwrap() {
        if let Ok(entry) = entry {
            let wat = entry.path();
            match wat.extension() {
                Some(ext) if ext == "wat" => {
                    let wasm = out_dir
                        .join(wat.file_name().unwrap())
                        .with_extension("wasm");
                    let binary = wat::parse_file(&wat).expect("Can't parse wat file");
                    std::fs::write(&wasm, &binary).expect("Can't write wasm file");
                    // If the "enarx" command is installed, create a
                    // bundled Wasm file for testing.
                    let status = Command::new("enarx")
                        .arg("wasm")
                        .arg("bundle")
                        .arg("fixtures/bundle")
                        .arg(&wasm)
                        .arg(&wasm.with_extension("bundled.wasm"))
                        .status();

                    if let Ok(status) = status {
                        if !status.success() {
                            println!(
                                "cargo:warning=Error bundling resources for {:?}: {}",
                                &wasm.file_name().unwrap(),
                                status.code().unwrap()
                            );
                        } else {
                            println!("cargo:rustc-cfg=bundle_tests");
                        }
                    } else {
                        println!(
                            "cargo:warning=Not bundling resources for {:?}",
                            &wasm.file_name().unwrap()
                        );
                    }

                    println!("cargo:rerun-if-changed={}", &wat.display());
                }
                _ => {}
            }
        }
    }
}
