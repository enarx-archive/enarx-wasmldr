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
//! $ RUST_LOG=enarx_wasmldr=info RUST_BACKTRACE=1 cargo run -- --module-on-fd=3 3< return_1.wasm
//! ```
//!
#![deny(missing_docs)]
#![deny(clippy::all)]

mod cli;
mod workload;

use anyhow::{Context, Result};
use cli::RunOptions;
use log::{debug, info};
use structopt::StructOpt;

use std::fs::File;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::io::FromRawFd;

// SAFETY: If opts.module_on_fd is Some(fd) we'll use File::from_raw_fd(fd),
// which is unsafe if something else is using that fd already. So this function
// is safe as long as it is called before anything else opens a file/socket/etc.
// (parse_module_fd() enforces fd >= 3, so we can ignore stdin/out/err.)
unsafe fn get_module_reader(opts: &RunOptions) -> Result<File> {
    #[cfg(unix)]
    if let Some(fd) = opts.module_on_fd {
        info!("reading module from fd {:?}", fd);
        return Ok(File::from_raw_fd(fd));
    };
    let path = opts.module.as_ref().expect("missing required arg");
    info!("reading module from {:?}", path);
    File::open(path).with_context(|| format!("failed opening {:?}", path))
}

fn main() -> Result<()> {
    // Initialize the logger, taking filtering and style settings from the
    // default env vars (RUST_LOG and RUST_LOG_STYLE).
    // The log target is the default target (stderr), so no files get opened.
    env_logger::Builder::from_default_env().init();

    info!("version {} starting up", env!("CARGO_PKG_VERSION"));

    debug!("parsing argv");
    let opts = cli::RunOptions::from_args();
    info!("opts: {:#?}", opts);

    // SAFETY: This is safe because we haven't opened anything else yet.
    let mut reader = unsafe { get_module_reader(&opts) }?;
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).context("loading module")?;

    // FUTURE: measure opts.envs, opts.args, opts.wasm_features, etc
    // FUTURE: fork() the workload off into a separate memory space?

    // TODO: configure wasmtime, stdio, etc.
    info!("running workload");
    let result = workload::run(bytes, opts.args, opts.envs).expect("Failed to run workload");
    info!("got result: {:#?}", result);
    // TODO: exit with the resulting code, if the result is a return code
    // FUTURE: produce attestation report here

    Ok(())
}
