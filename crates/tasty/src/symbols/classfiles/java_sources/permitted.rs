//! The permitted subclasses of a sealed Java class, as javac writes them in its
//! `PermittedSubclasses`: those of its `permits` clause, else the subclasses of its unit.

use super::Located;
use super::class_info::Signatures;
use super::resolve::{Resolved, Scope, resolve};
use super::syntax::Type;
use crate::symbols::Universe;

/// Determines the internal names of the permitted subclasses of class `at`; empty if it is not
/// sealed, the refusal of a `permits` name not found.
pub fn permitted(u: &mut Universe, s: &mut Signatures, at: &Located) -> Result<Vec<String>, String> {
    let class = at.class();
    if !class.sealed {
        return Ok(Vec::new());
    }
    if !class.permitted.is_empty() {
        let descriptors = class.permitted.iter().map(|t| s.ty(u, t).map(|(_, d)| d));
        let descriptors = descriptors.collect::<Result<Vec<_>, _>>()?;
        return Ok(descriptors.into_iter().map(|d| d[1..d.len() - 1].to_owned()).collect());
    }
    let mut out = Vec::new();
    for other in s.sources.unit_classes(&at.unit) {
        let scope = Scope { at: &other, vars: Vec::new(), in_header: true };
        let parents = other.class().extends.iter().chain(&other.class().implements);
        let extends_at = parents
            .filter_map(|t| if let Type::Named(parts) = t { Some(parts) } else { None })
            .any(|parts| {
                let names: Vec<&str> = parts.iter().map(|(n, _)| n.as_str()).collect();
                matches!(resolve(u, s.sources, &scope, &names), Ok(Resolved::Class(c)) if c == at.internal)
            });
        if extends_at {
            out.push(other.internal.clone());
        }
    }
    Ok(out)
}
