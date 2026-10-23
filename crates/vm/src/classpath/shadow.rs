//! Whether the directories and jars before the library define files in given packages.

use super::{Classpath, Root};

impl Classpath {
    /// Whether a directory or jar before the library has a NIR file in one of `packages`, given
    /// sorted as `/`-separated directories, which it would define in place of the library's.
    pub fn shadows(&mut self, packages: &[&str]) -> bool {
        let is_nir = |name: &str| name.ends_with(".nir");
        let roots = &mut self.roots[..self.own];
        super::keep_errno(|| {
            roots.iter_mut().any(|root| match root {
                Root::Dir(dir) => packages.iter().any(|p| {
                    std::fs::read_dir(dir.join(p)).is_ok_and(|entries| {
                        entries.flatten().any(|e| e.file_name().to_str().is_some_and(is_nir))
                    })
                }),
                Root::Jar(zip) => zip.file_names().flatten().any(|n| {
                    n.rsplit_once('/')
                        .is_some_and(|(dir, file)| is_nir(file) && packages.binary_search(&dir).is_ok())
                }),
                Root::Image(_) => false,
            })
        })
    }
}
