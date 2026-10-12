//! A store's files held in memory.
//!
//! A browser can fetch a file but not walk a directory, and historica's
//! [`Filesystem`] is synchronous where every fetch is not. So the page fetches
//! the files first and hands them over whole, and the store reads them from
//! here. Directories are not stored: a directory is any path some file sits
//! beneath.
//!
//! Read-only. The view writes nothing, and a write reaching this is a bug
//! worth an error rather than a file that vanishes with the tab.

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};

use historica::fs::{Entry, Filesystem, Kind};

/// Files by absolute path.
#[derive(Debug, Default, Clone)]
pub struct Memory {
    files: BTreeMap<PathBuf, Vec<u8>>,
}

impl Memory {
    /// An empty folder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Place a file at `path`, replacing whatever was there.
    pub fn insert(&mut self, path: impl Into<PathBuf>, bytes: Vec<u8>) {
        self.files.insert(path.into(), bytes);
    }

    /// How many files are held.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether nothing is held.
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The files strictly beneath `directory`, in path order.
    fn beneath<'a>(&'a self, directory: &'a Path) -> impl Iterator<Item = &'a PathBuf> + 'a {
        self.files
            .range(directory.to_path_buf()..)
            .map(|(path, _)| path)
            .take_while(move |path| path.starts_with(directory))
            .filter(move |path| path.as_path() != directory)
    }
}

fn read_only(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("{} is in a read-only view", path.display()),
    )
}

impl Filesystem for Memory {
    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }

    fn write(&self, path: &Path, _: &[u8]) -> io::Result<()> {
        Err(read_only(path))
    }

    fn create_new(&self, path: &Path, _: &[u8]) -> io::Result<()> {
        Err(read_only(path))
    }

    fn create_directory(&self, path: &Path) -> io::Result<()> {
        Err(read_only(path))
    }

    fn entries(&self, path: &Path) -> io::Result<Vec<Entry>> {
        if self.files.contains_key(path) {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                format!("{} is a file", path.display()),
            ));
        }
        let mut entries: BTreeMap<PathBuf, Kind> = BTreeMap::new();
        for file in self.beneath(path) {
            let Ok(rest) = file.strip_prefix(path) else {
                continue;
            };
            let mut components = rest.components();
            let Some(Component::Normal(first)) = components.next() else {
                continue;
            };
            let kind = if components.next().is_some() {
                Kind::Directory
            } else {
                Kind::File
            };
            entries.insert(path.join(first), kind);
        }
        if entries.is_empty() {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        }
        Ok(entries
            .into_iter()
            .map(|(path, kind)| Entry { path, kind })
            .collect())
    }

    fn look(&self, path: &Path) -> io::Result<Option<Kind>> {
        if self.files.contains_key(path) {
            return Ok(Some(Kind::File));
        }
        Ok(self.beneath(path).next().map(|_| Kind::Directory))
    }

    fn rename(&self, from: &Path, _: &Path) -> io::Result<()> {
        Err(read_only(from))
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        Err(read_only(path))
    }

    fn remove_directory(&self, path: &Path) -> io::Result<()> {
        Err(read_only(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> Memory {
        let mut memory = Memory::new();
        memory.insert("/s/a.txt", b"a".to_vec());
        memory.insert("/s/d/b.txt", b"b".to_vec());
        memory.insert("/s/d/e/c.txt", b"c".to_vec());
        memory.insert("/sibling.txt", b"x".to_vec());
        memory
    }

    #[test]
    fn a_directory_lists_its_children_once_each() {
        let entries = folder().entries(Path::new("/s")).unwrap();
        let listed: Vec<_> = entries
            .iter()
            .map(|entry| (entry.path.to_str().unwrap(), entry.kind))
            .collect();
        assert_eq!(
            listed,
            [("/s/a.txt", Kind::File), ("/s/d", Kind::Directory)]
        );
    }

    #[test]
    fn a_prefix_of_a_name_is_not_a_directory() {
        // `/s` is a prefix of `/sibling.txt` as a string and not as a path.
        let entries = folder().entries(Path::new("/s")).unwrap();
        assert!(entries.iter().all(|entry| entry.path.starts_with("/s")));
    }

    #[test]
    fn look_tells_files_from_directories_from_nothing() {
        let memory = folder();
        assert_eq!(
            memory.look(Path::new("/s/d")).unwrap(),
            Some(Kind::Directory)
        );
        assert_eq!(
            memory.look(Path::new("/s/a.txt")).unwrap(),
            Some(Kind::File)
        );
        assert_eq!(memory.look(Path::new("/s/nope")).unwrap(), None);
        assert_eq!(
            memory.entries(Path::new("/s/nope")).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }
}
