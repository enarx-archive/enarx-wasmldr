// SPDX-License-Identifier: Apache-2.0

use crate::config::{DeployConfig, HandleFrom};
use anyhow::{bail, Context, Result};
use log::debug;
use wasmtime_wasi::sync::WasiCtxBuilder;

/// The error codes of workload execution.
#[derive(Debug)]
pub enum Error {
    /// configuration error
    ConfigurationError,
    /// export not found
    ExportNotFound,
    /// module instantiation failed
    InstantiationFailed,
    /// call failed
    CallFailed,
    /// I/O error
    IoError(std::io::Error),
    /// WASI error
    WASIError(wasmtime_wasi::Error),
    /// Arguments or environment too large
    StringTableError,
}

use std::fmt;

/* FIXME: either implement this properly *or* just use anyhow .context */
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "placeholder impl")
    }
}

/// Runs a WebAssembly workload.
pub fn run<T: AsRef<str>, U: AsRef<str>>(
    bytes: impl AsRef<[u8]>,
    args: impl IntoIterator<Item = T>,
    envs: impl IntoIterator<Item = (U, U)>,
) -> Result<Box<[wasmtime::Val]>> {
    let mut wasmconfig = wasmtime::Config::new();
    // FIXME: get features from CLI / config object
    // Support module-linking (https://github.com/webassembly/module-linking)
    wasmconfig.wasm_module_linking(true);
    // module-linking requires multi-memory
    wasmconfig.wasm_multi_memory(true);
    // Prefer dynamic memory allocation style over static memory
    wasmconfig.static_memory_maximum_size(0);

    let engine = wasmtime::Engine::new(&wasmconfig).context("configuring engine")?;

    // Set up linker and link WASI into engine
    let mut linker = wasmtime::Linker::new(&engine);
    wasmtime_wasi::add_to_linker(&mut linker, |s| s).context("adding WASI")?;

    // Add args and envs to the WasiCtx
    let mut wasi = WasiCtxBuilder::new();
    for arg in args {
        wasi = wasi
            .arg(arg.as_ref())
            .context(Error::StringTableError)
            .context("adding args")?;
    }
    for kv in envs {
        wasi = wasi
            .env(kv.0.as_ref(), kv.1.as_ref())
            .context(Error::StringTableError)
            .context("adding envs")?;
    }

    // TODO: get this config from the caller.. set up filehandles & sockets, etc etc
    let deploy_config = DeployConfig {
        stdin: HandleFrom::Inherit,
        stdout: HandleFrom::Inherit,
        stderr: HandleFrom::Inherit,
    };
    match deploy_config.stdin {
        HandleFrom::File(path) => {
            bail!("HandleFrom::File() not implemented")
        }
        HandleFrom::Inherit => {
            wasi = wasi.stdin(Box::new(wasmtime_wasi::stdio::stdin()));
        }
        HandleFrom::Null => {}
    };

    match deploy_config.stdout {
        HandleFrom::File(path) => {
            bail!("HandleFrom::File() not implemented")
        }
        HandleFrom::Inherit => {
            wasi = wasi.stdout(Box::new(wasmtime_wasi::stdio::stdout()));
        }
        HandleFrom::Null => {}
    };

    match deploy_config.stderr {
        HandleFrom::File(path) => {
            bail!("HandleFrom::File() not implemented")
        }
        HandleFrom::Inherit => {
            wasi = wasi.stderr(Box::new(wasmtime_wasi::stdio::stderr()));
        }
        HandleFrom::Null => {}
    };

    let mut store = wasmtime::Store::new(&engine, wasi.build());
    let module =
        wasmtime::Module::from_binary(&engine, bytes.as_ref()).context("parsing module")?;
    linker
        .module(&mut store, "", &module)
        .context("instantiation failed")?;

    // TODO: use the --invoke FUNCTION name, if any
    let func = linker
        .get_default(&mut store, "")
        .context(Error::ExportNotFound)
        .context("export not found")?;

    func.call(store, Default::default())
        .context(Error::CallFailed)
}

#[cfg(test)]
pub(crate) mod test {
    use crate::workload;
    use std::iter::empty;

    #[test]
    fn workload_run_return_1() {
        let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/return_1.wasm")).to_vec();

        let results: Vec<i32> =
            workload::run(&bytes, empty::<String>(), empty::<(String, String)>())
                .unwrap()
                .iter()
                .map(|v| v.unwrap_i32())
                .collect();

        assert_eq!(results, vec![1]);
    }

    #[test]
    fn workload_run_no_export() {
        let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/no_export.wasm")).to_vec();
        let err =
            workload::run(&bytes, empty::<String>(), empty::<(String, String)>()).unwrap_err();
        match err.downcast_ref::<workload::Error>() {
            Some(workload::Error::ExportNotFound) => {}
            _ => panic!("unexpected error"),
        };
        /* Not a great way to check errors, but let's be sure it works */
        match err.to_string().as_str() {
            "export not found" => {}
            _ => panic!("unexpected error"),
        };
    }

    #[test]
    fn workload_run_wasi_snapshot1() {
        let bytes =
            include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/wasi_snapshot1.wasm")).to_vec();

        let results: Vec<i32> = workload::run(
            &bytes,
            vec!["a".to_string(), "b".to_string(), "c".to_string()],
            vec![("k", "v")],
        )
        .unwrap()
        .iter()
        .map(|v| v.unwrap_i32())
        .collect();

        assert_eq!(results, vec![3]);
    }

    #[cfg(bundle_tests)]
    #[test]
    fn workload_run_bundled() {
        let bytes = include_bytes!(concat!(
            env!("OUT_DIR"),
            "/fixtures/hello_wasi_snapshot1.bundled.wasm"
        ))
        .to_vec();

        workload::run(&bytes, empty::<&str>(), empty::<(&str, &str)>()).unwrap();

        let output = std::fs::read("stdout.txt").unwrap();
        assert_eq!(output, "Hello, world!\n".to_string().into_bytes());
    }
}
