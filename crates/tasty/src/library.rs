//! The TASTy files of a list of jars or of a library image: their paths, and each file's bytes
//! on request.

use libimage::Image;
use rustc_hash::FxHashMap;
use std::borrow::Cow;
use std::fs::File;
use std::io::{self, Read};
use std::path::PathBuf;
use zip::ZipArchive;

/// A file's bytes, and its symbol table if an image keeps one.
pub type Contents = (Cow<'static, [u8]>, Option<&'static [u8]>);

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

    /// Every `.tasty` path, in no particular order.
    pub fn paths(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        match self {
            Library::Jars { files, .. } => Box::new(files.keys().map(String::as_str)),
            Library::Image(image) => Box::new(image.names().filter(|n| n.ends_with(".tasty"))),
        }
    }

    /// The bytes of `path`, e.g. `scala/Option.tasty`, and the symbol table an image keeps of it.
    pub fn read(&mut self, path: &str) -> io::Result<Contents> {
        match self {
            Library::Jars { jars, files } => {
                let Some(&jar) = files.get(path) else { return Err(io::ErrorKind::NotFound.into()) };
                let mut file = jars[jar].by_name(path).map_err(io::Error::other)?;
                let mut bytes = Vec::with_capacity(file.size() as usize);
                file.read_to_end(&mut bytes)?;
                Ok((Cow::Owned(bytes), None))
            }
            Library::Image(image) => match image.find(path) {
                Some((data, table)) => Ok((Cow::Borrowed(data), table)),
                None => Err(io::ErrorKind::NotFound.into()),
            },
        }
    }
}
