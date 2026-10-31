// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/quoted/PickledQuotes.scala
//! The types the spliced type symbols of a macro's quotes stand for (dotty's `spliceTypes`).

use super::macros::{Expanded, Quoted};
use rustc_hash::FxHashMap;
use tasty::{SymbolId, Type, TypeId, Universe};

/// The type of each spliced type symbol of the quotes of `expanded`, the spliced type symbols
/// in it replaced by theirs.
pub fn spliced_types(u: &mut Universe, expanded: &Expanded) -> FxHashMap<SymbolId, TypeId> {
    let all: FxHashMap<SymbolId, TypeId> = (expanded.exprs.iter())
        .flat_map(|q| match q {
            Quoted::Quote { spliced, .. } => spliced.as_slice(),
            Quoted::Tree(_) => &[],
        })
        .copied()
        .collect();
    all.iter().map(|(&s, &t)| (s, spliced_type(u, &all, t, 0))).collect()
}

fn spliced_type(u: &mut Universe, spliced: &FxHashMap<SymbolId, TypeId>, t: TypeId, depth: usize) -> TypeId {
    if depth > spliced.len() {
        return t;
    }
    u.map_type(t, &mut |u, part| match *u.types().get(part) {
        Type::Sym(None, s) => spliced.get(&s).map(|&r| spliced_type(u, spliced, r, depth + 1)),
        _ => None,
    })
}
