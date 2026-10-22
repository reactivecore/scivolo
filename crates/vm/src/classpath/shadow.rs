//! Whether the directories and jars before the library define files in given packages.

use super::{Classpath, Root};

impl Classpath {
    /// Whether a directory or jar before the library has a file in one of `packages`, given as
    /// `/`-separated directories, which it would define in place of the library's.
    pub fn shadows(&mut self, packages: &[&str]) -> bool {
        self.roots[..self.own].iter_mut().any(|root| match root {
            Root::Dir(dir) => packages.iter().any(|p| dir.join(p).is_dir()),
            Root::Jar(zip) => zip.file_names().any(|n| {
                n.rsplit_once('/').is_some_and(|(dir, file)| !file.is_empty() && packages.contains(&dir))
            }),
            Root::Image(_) => false,
        })
    }
}
