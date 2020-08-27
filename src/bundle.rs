// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]

use std::io::prelude::*;
use std::io::{ErrorKind, Read, Result, Write};
use std::path::{Path, PathBuf};
use wasmparser::{Chunk, Parser, Payload::*};

pub const RESOURCES_SECTION: &str = ".enarx.resources";

pub struct Builder {
    paths: Vec<PathBuf>,
    prefix: Option<String>,
    section: Option<String>,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            paths: Vec::new(),
            prefix: None,
            section: None,
        }
    }

    pub fn path(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.paths.push(path.as_ref().into());
        self
    }

    pub fn prefix(&mut self, prefix: &str) -> &mut Self {
        self.prefix = Some(prefix.to_string());
        self
    }

    pub fn section(&mut self, section: &str) -> &mut Self {
        self.section = Some(section.to_string());
        self
    }

    pub fn build(&mut self, input: impl Read, mut output: impl Write) -> Result<()> {
        let prefix = match &self.prefix {
            Some(prefix) => prefix.as_str(),
            None => "",
        };
        let mut archive = tempfile::tempfile()?;
        create_archive(&self.paths, prefix, &mut archive)?;

        let section = match &self.section {
            Some(section) => section.as_str(),
            None => RESOURCES_SECTION,
        };
        parse(input, section, |_| Ok(()), |bytes| output.write_all(bytes))?;
        append_archive(&mut output, section, &archive)?;
        Ok(())
    }
}

fn create_archive<P: AsRef<Path>>(
    paths: impl IntoIterator<Item = P>,
    prefix: &str,
    writer: &mut impl Write,
) -> Result<()> {
    let mut builder = tar::Builder::new(writer);

    for path in paths {
        for ancestor in path.as_ref().ancestors() {
            if ancestor == Path::new("") {
                break;
            }
            let metadata = std::fs::metadata(&ancestor)?;
            if !metadata.is_dir() && !metadata.is_file() {
                return Err(ErrorKind::InvalidInput.into());
            }
        }
        let name = path
            .as_ref()
            .strip_prefix(prefix)
            .or(Err(ErrorKind::InvalidInput))?;
        builder.append_path_with_name(&path, &name)?;
    }

    builder.finish()?;

    Ok(())
}

pub fn parse(
    mut input: impl Read,
    section: &str,
    mut handle_custom: impl FnMut(&[u8]) -> Result<()>,
    mut handle_default: impl FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    let mut buf = Vec::new();
    let mut parser = Parser::new(0);
    let mut eof = false;
    let mut stack = Vec::new();

    loop {
        let (payload, consumed) = match parser.parse(&buf, eof).or(Err(ErrorKind::InvalidInput))? {
            Chunk::NeedMoreData(hint) => {
                assert!(!eof); // otherwise an error would be returned

                // Use the hint to preallocate more space, then read
                // some more data into our buffer.
                //
                // Note that the buffer management here is not ideal,
                // but it's compact enough to fit in an example!
                let len = buf.len();
                buf.extend((0..hint).map(|_| 0u8));
                let n = input.read(&mut buf[len..])?;
                buf.truncate(len + n);
                eof = n == 0;
                continue;
            }

            Chunk::Parsed { consumed, payload } => (payload, consumed),
        };

        match payload {
            CustomSection { name, data, .. } => {
                if name == section {
                    handle_custom(data)?;
                } else {
                    handle_default(&buf[..consumed])?;
                }
            }
            // When parsing nested modules we need to switch which
            // `Parser` we're using.
            ModuleCodeSectionEntry {
                parser: subparser, ..
            } => {
                stack.push(parser);
                parser = subparser;
            }
            End => {
                if let Some(parent_parser) = stack.pop() {
                    parser = parent_parser;
                } else {
                    break;
                }
            }
            _ => {
                handle_default(&buf[..consumed])?;
            }
        }

        // once we're done processing the payload we can forget the
        // original.
        buf.drain(..consumed);
    }
    Ok(())
}

fn append_archive(
    writer: &mut impl Write,
    section: &str,
    mut archive: &std::fs::File,
) -> Result<()> {
    let mut header: Vec<u8> = Vec::new();
    let name = section.as_bytes();
    leb128::write::unsigned(&mut header, name.len() as u64)?;
    header.write_all(name)?;
    let size = archive.seek(std::io::SeekFrom::End(0))?;

    writer.write_all(&[0])?;
    leb128::write::unsigned(writer, size + header.len() as u64)?;
    writer.write_all(&header)?;

    let _ = archive.seek(std::io::SeekFrom::Start(0))?;
    loop {
        let mut buf = [0; 4096];
        let n = archive.read(&mut buf[..])?;

        if n == 0 {
            break;
        }

        writer.write_all(&buf[..n])?;
    }

    Ok(())
}
