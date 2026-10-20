//! The variances of type parameters: a class's, and those of the type lambda an alias, opaque
//! or abstract type defines.

use super::{Flags, Kind, SymbolId, Universe};
use crate::tags::{CONTRAVARIANT, COVARIANT, LAMBDAtpt, TYPEPARAM};
use crate::{Ast, NodeId, TypeId};

/// How deep below its definition a type's lambda is looked for.
const LAMBDA_DEPTH: u32 = 3;

impl Universe {
    /// The variances of the type parameters of class or type `s`, +1, -1 or 0 each, those of
    /// the parameters it declares for a type without a tree; none for a type that takes no
    /// parameters.
    pub fn variances(&self, s: SymbolId) -> Vec<i8> {
        let kind = self.symbol(s).kind;
        let script_type = matches!(kind, Kind::Type | Kind::TypeParam) && self.symbol(s).tree.is_none();
        if kind == Kind::Class || script_type {
            return self.type_params(s).into_iter().map(|p| variance(self.symbol(p).flags)).collect();
        }
        let Some((file, id)) = self.symbol(s).tree else { return Vec::new() };
        let (ast, _) = self.file(file);
        let Some(lambda) = find_lambda(ast, id, LAMBDA_DEPTH) else { return Vec::new() };
        let params = ast.kids(lambda).iter().filter(|&&k| ast.tag(k) == TYPEPARAM);
        params
            .map(|&p| variance(ast.kids(p).iter().fold(Flags::default(), |f, &m| f.with(ast.tag(m)))))
            .collect()
    }

    /// The variances of the type parameters of type constructor `t`: an abstract or opaque
    /// type's own, else those of its class.
    pub fn constructor_variances(&mut self, t: TypeId) -> Vec<i8> {
        if let Some(s) = self.referenced(t)
            && matches!(self.symbol(s).kind, Kind::Type | Kind::TypeParam)
            && self.alias_of(s).is_none()
        {
            return self.variances(s);
        }
        self.class_symbol(t).map(|c| self.variances(c)).unwrap_or_default()
    }
}

fn variance(flags: Flags) -> i8 {
    match (flags.has(COVARIANT), flags.has(CONTRAVARIANT)) {
        (true, _) => 1,
        (_, true) => -1,
        _ => 0,
    }
}

/// The first type lambda among the trees below `id`, at most `depth` levels down.
fn find_lambda(ast: &Ast, id: NodeId, depth: u32) -> Option<NodeId> {
    let depth = depth.checked_sub(1)?;
    ast.kids(id).iter().map(|&k| ast.deref(k)).find_map(|k| match ast.tag(k) {
        LAMBDAtpt => Some(k),
        _ => find_lambda(ast, k, depth),
    })
}
