//! The package table: each TASTy file of a library with the package and name it is entered
//! under, kept in the library image so that opening a universe reads no other file name.

use super::jdk;

/// The name of the package table in a library image; it changes with the table's format.
pub const PACKAGE_TABLE: &str = "scalai/tasty-packages-1";

/// The package table of the `.tasty` files among `names`: a line `package\tname\tpath` per
/// file, in the order of the paths; Scala Native's renamed JDK classes under their JDK names.
pub fn package_table(names: &[&str]) -> String {
    let mut paths: Vec<&str> = names.iter().copied().filter(|n| n.ends_with(".tasty")).collect();
    paths.sort_unstable();
    let mut out = String::new();
    for path in paths {
        let (dir, file) = path.rsplit_once('/').unwrap_or(("", path));
        let (dir, stem) = (dir.replace('/', "."), file.strip_suffix(".tasty").unwrap_or(file));
        let (dir, stem) = match jdk::jdk_name(&dir, stem) {
            Some((pkg, name)) => (pkg.to_owned(), name),
            None => (dir, stem.to_owned()),
        };
        out.extend([dir.as_str(), "\t", &stem, "\t", path, "\n"]);
    }
    out
}
