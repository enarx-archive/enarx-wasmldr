// SPDX-License-Identifier: Apache-2.0

#![allow(missing_docs, unused_variables)] // This is a work-in-progress, so...

use anyhow::{bail, Result};
use lazy_static::lazy_static;
use std::{path::PathBuf, str::FromStr};
use structopt::{clap::AppSettings, StructOpt};

use crate::config::HandleFrom;

#[cfg(unix)]
use std::os::unix::io::RawFd;

// The main StructOpt for running `wasmldr` from the CLI
#[derive(StructOpt, Debug)]
#[structopt(
    name = "enarx-wasmtime",
    setting = AppSettings::DeriveDisplayOrder,
    setting = AppSettings::UnifiedHelpMessage,
)]
/// Enarx Keep Configurator and WebAssembly Loader
pub struct RunOptions {
    /// Set an environment variable inside the WASI context
    #[structopt(
        short = "e",
        long = "env",
        number_of_values = 1,
        value_name = "NAME=VAL",
        parse(try_from_str = parse_env_var),
    )]
    pub envs: Vec<(String, String)>,

    /// Pass all environment variables into the WASI context
    #[structopt(long)]
    pub inherit_env: bool,

    /// Name of the function to invoke
    #[structopt(long, value_name = "FUNCTION")]
    pub invoke: Option<String>,

    #[cfg(unix)]
    /// Load WebAssembly module from the given FD (must be >=3)
    #[structopt(long, value_name = "FD", parse(try_from_str = parse_module_fd))]
    pub module_on_fd: Option<RawFd>,

    #[structopt(flatten)]
    pub wasmtime: WasmtimeOptions,

    /// Filename of the WebAssembly module to load
    #[structopt(
        index = 1,
        required_unless = "module-on-fd",
        value_name = "MODULE",
        parse(from_os_str)
    )]
    pub module: Option<PathBuf>,

    /// Arguments to pass to the WebAssembly module.
    ///
    /// The '--' separator is required.
    #[structopt(value_name = "ARGS", last = true)]
    pub args: Vec<String>,
}

// Options that change the behavior of wasmtime

const SUPPORTED_WASM_FEATURES: &[(&str, &str)] = &[
    (
        "bulk-memory",
        "enables support for bulk memory instructions",
    ),
    (
        "module-linking",
        "enables support for the module-linking proposal",
    ),
    (
        "multi-memory",
        "enables support for the multi-memory proposal",
    ),
    ("multi-value", "enables support for multi-value functions"),
    ("reference-types", "enables support for reference types"),
    ("simd", "enables support for proposed SIMD instructions"),
    ("threads", "enables support for WebAssembly threads"),
];

lazy_static! {
    static ref FLAG_EXPLANATIONS: String = {
        use std::fmt::Write;
        let mut s = String::new();

        // Explain --wasm-features.
        writeln!(&mut s, "Enable or disable the named WebAssembly feature:").unwrap();
        let max = SUPPORTED_WASM_FEATURES.iter().max_by_key(|(name, _)| name.len()).unwrap();
        for (name, desc) in SUPPORTED_WASM_FEATURES.iter() {
            writeln!(&mut s, "  {:width$} {}", name, desc, width = max.0.len() + 2).unwrap();
        }
        writeln!(&mut s, "Prefix FEATURE with '-' to disable it.\n ").unwrap();

        s
    };
}

#[derive(StructOpt, Debug)]
pub struct WasmtimeOptions {
    /// Enable or disable WebAssembly features.
    #[structopt(
        long,
        value_name = "FEATURE,FEATURE,...",
        parse(try_from_str = parse_wasm_features),
        long_help = FLAG_EXPLANATIONS.as_ref()
    )]
    wasm_features: Option<wasmparser::WasmFeatures>,
}

// Parsing functions

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

fn parse_handle_from(s: &str) -> Result<HandleFrom> {
    match s {
        "null" => Ok(HandleFrom::Null),
        "inherit" => Ok(HandleFrom::Inherit),
        _ => Ok(HandleFrom::File(PathBuf::from_str(s)?)),
    }
}

fn parse_wasm_features(argstr: &str) -> Result<wasmparser::WasmFeatures> {
    let mut features = wasmparser::WasmFeatures::default();

    for s in argstr.trim().split(',') {
        if s.is_empty() {
            continue;
        }
        // Check for - prefix to disable features
        let (s, val) = if s.starts_with('-') {
            (&s[1..], false)
        } else {
            (s, true)
        };
        match s {
            "bulk-memory" => features.bulk_memory = val,
            "module-linking" => features.module_linking = val,
            "multi-memory" => features.multi_memory = val,
            "multi-value" => features.multi_value = val,
            "reference-types" => features.reference_types = val,
            "simd" => features.simd = val,
            "threads" => features.threads = val,
            _ => bail!("unknown feature {:?}", s),
        }
    }
    Ok(features)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn wasm_features_names() {
        for (name, _) in SUPPORTED_WASM_FEATURES {
            parse_wasm_features(name).unwrap();
        }
    }
    #[test]
    fn wasm_features_negate() {
        // check that bulk-memory is still enabled by default
        assert_eq!(parse_wasm_features("").unwrap().bulk_memory, true);
        assert_eq!(
            parse_wasm_features("-bulk-memory").unwrap().bulk_memory,
            false
        );
    }
}
