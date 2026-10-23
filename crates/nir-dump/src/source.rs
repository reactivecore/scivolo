//! Collects `.nir` files from a single file, a directory tree or a jar.

use std::fs;
use std::io::{self, Read};
use std::path::Path;

pub struct Entry {
    pub name: String,
    pub data: Vec<u8>,
}

pub fn load(path: &Path) -> io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    if path.is_dir() {
        walk(path, &mut out)?;
    } else if path.extension().is_some_and(|e| e == "jar" || e == "zip") {
        read_jar(path, &mut out)?;
    } else {
        out.push(Entry { name: path.display().to_string(), data: fs::read(path)? });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<Entry>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "nir") {
            out.push(Entry { name: path.display().to_string(), data: fs::read(&path)? });
        }
    }
    Ok(())
}

fn read_jar(path: &Path, out: &mut Vec<Entry>) -> io::Result<()> {
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?).map_err(io::Error::other)?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(io::Error::other)?;
        let name = file.name().map_err(io::Error::other)?.into_owned();
        if !name.ends_with(".nir") {
            continue;
        }
        let mut data = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut data)?;
        out.push(Entry { name, data });
    }
    Ok(())
}
