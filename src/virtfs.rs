// SPDX-License-Identifier: Apache-2.0

use std::any::Any;
use std::collections::HashMap;
use std::convert::TryInto;
use std::io::prelude::*;
use std::io::ErrorKind;
use std::path::{Component, Path};
use std::rc::Rc;
use wasi_common::virtfs::{FileContents, VirtualDirEntry};
use wasi_common::wasi::{types, Result};

#[derive(Debug, PartialEq)]
pub(crate) enum TarDirEntry {
    Directory(HashMap<String, TarDirEntry>),
    File(Box<TarFileContents>),
}

impl TarDirEntry {
    pub fn empty_directory() -> Self {
        Self::Directory(HashMap::new())
    }

    fn populate_directory(&mut self, path: impl AsRef<Path>) -> std::io::Result<&mut Self> {
        let mut dir = self;
        for component in path.as_ref().components() {
            let name = match component {
                Component::Normal(first) => {
                    first.to_str().ok_or(ErrorKind::InvalidInput)?.to_string()
                }
                _ => return Err(ErrorKind::InvalidInput.into()),
            };
            match dir {
                TarDirEntry::Directory(ref mut map) => {
                    if !map.contains_key(&name) {
                        map.insert(name.clone(), TarDirEntry::Directory(HashMap::new()));
                    }
                    dir = map.get_mut(&name).unwrap();
                }
                _ => unreachable!(),
            }
        }
        Ok(dir)
    }

    pub(crate) fn populate<R: Read>(
        &mut self,
        content: Rc<[u8]>,
        entry: &tar::Entry<R>,
    ) -> std::io::Result<()> {
        match entry.header().entry_type() {
            tar::EntryType::Regular => {
                let path = entry.header().path()?;
                let parent = {
                    if let Some(parent) = path.parent() {
                        self.populate_directory(parent)?
                    } else {
                        self
                    }
                };

                match parent {
                    TarDirEntry::Directory(ref mut map) => {
                        let name = path
                            .file_name()
                            .ok_or(ErrorKind::InvalidInput)?
                            .to_str()
                            .ok_or(ErrorKind::InvalidInput)?
                            .to_string();
                        let content = TarFileContents::new(content, entry.raw_file_position());
                        map.insert(name, TarDirEntry::File(Box::new(content)));
                    }
                    _ => unreachable!(),
                }
            }
            tar::EntryType::Directory => {
                let path = entry.header().path()?;
                let _ = self.populate_directory(path)?;
            }
            _ => {}
        }
        Ok(())
    }

    pub(crate) fn lookup(&self, path: impl AsRef<Path>) -> Option<&TarDirEntry> {
        let mut dir = self;
        if let Some(parent) = path.as_ref().parent() {
            for component in parent.components() {
                let name = match component {
                    Component::Normal(first) => Some(first.to_str()?.to_string()),
                    _ => None,
                }?;
                dir = if let TarDirEntry::Directory(ref map) = dir {
                    Some(map.get(&name)?)
                } else {
                    None
                }?;
            }
        }

        let name = path.as_ref().file_name()?.to_str()?.to_string();
        if let TarDirEntry::Directory(ref map) = dir {
            Some(map.get(&name)?)
        } else {
            None
        }
    }
}

