//! The TASTy files of a list of jars or of a library image: their package table, and each
//! file's bytes on request.

use crate::symbols::{PACKAGE_TABLE, package_table};
use libimage::Image;
use rustc_hash::FxHashMap;
use std::borrow::Cow;
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use zip::ZipArchive;

pub enum Library {
    Jars {
        jars: Vec<ZipArchive<File>>,
        /// Each `.tasty` path, e.g. `scala/Option.tasty`, and the first jar that has it.
        files: FxHashMap<String, usize>,
    },
    /// Mapped for the rest of the process; its files are borrowed.
    Image(&'static Image),
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
        Ok(Self::Jars { jars: archives, files })
    }

    /// The package table of the TASTy files (`package_table`): the image's, else built.
    pub fn package_table(&self) -> Cow<'static, str> {
        let names: Vec<&str> = match self {
            Library::Jars { files, .. } => files.keys().map(String::as_str).collect(),
            Library::Image(image) => match image.find(PACKAGE_TABLE).map(|(t, _)| std::str::from_utf8(t)) {
                Some(Ok(table)) => return Cow::Borrowed(table),
                _ => image.names().collect(),
            },
        };
        Cow::Owned(package_table(&names))
    }

    /// The bytes of `path`, e.g. `scala/Option.tasty`.
    pub fn read(&mut self, path: &str) -> io::Result<Cow<'static, [u8]>> {
        match self {
            Library::Jars { jars, files } => {
                let Some(&jar) = files.get(path) else { return Err(io::ErrorKind::NotFound.into()) };
                let mut file = jars[jar].by_name(path).map_err(io::Error::other)?;
                let mut bytes = Vec::with_capacity(file.size() as usize);
                file.read_to_end(&mut bytes)?;
                Ok(Cow::Owned(bytes))
            }
            Library::Image(image) => match image.find(path) {
                Some((data, _)) => Ok(Cow::Borrowed(data)),
                None => Err(io::ErrorKind::NotFound.into()),
            },
        }
    }
}
