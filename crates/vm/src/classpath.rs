//! Finds the `.nir` file of a top-level name in directories, jars and the library image.

use crate::image::Image;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

enum Root {
    Dir(PathBuf),
    Jar(ZipArchive<File>),
    Image(Image),
}

pub struct Classpath {
    roots: Vec<Root>,
}

impl Classpath {
    pub fn open(paths: &[PathBuf]) -> io::Result<Self> {
        let roots = paths
            .iter()
            .map(|p| match p.is_dir() {
                true => Ok(Root::Dir(p.clone())),
                false => ZipArchive::new(File::open(p)?).map(Root::Jar).map_err(io::Error::other),
            })
            .collect::<io::Result<_>>()?;
        Ok(Self { roots })
    }

    /// `paths`, then the library `jars` through their image in directory `cache`.
    pub fn with_image(paths: &[PathBuf], jars: &[PathBuf], cache: &Path) -> io::Result<Self> {
        let mut classpath = Self::open(paths)?;
        classpath.roots.push(Root::Image(Image::open(jars, cache)?));
        Ok(classpath)
    }

    /// The top-level names defined on the classpath that end in `suffix`, sorted.
    pub fn tops_ending(&mut self, suffix: &str) -> Vec<String> {
        let file_suffix = format!("{suffix}.nir");
        let mut files = Vec::new();
        let errno = crate::natives::errno();
        for root in &mut self.roots {
            match root {
                Root::Dir(dir) => walk(dir, dir, &mut files),
                Root::Jar(zip) => files.extend(zip.file_names().map(str::to_owned)),
                Root::Image(image) => files.extend(image.names().map(str::to_owned)),
            }
        }
        crate::natives::set_errno(errno);
        let mut tops: Vec<String> = files
            .iter()
            .filter_map(|f| f.strip_suffix(".nir").filter(|_| f.ends_with(&file_suffix)))
            .map(|f| f.replace('/', "."))
            .collect();
        tops.sort();
        tops
    }

    /// The bytes of the file defining `top`, e.g. `scala.Predef$` → `scala/Predef$.nir`; C's
    /// `errno` is left as it was.
    pub fn find(&mut self, top: &str) -> Option<Vec<u8>> {
        let rel = format!("{}.nir", top.replace('.', "/"));
        let errno = crate::natives::errno();
        let found = self.roots.iter_mut().find_map(|root| read(root, &rel));
        crate::natives::set_errno(errno);
        found
    }
}

/// The paths of the files under `dir`, relative to `base` and `/`-separated.
fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() {
            walk(base, &path, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn read(root: &mut Root, rel: &str) -> Option<Vec<u8>> {
    match root {
        Root::Dir(dir) => std::fs::read(Path::new(dir).join(rel)).ok(),
        Root::Jar(zip) => {
            let mut file = zip.by_name(rel).ok()?;
            let mut data = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut data).ok()?;
            Some(data)
        }
        Root::Image(image) => image.find(rel).map(<[u8]>::to_vec),
    }
}
