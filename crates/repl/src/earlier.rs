//! The earlier inputs that ran, as the import statements that bring their definitions into the
//! scope of a later input; a later definition hides an earlier one of the same name and kind.

use tasty::tags::PRIVATE;
use tasty::{Kind, Universe};

pub struct Earlier {
    /// The wrapper object.
    pub object: String,
    /// The input's top-level imports as source text.
    pub imports: Vec<String>,
    /// The input's top-level imports as `:imports` lists them.
    pub shown: Vec<String>,
    /// The names the wrapper defines, each with whether it names a type.
    pub members: Vec<(String, bool)>,
}

/// The names top-level object `object` defines that an import can hide, each with whether it
/// names a type.
pub fn members(u: &mut Universe, object: &str) -> Vec<(String, bool)> {
    let root = u.root();
    let Some(class) = u.module_class_in(root, object) else { return Vec::new() };
    let mut names: Vec<(String, bool)> = (u.symbol(class).decls.iter())
        .map(|&d| u.symbol(d))
        .filter(|s| !s.flags.has(PRIVATE) && !s.name.contains(['<', '$']))
        .map(|s| (s.name.clone(), matches!(s.kind, Kind::Class | Kind::Type)))
        .collect();
    names.sort();
    names.dedup();
    names
}

/// The import statements of `lines`, in order.
pub fn prelude(lines: &[Earlier]) -> String {
    let mut out = String::new();
    for (i, line) in lines.iter().enumerate() {
        let later = &lines[i + 1..];
        let mut hidden: Vec<&str> = (line.members.iter())
            .filter(|m| later.iter().any(|l| l.members.contains(m)))
            .map(|(m, _)| m.as_str())
            .collect();
        hidden.dedup();
        let selectors: Vec<String> =
            hidden.iter().map(|m| format!("`{m}` as _")).chain(["given".into(), "*".into()]).collect();
        out.push_str(&format!("import {}.{{{}}}\n", line.object, selectors.join(", ")));
        line.imports.iter().for_each(|i| out.push_str(&format!("{i}\n")));
    }
    out
}
