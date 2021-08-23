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
//! On Unix platforms, the command can also read the workload from an open file descriptor:
//! ```console
//! $ wat2wasm fixtures/hello_wasi_snapshot1.wat -o hello.wasm
//! $ RUST_LOG=enarx_wasmldr=info RUST_BACKTRACE=1 cargo run hello.wasm --module-on-fd 3 3<./hello.wasm
//! [2021-08-06T18:22:03Z INFO  enarx_wasmldr] version 0.2.0 starting up
//! [2021-08-06T18:22:03Z INFO  enarx_wasmldr] opts: RunOptions {
//!     envs: [],
//!     invoke: None,
//!     wasmtime: WasmtimeOptions {
//!         wasm_features: None,
//!     },
//!     module_on_fd: Some(
//!         3,
//!     ),
//!     module: "hello.wasm",
//!     args: [],
//! }
//! [2021-08-06T18:22:03Z INFO  enarx_wasmldr] reading "hello.wasm" from fd 3
//! [2021-08-06T18:22:03Z INFO  enarx_wasmldr] running workload
//! Hello, world!
//! [2021-08-06T18:22:03Z INFO  enarx_wasmldr] got result: []
//! ```

#![deny(missing_docs)]
#![deny(clippy::all)]

mod cli;
mod config;
mod workload;

use anyhow::{Context, Result};
use log::{debug, info};
use structopt::StructOpt;

use std::fs::File;
use std::io::Read;

#[cfg(unix)]
use std::os::unix::io::FromRawFd;

use cfg_if::cfg_if;

fn main() -> Result<()> {
    // Initialize the logger, taking settings from the default env vars
    env_logger::Builder::from_default_env().init();

    info!("version {} starting up", env!("CARGO_PKG_VERSION"));

    debug!("parsing argv");
    let opts = cli::RunOptions::from_args();
    info!("opts: {:#?}", opts);

    cfg_if! {
        if #[cfg(unix)] {
            let mut reader = match opts.module_on_fd {
                // SAFETY: unsafe if another struct is using the given fd.
                // Since we haven't opened any other files yet, we're OK.
                Some(fd) => {
                    info!("reading {:?} from fd {:?}", opts.module, fd);
                    unsafe { File::from_raw_fd(fd) }
                },
                None => {
                    info!("reading module from {:?}", opts.module);
                    File::open(&opts.module)
                        .with_context(|| format!("failed opening {:?}", opts.module))?
                },
            };
        } else {
            info!("reading module from {:?}", opts.module);
            let mut reader = File::open(&opts.module)
                .with_context(|| format!("failed opening {:?}", opts.module))?;
        }
    }

    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .expect("Failed to load workload");

    // FUTURE: measure opts.envs, opts.args, opts.wasm_features...
    // FUTURE: fork() the workload off into a separate memory space

    info!("running workload");
    // TODO: pass opts.wasm_features
    let result = workload::run(bytes, opts.args, opts.envs).expect("Failed to run workload");
    info!("got result: {:#?}", result);

    // FUTURE: produce attestation report here

    Ok(())
}
