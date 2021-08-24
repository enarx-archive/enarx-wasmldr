// SPDX-License-Identifier: Apache-2.0

#![allow(missing_docs, unused_variables)] // This is a work-in-progress, so...

use anyhow::{bail, Result};
use structopt::{clap::AppSettings, StructOpt};

use std::path::PathBuf;
use std::str::FromStr;

#[cfg(unix)]
use std::os::unix::io::RawFd;

// The main StructOpt for CLI options
#[derive(StructOpt, Debug)]
#[structopt(
    setting = AppSettings::DeriveDisplayOrder,
    setting = AppSettings::UnifiedHelpMessage,
)]
/// Enarx Keep Configurator and WebAssembly Loader
pub struct RunOptions {
    /// Pass an environment variable to the program
    #[structopt(
        short = "e",
        long = "env",
        number_of_values = 1,
        value_name = "NAME=VAL",
        parse(try_from_str=parse_env_var),
    )]
    pub envs: Vec<(String, String)>,

    /// Name of the function to invoke
    #[structopt(long, value_name = "FUNCTION")]
    invoke: Option<String>,

    /// Load WebAssembly module from the given FD (must be >=3)
    #[cfg(unix)]
    #[structopt(long, value_name = "FD", parse(try_from_str = parse_module_fd))]
    pub module_on_fd: Option<RawFd>,

    // TODO: --inherit-env
    // TODO: --stdin, --stdout, --stderr
    /// Path of the WebAssembly module to run
    #[structopt(
        index = 1,
        required_unless = "module-on-fd",
        value_name = "MODULE",
        parse(from_os_str)
    )]
    pub module: Option<PathBuf>,

    /// Arguments to pass to the WebAssembly module
    #[structopt(value_name = "ARGS", last = true)]
    pub args: Vec<String>,
}

fn parse_env_var(s: &str) -> Result<(String, String)> {
    let parts: Vec<&str> = s.splitn(2, '=').collect();
    if parts.len() != 2 {
        bail!("must be of the form `NAME=VAL`");
    }
    Ok((parts[0].to_owned(), parts[1].to_owned()))
}

fn parse_module_fd(s: &str) -> Result<RawFd> {
    let fd = RawFd::from_str(s)?;
    if fd <= 2 {
        bail!("FD must be >= 3");
    }
    Ok(fd)
}
