//! The TASTy files of a list of jars: their paths, and each file's bytes on request.

use rustc_hash::FxHashMap;
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use zip::ZipArchive;

pub struct Library {
    jars: Vec<ZipArchive<File>>,
    /// Each `.tasty` path, e.g. `scala/Option.tasty`, and the first jar that has it.
    files: FxHashMap<String, usize>,
}

impl Library {
    pub fn open(jars: &[PathBuf]) -> io::Result<Self> {
        let mut archives = Vec::new();
        let mut files = FxHashMap::default();
        for (i, jar) in jars.iter().enumerate() {
            let zip = ZipArchive::new(File::open(jar)?).map_err(io::Error::other)?;
            for name in zip.file_names().filter(|n| n.ends_with(".tasty")) {
                files.entry(name.to_owned()).or_insert(i);
            }
            archives.push(zip);
        }
        Ok(Self { jars: archives, files })
    }

    /// Every `.tasty` path, in no particular order.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.keys().map(String::as_str)
    }

    /// The bytes of `path`, e.g. `scala/Option.tasty`.
    pub fn read(&mut self, path: &str) -> io::Result<Vec<u8>> {
        let Some(&jar) = self.files.get(path) else { return Err(io::ErrorKind::NotFound.into()) };
        let mut file = self.jars[jar].by_name(path).map_err(io::Error::other)?;
        let mut bytes = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}
