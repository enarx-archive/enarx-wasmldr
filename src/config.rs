// SPDX-License-Identifier: Apache-2.0
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Handle {
    /// Inherit from the parent process
    Inherit,

    /// External file
    File(PathBuf),

    /// File bundled in the Wasm binary
    Bundle(PathBuf),
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Stdio {
    pub stdin: Option<Handle>,
    pub stdout: Option<Handle>,
    pub stderr: Option<Handle>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub stdio: Stdio,
}