impl Into<VirtualDirEntry> for TarDirEntry {
    fn into(self) -> VirtualDirEntry {
        match self {
            TarDirEntry::Directory(map) => {
                let mut virt = HashMap::new();
                for (name, entry) in map {
                    virt.insert(name, entry.into());
                }
                VirtualDirEntry::Directory(virt)
            }
            TarDirEntry::File(contents) => VirtualDirEntry::File(contents),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TarFileContents {
    content: Rc<[u8]>,
    offset: u64,
}

impl TarFileContents {
    fn new(content: Rc<[u8]>, offset: u64) -> Self {
        Self { content, offset }
    }

    pub(crate) fn as_any(&self) -> &dyn Any {
        self as &dyn Any
    }

    fn get_entry<'a, 'b>(
        entries: &'a mut tar::Entries<'a, &'b [u8]>,
        offset: u64,
    ) -> Result<tar::Entry<'a, &'b [u8]>> {
        let entry = entries
            .take_while(|e| e.is_ok())
            .map(|e| e.unwrap())
            .find(|e| e.raw_file_position() == offset);
        if let Some(entry) = entry {
            Ok(entry)
        } else {
            Err(types::Errno::Noent)
        }
    }

    fn try_size(&self) -> Result<types::Filesize> {
        let mut archive = tar::Archive::new(&*self.content);
        let mut entries = archive.entries()?;
        let entry = Self::get_entry(&mut entries, self.offset)?;
        let size = entry.header().size()?;
        Ok(size)
    }
}

impl FileContents for TarFileContents {
    fn max_size(&self) -> types::Filesize {
        std::usize::MAX as types::Filesize
    }

    fn size(&self) -> types::Filesize {
        self.try_size().unwrap_or(0)
    }

    fn resize(&mut self, _new_size: types::Filesize) -> Result<()> {
        Err(types::Errno::Inval)
    }

    fn preadv(&self, iovs: &mut [std::io::IoSliceMut], offset: types::Filesize) -> Result<usize> {
        let mut read_total = 0usize;
        for iov in iovs.iter_mut() {
            let skip: u64 = read_total.try_into().map_err(|_| types::Errno::Inval)?;
            let read = self.pread(iov, offset + skip)?;
            read_total = read_total.checked_add(read).expect("FileContents::preadv must not be called when reads could total to more bytes than the return value can hold");
        }
        Ok(read_total)
    }

    fn pwritev(&mut self, _iovs: &[std::io::IoSlice], _offset: types::Filesize) -> Result<usize> {
        Err(types::Errno::Inval)
    }

    fn pread(&self, buf: &mut [u8], offset: types::Filesize) -> Result<usize> {
        let mut archive = tar::Archive::new(&*self.content);
        let mut entries = archive.entries()?;
        let mut entry = Self::get_entry(&mut entries, self.offset)?;

        let offset: usize = offset.try_into().map_err(|_| types::Errno::Inval)?;

        let size: usize = entry.header().size()?.try_into()?;
        let data_remaining = size.saturating_sub(offset);

        let read_count = std::cmp::min(buf.len(), data_remaining);

        std::io::copy(&mut entry.by_ref().take(offset as _), &mut std::io::sink())?;
        entry.read_exact(&mut buf[..read_count])?;
        Ok(read_count)
    }

    fn pwrite(&mut self, _buf: &[u8], _offset: types::Filesize) -> Result<usize> {
        Err(types::Errno::Inval)
    }
}

#[cfg(test)]
pub(crate) mod test {
    use super::*;

    #[test]
    fn populate_lookup() {
        let mut builder = tar::Builder::new(Vec::new());
        builder
            .append_path_with_name("fixtures/bundle/config.yaml", "config.yaml")
            .unwrap();
        builder
            .append_path_with_name("fixtures/bundle/stdin.txt", "data/stdin.txt")
            .unwrap();
        builder.finish().unwrap();
        let content = builder.into_inner().unwrap();

        let mut root = TarDirEntry::empty_directory();
        let rc: Rc<[u8]> = content.into_boxed_slice().into();
        let content = rc.clone();
        let mut ar = tar::Archive::new(&*content);
        for entry in ar.entries().unwrap() {
            let entry = entry.unwrap();
            root.populate(rc.clone(), &entry).unwrap();
        }
        assert_eq!(root.lookup(Path::new("foo")), None);
        assert!(matches!(
            root.lookup(Path::new("config.yaml")),
            Some(TarDirEntry::File(_))
        ));
        assert!(matches!(
            root.lookup(Path::new("data")),
            Some(TarDirEntry::Directory(_))
        ));
        assert!(matches!(
            root.lookup(Path::new("data/stdin.txt")),
            Some(TarDirEntry::File(_))
        ));
    }
}
