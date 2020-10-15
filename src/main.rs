// SPDX-License-Identifier: Apache-2.0

//! The Enarx Keep runtime binary.
//!
//! It can be used to run a Wasm file with given command-line
//! arguments and environment variables.
//!
//! ## Example invocation
//!
//! ```console
//! $ wat2wasm fixtures/return_1.wat
//! $ RUST_LOG=enarx_wasmldr=info RUST_BACKTRACE=1 cargo run return_1.wasm
//!     Finished dev [unoptimized + debuginfo] target(s) in 0.07s
//!      Running `target/x86_64-unknown-linux-musl/debug/enarx-wasmldr target/x86_64-unknown-linux-musl/debug/build/enarx-wasmldr-c374d181f6abdda0/out/fixtures/return_1.wasm`
//! [2020-09-10T17:56:18Z INFO  enarx_wasmldr] got result: [
//!         I32(
//!             1,
//!         ),
//!     ]
//! ```
//!
//! On Unix platforms, the command can also read the workload from the
//! file descriptor (3):
//! ```console
//! $ RUST_LOG=enarx_wasmldr=info RUST_BACKTRACE=1 cargo run 3< return_1.wasm
//! ```
//!
#![deny(missing_docs)]
#![deny(clippy::all)]

mod bundle;
mod config;
mod virtfs;
mod workload;

use cfg_if::cfg_if;
use log::info;

use std::fs::File;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::io::FromRawFd;

#[cfg(unix)]
const FD: std::os::unix::io::RawFd = 3;

fn main() {
    let _ = env_logger::try_init_from_env(env_logger::Env::default());

    // Skip the program name by default, but also skip the enarx-keepldr image
    // name if it happens to precede the regular command line arguments.
    let nskip = 1 + std::env::args()
        .take(1)
        .filter(|s| s.ends_with("enarx-keepldr"))
        .count();
    let mut args = std::env::args().skip(nskip);
    let vars = std::env::vars();

    let mut reader = if let Some(path) = args.next() {
        File::open(&path).expect("Unable to open file")
    } else {
        cfg_if! {
            if #[cfg(unix)] {
                unsafe { File::from_raw_fd(FD) }
            } else {
                unreachable!();
            }
        }
    };

    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .expect("Failed to load workload");

    let result = workload::run(&bytes, args, vars).expect("Failed to run workload");

    info!("got result: {:#?}", result);
}
