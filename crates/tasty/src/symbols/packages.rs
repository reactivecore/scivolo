//! Package symbols, and the package table: each TASTy file of a library with the package and
//! name it is entered under, kept in the library image so that opening a universe reads no other
//! file name.

use super::{Flags, Kind, Symbol, SymbolId, Universe, jdk};

impl Universe {
    /// The package of qualified name `name`, entered with its enclosing packages if new; `""`
    /// and `<empty>`, the package of files at the root of a jar, are the root.
    pub fn enter_package(&mut self, name: &str) -> SymbolId {
        let name = if name == "<empty>" { "" } else { name };
        if let Some(&p) = self.packages.get(name) {
            return p;
        }
        let (outer, simple) = name.rsplit_once('.').unwrap_or(("", name));
        let owner = self.enter_package(outer);
        let id = self.define(Symbol {
            name: simple.to_owned(),
            kind: Kind::Package,
            owner: Some(owner),
            flags: Flags::default(),
            target: None,
            is_extern: false,
            derived: false,
            decls: Vec::new(),
            tree: None,
        });
        self.packages.insert(name.to_owned(), id);
        id
    }
}

/// The name of the package table in a library image.
pub const PACKAGE_TABLE: &str = "scivolo/tasty-packages";

/// The format of the package table.
const FORMAT: u32 = 1;

/// What the package table depends on besides the file names: its format and the JDK renames.
pub fn package_table_version() -> String {
    format!("{FORMAT} {:?}", jdk::RENAMED)
}

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
