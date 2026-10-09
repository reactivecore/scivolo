//! `:load <path>`: the whole file run as one input.

use super::Session;
use std::path::{Component, Path, PathBuf};

impl Session {
    /// The lines `:load path` prints.
    pub fn load(&mut self, path: &str) -> Vec<String> {
        if !Path::new(path).exists() {
            return vec![format!("Couldn't find file \"{}\"", canonical(path).display())];
        }
        match std::fs::read(path) {
            Ok(bytes) => self.answer(&String::from_utf8_lossy(&bytes)),
            Err(e) => vec![format!("Couldn't read file \"{}\": {e}", canonical(path).display())],
        }
    }
}

/// `path` as Java's `File.getCanonicalPath`: absolute, the longest existing prefix with its
/// links resolved, then the rest without `.` and `..`.
fn canonical(path: &str) -> PathBuf {
    let full = std::env::current_dir().unwrap_or_default().join(path);
    let parts: Vec<Component> = full.components().collect();
    let resolved = (1..=parts.len()).rev().find_map(|n| {
        let prefix: PathBuf = parts[..n].iter().collect();
        Some((std::fs::canonicalize(prefix).ok()?, n))
    });
    let (mut out, n) = resolved.unwrap_or_default();
    for part in &parts[n..] {
        match part {
            Component::CurDir => {}
            Component::ParentDir => _ = out.pop(),
            part => out.push(part),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::canonical;

    #[test]
    fn canonical_paths_as_java() {
        let cwd = std::fs::canonicalize(std::env::current_dir().unwrap()).unwrap();
        assert_eq!(canonical(""), cwd);
        assert_eq!(canonical("nope/x.scala"), cwd.join("nope/x.scala"));
        assert_eq!(canonical("./nope/../nope/./x"), cwd.join("nope/x"));
        assert_eq!(canonical("src/../nope/a b "), cwd.join("nope/a b "));
        assert_eq!(canonical("/nope/.."), std::path::Path::new("/"));
    }
}
