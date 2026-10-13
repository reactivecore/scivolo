//! Finds the `.nir` file of a top-level name in directories, jars and the library image.

use crate::member_table::Table;
use libimage::Image;
use std::borrow::Cow;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

enum Root {
    Dir(PathBuf),
    Jar(ZipArchive<File>),
    /// Mapped for the rest of the process, so its files are borrowed without copying.
    Image(&'static Image),
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

    /// `paths`, then the library files of `image`.
    pub fn with_image(paths: &[PathBuf], image: &'static Image) -> io::Result<Self> {
        let mut classpath = Self::open(paths)?;
        classpath.roots.push(Root::Image(image));
        Ok(classpath)
    }

    /// The distinct top-level names defined on the classpath that end in `suffix`, sorted.
    pub fn tops_ending(&mut self, suffix: &str) -> Vec<String> {
        let file_suffix = format!("{suffix}.nir");
        let mut files = Vec::new();
        keep_errno(|| {
            for root in &mut self.roots {
                match root {
                    Root::Dir(dir) => walk(dir, dir, &mut files),
                    Root::Jar(zip) => files.extend(zip.file_names().map(str::to_owned)),
                    Root::Image(image) => {
                        files.extend(image.names().filter(|n| n.ends_with(&file_suffix)).map(str::to_owned))
                    }
                }
            }
        });
        let mut tops: Vec<String> = files
            .iter()
            .filter_map(|f| f.strip_suffix(".nir").filter(|_| f.ends_with(&file_suffix)))
            .map(|f| f.replace('/', "."))
            .collect();
        tops.sort();
        tops.dedup();
        tops
    }

    /// The resource files of the directories and jars, as root index and path; the first of
    /// equal paths wins. Left out, as Scala Native's resource embedding leaves them out:
    /// `META-INF/`, `scala-native/`, license and build files, classes, NIR, TASTy, sources.
    pub fn resources(&mut self) -> Vec<(usize, String)> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        keep_errno(|| {
            for (i, root) in self.roots.iter_mut().enumerate() {
                let mut names = Vec::new();
                match root {
                    Root::Dir(dir) => walk(dir, dir, &mut names),
                    Root::Jar(zip) => {
                        names.extend(zip.file_names().filter(|n| !n.ends_with('/')).map(str::to_owned))
                    }
                    Root::Image(_) => continue,
                }
                out.extend(
                    names.into_iter().filter(|n| is_resource(n) && seen.insert(n.clone())).map(|n| (i, n)),
                );
            }
        });
        out
    }

    /// The contents of file `rel` in root `root`, one of `resources`.
    pub fn read_at(&mut self, root: usize, rel: &str) -> Option<Vec<u8>> {
        keep_errno(|| read(self.roots.get_mut(root)?, rel))
    }

    /// The contents of file `rel` in each directory and jar that has one, in classpath order.
    pub fn all(&mut self, rel: &str) -> Vec<Vec<u8>> {
        keep_errno(|| {
            self.roots
                .iter_mut()
                .filter(|r| !matches!(r, Root::Image(_)))
                .filter_map(|r| read(r, rel))
                .collect()
        })
    }

    /// The bytes of the file defining `top`, e.g. `scala.Predef$` → `scala/Predef$.nir`.
    pub fn find(&mut self, top: &str) -> Option<(Cow<'static, [u8]>, Option<Table>)> {
        let rel = format!("{}.nir", top.replace('.', "/"));
        keep_errno(|| {
            self.roots.iter_mut().find_map(|root| match root {
                Root::Image(image) => {
                    image.find(&rel).map(|(data, table)| (Cow::Borrowed(data), table.map(Table)))
                }
                root => read(root, &rel).map(|data| (Cow::Owned(data), None)),
            })
        })
    }
}

/// The member table the library image keeps for NIR file `rel` with contents `data`; empty for
/// other files and files without one.
pub fn nir_table(rel: &str, data: &[u8]) -> Vec<u8> {
    let Some(top) = rel.strip_suffix(".nir") else { return Vec::new() };
    crate::member_table::build(data, &top.replace('/', ".")).unwrap_or_default()
}

/// `f`, leaving C's `errno` as it was.
fn keep_errno<T>(f: impl FnOnce() -> T) -> T {
    let errno = crate::natives::errno();
    let out = f();
    crate::natives::set_errno(errno);
    out
}

/// The paths of the files under `dir`, relative to `base` and `/`-separated, not following
/// symbolic links.
fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let (path, is_dir) = (entry.path(), entry.file_type().is_ok_and(|t| t.is_dir()));
        if is_dir {
            walk(base, &path, out);
        } else if let Ok(rel) = path.strip_prefix(base) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

fn is_resource(rel: &str) -> bool {
    const META: [&str; 6] =
        ["LICENSE", "NOTICE", "CONTRIBUTING", "library.properties", "BUILD", "rootdoc.txt"];
    const SOURCES: [&str; 6] = [".class", ".tasty", ".nir", ".jar", ".scala", ".java"];
    !(rel.starts_with("META-INF/")
        || rel.starts_with("scala-native/")
        || META.contains(&rel)
        || SOURCES.iter().any(|ext| rel.ends_with(ext)))
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
        Root::Image(image) => image.find(rel).map(|(data, _)| data.to_vec()),
    }
}
