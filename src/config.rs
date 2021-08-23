// SPDX-License-Identifier: Apache-2.0
use std::path::PathBuf;

#[derive(Debug)]
pub enum HandleFrom {
    Null,
    Inherit,
    File(PathBuf),
}

impl Default for HandleFrom {
    fn default() -> Self {
        HandleFrom::Null
    }
}

#[derive(Default, Debug)]
pub(crate) struct DeployConfig {
    pub stdin: HandleFrom,
    pub stdout: HandleFrom,
    pub stderr: HandleFrom,
}
