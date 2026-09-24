use std::fs;
use std::path::{Path, PathBuf};

pub struct Source {
    pub path: PathBuf,
    pub text: String,
}

pub struct Corpus {
    pub name: String,
    pub files: Vec<Source>,
}

impl Corpus {
    pub fn load(root: &Path) -> Corpus {
        let mut paths = Vec::new();
        collect(root, &mut paths);
        paths.sort();
        let files = paths
            .into_iter()
            .filter_map(|path| fs::read_to_string(&path).ok().map(|text| Source { path, text }))
            .collect();
        Corpus { name: root.display().to_string(), files }
    }

    pub fn bytes(&self) -> usize {
        self.files.iter().map(|f| f.text.len()).sum()
    }
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path.extension().is_some_and(|e| e == "scala") {
            out.push(path.to_path_buf());
        }
        return;
    }
    let Ok(entries) = fs::read_dir(path) else { return };
    for entry in entries.flatten() {
        collect(&entry.path(), out);
    }
}

pub fn mb_per_s(bytes: usize, iters: u32, secs: f64) -> f64 {
    bytes as f64 * f64::from(iters) / secs / 1e6
}
