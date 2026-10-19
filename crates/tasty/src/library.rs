//! The TASTy files of a list of jars or of a library image: their package table, and each
//! file's bytes on request.

use crate::symbols::{PACKAGE_TABLE, package_table};
use libimage::Image;
use rustc_hash::FxHashMap;
use std::borrow::Cow;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

/// A file's bytes, and its symbol table if an image keeps one.
pub type Contents = (Cow<'static, [u8]>, Option<&'static [u8]>);

pub enum Library {
    Jars {
        jars: Vec<Archive>,
        /// Each `.tasty` path, e.g. `scala/Option.tasty`, and the first jar that has it.
        files: FxHashMap<String, usize>,
    },
    /// Mapped for the rest of the process; its files are borrowed.
    Image(&'static Image),
}

/// A jar, or a directory of class files.
pub enum Archive {
    Zip(ZipArchive<File>),
    Dir(PathBuf),
}

impl Library {
    /// The TASTy files of `jars`, each a jar or a directory.
    pub fn open(jars: &[PathBuf]) -> io::Result<Self> {
        let mut archives = Vec::new();
        let mut files = FxHashMap::default();
        for (i, jar) in jars.iter().enumerate() {
            let archive = if jar.is_dir() {
                let mut names = Vec::new();
                tasty_files(jar, "", &mut names)?;
                names.into_iter().for_each(|n| _ = files.entry(n).or_insert(i));
                Archive::Dir(jar.clone())
            } else {
                let zip = ZipArchive::new(File::open(jar)?).map_err(io::Error::other)?;
                for name in zip.file_names().filter(|n| n.ends_with(".tasty")) {
                    files.entry(name.to_owned()).or_insert(i);
                }
                Archive::Zip(zip)
            };
            archives.push(archive);
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

    /// The bytes of `path`, e.g. `scala/Option.tasty`, and the symbol table an image keeps of it.
    pub fn read(&mut self, path: &str) -> io::Result<Contents> {
        match self {
            Library::Jars { jars, files } => {
                let Some(&jar) = files.get(path) else { return Err(io::ErrorKind::NotFound.into()) };
                let bytes = match &mut jars[jar] {
                    Archive::Zip(zip) => {
                        let mut file = zip.by_name(path).map_err(io::Error::other)?;
                        let mut bytes = Vec::with_capacity(file.size() as usize);
                        file.read_to_end(&mut bytes)?;
                        bytes
                    }
                    Archive::Dir(dir) => std::fs::read(dir.join(path))?,
                };
                Ok((Cow::Owned(bytes), None))
            }
            Library::Image(image) => match image.find(path) {
                Some((data, table)) => Ok((Cow::Borrowed(data), table)),
                None => Err(io::ErrorKind::NotFound.into()),
            },
        }
    }
}

/// Adds the `.tasty` paths under `dir`/`prefix` to `names`, relative to `dir`.
fn tasty_files(dir: &Path, prefix: &str, names: &mut Vec<String>) -> io::Result<()> {
    for entry in std::fs::read_dir(dir.join(prefix))? {
        let entry = entry?;
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        if entry.file_type()?.is_dir() {
            tasty_files(dir, &format!("{name}/"), names)?;
        } else if name.ends_with(".tasty") {
            names.push(name);
        }
    }
    Ok(())
}
