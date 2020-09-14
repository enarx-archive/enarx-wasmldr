// SPDX-License-Identifier: Apache-2.0
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[serde(rename_all = "lowercase")]
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum ReadOnly {
    /// Discard the I/O
    Null,

    /// Inherit from the parent process
    Inherit,

    /// External file
    File(PathBuf),

    /// File bundled in the Wasm binary
    Bundle(PathBuf),
}

impl Default for ReadOnly {
    fn default() -> Self {
        Self::Inherit
    }
}

#[serde(rename_all = "lowercase")]
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum WriteOnly {
    /// Discard the I/O
    Null,

    /// Inherit from the parent process
    Inherit,

    /// External file
    File(PathBuf),
}

impl Default for WriteOnly {
    fn default() -> Self {
        Self::Inherit
    }
}

#[serde(default)]
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Stdio {
    pub stdin: ReadOnly,
    pub stdout: WriteOnly,
    pub stderr: WriteOnly,
}

#[serde(default)]
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Config {
    pub stdio: Stdio,
}
