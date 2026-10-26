// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala
//! The children of a sealed class of the library, from its `@Child` annotations.

use super::entries::annotations;
use super::{SymbolId, Universe};
use crate::tags::{APPLY, TYPEAPPLY};
use crate::types::{Decoder, TypeId};
use crate::{Ast, NodeId, View, view};

impl Universe {
    /// The types the `@Child` annotations of class `c` in its TASTy name, in the order of the
    /// children's definitions (dotty's `children`); `None` for a class without TASTy.
    pub fn decoded_children(&mut self, c: SymbolId) -> Option<Vec<TypeId>> {
        let (file, id) = self.symbol(c).tree?;
        let Universe { files, by_tree, packages, types, .. } = self;
        let loaded = files[file.0 as usize].decoded();
        let ast = &loaded.ast;
        let View::TypeDef { mods, .. } = view(ast, id) else { return None };
        let mut d = Decoder::new((file, ast, &loaded.names), by_tree, packages, types);
        let annotated: Vec<(TypeId, Option<TypeId>)> = annotations(ast, mods)
            .map(|(ty, call)| (d.ty(ty), type_arg(ast, call).map(|a| d.ty(a))))
            .collect();
        let mut children = Vec::new();
        for (class, arg) in annotated {
            if let Some(arg) = arg
                && self.class_symbol(class).is_some_and(|a| self.full_name(a) == CHILD)
            {
                children.push(arg);
            }
        }
        children.reverse();
        Some(children)
    }
}

const CHILD: &str = "scala.annotation.internal.Child";

/// The first type argument of the constructor call at `id`, `@Child[C]()`.
fn type_arg(ast: &Ast, id: NodeId) -> Option<NodeId> {
    let id = ast.deref(id);
    match (ast.tag(id), ast.kids(id)) {
        (TYPEAPPLY, [_, arg, ..]) => Some(*arg),
        (APPLY | TYPEAPPLY, [fun, ..]) => type_arg(ast, *fun),
        _ => None,
    }
}
