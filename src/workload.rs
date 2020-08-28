// SPDX-License-Identifier: Apache-2.0

use crate::config::Handle;
use crate::virtfs::TarDirEntry;
use std::convert::TryFrom;
use std::rc::Rc;
use wasi_common::virtfs::FileContents;

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
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::IoError(err)
    }
}

/// Result type used throughout the library.
pub type Result<T> = std::result::Result<T, Error>;

fn populate_virtfs(root: &mut TarDirEntry, bytes: &[u8]) -> Result<()> {
    crate::bundle::parse(
        bytes,
        crate::bundle::RESOURCES_SECTION,
        |data| -> std::io::Result<()> {
            let mut buf = Vec::new();
            buf.resize(data.len(), 0u8);
            buf.copy_from_slice(data);
            let rc = Rc::new(buf);
            let mut ar = tar::Archive::new(rc.as_slice());
            for entry in ar.entries()? {
                let entry = entry?;
                root.populate(rc.clone(), &entry)?;
            }
            Ok(())
        },
        |_| Ok(()),
    )?;
    Ok(())
}

/// Runs a WebAssembly workload.
pub fn run<T: AsRef<[u8]>, U: AsRef<[u8]>, V: std::borrow::Borrow<(U, U)>>(
    bytes: impl AsRef<[u8]>,
    args: impl IntoIterator<Item = T>,
    envs: impl IntoIterator<Item = V>,
) -> Result<Box<[wasmtime::Val]>> {
    let mut config = wasmtime::Config::new();
    // Prefer dynamic memory allocation style over static memory
    config.static_memory_maximum_size(0);
    let engine = wasmtime::Engine::new(&config);
    let store = wasmtime::Store::new(&engine);
    let mut linker = wasmtime::Linker::new(&store);

    // Instantiate WASI.
    let mut builder = wasi_common::WasiCtxBuilder::new();
    builder.args(args).envs(envs);
    let mut root = TarDirEntry::empty_directory();
    populate_virtfs(&mut root, bytes.as_ref())?;

    // Read deployment configuration from the bundled resource.
    let mut deploy_config: Option<crate::config::Config> = None;
    match root {
        TarDirEntry::Directory(ref map) => {
            if let Some(config) = map.get("config.yaml") {
                if let TarDirEntry::File(ref content) = config {
                    let mut buf = Vec::new();
                    buf.resize(content.size() as usize, 0u8);
                    let mut len = 0usize;
                    loop {
                        let n = content
                            .pread(&mut buf[len..], len as u64)
                            .or(Err(Error::InstantiationFailed))?;
                        if n == 0 {
                            break;
                        }
                        len += n;
                        buf.extend((0..len * 2).map(|_| 0u8));
                    }
                    let config =
                        serde_yaml::from_slice(&buf[..len]).or(Err(Error::InstantiationFailed))?;
                    deploy_config.replace(config);
                }
            }
        }
        _ => unreachable!(),
    };

    // Associate stdin handles according to the deployment configuration.
    if let Some(deploy_config) = deploy_config {
        match deploy_config.stdio.stdin {
            Some(Handle::Inherit) => {
                builder.inherit_stdin();
            }
            Some(Handle::File(path)) => {
                let file = std::fs::OpenOptions::new().read(true).open(&path)?;
                builder.stdin(wasi_common::OsFile::try_from(file)?);
            }
            Some(Handle::Bundle(path)) => {
                let entry = root.lookup(&path).ok_or(Error::ConfigurationError)?;
                match entry {
                    TarDirEntry::Directory(_) => return Err(Error::ConfigurationError),
                    TarDirEntry::File(file) => {
                        if let Some(file) = file
                            .as_any()
                            .downcast_ref::<crate::virtfs::TarFileContents>()
                        {
                            builder.stdin(wasi_common::virtfs::InMemoryFile::new(Box::new(
                                file.clone(),
                            )));
                        }
                    }
                }
            }
            _ => {}
        }
        match deploy_config.stdio.stdout {
            Some(Handle::Inherit) => {
                builder.inherit_stdout();
            }
            Some(Handle::File(path)) => {
                let file = std::fs::OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(&path)?;
                builder.stdout(wasi_common::OsFile::try_from(file)?);
            }
            _ => {}
        }
        match deploy_config.stdio.stderr {
            Some(Handle::Inherit) => {
                builder.inherit_stderr();
            }
            Some(Handle::File(path)) => {
                let file = std::fs::OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .open(&path)?;
                builder.stderr(wasi_common::OsFile::try_from(file)?);
            }
            _ => {}
        }
    }
    builder.preopened_virt(root.into(), ".");

    let ctx = builder.build().or(Err(Error::InstantiationFailed))?;
    let wasi = wasmtime_wasi::Wasi::new(linker.store(), ctx);
    wasi.add_to_linker(&mut linker)
        .or(Err(Error::InstantiationFailed))?;

    // Instantiate the command module.
    let module = wasmtime::Module::from_binary(&linker.store().engine(), bytes.as_ref())
        .or(Err(Error::InstantiationFailed))?;
    linker
        .module("", &module)
        .or(Err(Error::InstantiationFailed))?;

    let function = linker.get_default("").or(Err(Error::ExportNotFound))?;

    // Invoke the function.
    function.call(Default::default()).or(Err(Error::CallFailed))
}

#[cfg(test)]
pub(crate) mod test {
    use crate::workload;
    use std::iter::empty;

    #[test]
    fn workload_run_return_1() {
        let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/return_1.wasm")).to_vec();

        let results: Vec<i32> = workload::run(&bytes, empty::<&str>(), empty::<(&str, &str)>())
            .unwrap()
            .iter()
            .map(|v| v.unwrap_i32())
            .collect();

        assert_eq!(results, vec![1]);
    }

    #[test]
    fn workload_run_no_export() {
        let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/fixtures/no_export.wasm")).to_vec();

        match workload::run(&bytes, empty::<&str>(), empty::<(&str, &str)>()) {
            Err(workload::Error::ExportNotFound) => {}
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
            empty::<(&str, &str)>(),
        )
        .unwrap()
        .iter()
        .map(|v| v.unwrap_i32())
        .collect();

        assert_eq!(results, vec![3]);
    }
}
