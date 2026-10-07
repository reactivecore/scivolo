//! The earlier inputs that ran, as the import statements that bring their definitions into the
//! scope of a later input; a later definition hides an earlier one of the same name.

use crate::render::module_class;
use tasty::Universe;
use tasty::tags::PRIVATE;

pub struct Earlier {
    /// The wrapper object.
    pub object: String,
    /// The input's top-level imports as source text.
    pub imports: Vec<String>,
    /// The names the wrapper defines.
    pub members: Vec<String>,
}

/// The names module class `module` defines that an import can hide.
pub fn members(u: &mut Universe, module: &str) -> Vec<String> {
    let Some(class) = module_class(u, module) else { return Vec::new() };
    let mut names: Vec<String> = (u.symbol(class).decls.iter())
        .map(|&d| u.symbol(d))
        .filter(|s| !s.flags.has(PRIVATE) && !s.name.contains(['<', '$']))
        .map(|s| s.name.clone())
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
        let hidden = line.members.iter().filter(|m| later.iter().any(|l| l.members.contains(m)));
        let selectors: Vec<String> =
            hidden.map(|m| format!("`{m}` as _")).chain(["given".into(), "*".into()]).collect();
        out.push_str(&format!("import {}.{{{}}}\n", line.object, selectors.join(", ")));
        line.imports.iter().for_each(|i| out.push_str(&format!("{i}\n")));
    }
    out
}
