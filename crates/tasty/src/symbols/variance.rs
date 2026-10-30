//! The variances of type parameters: a class's, and those of the type lambda an alias, opaque
//! or abstract type defines.

use super::{Flags, Kind, SymbolId, Universe};
use crate::tags::{CONTRAVARIANT, COVARIANT, LAMBDAtpt, STABLE, TYPEBOUNDStpt, TYPELAMBDAtype, TYPEPARAM};
use crate::{Ast, Binder, NodeId, TypeId};

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

    /// Records the variances of type parameters `params` as those type lambda `binder`, made
    /// outside TASTy, declares.
    pub fn declare_variances(&mut self, binder: Binder, params: &[SymbolId]) {
        let variances = params.iter().map(|&p| variance(self.symbol(p).flags)).collect();
        self.lambda_variances.insert(binder, variances);
    }

    /// Whether type parameter `s` is declared covariant or contravariant.
    pub fn has_variance(&self, s: SymbolId) -> bool {
        variance(self.symbol(s).flags) != 0
    }

    /// The variances type lambda `binder` declares: those recorded, those of a `TYPELAMBDAtype`'s
    /// modifiers, or those of a `LAMBDAtpt`'s parameters if it is over bounds or one has a
    /// variance (dotty's `isDeclaredVarianceLambda`); `None` where they are structural.
    pub fn declared_variances(&self, binder: Binder) -> Option<Vec<i8>> {
        if let Some(v) = self.lambda_variances.get(&binder) {
            return Some(v.clone());
        }
        let Binder(file, addr) = binder;
        if file.0 as usize >= self.files.len() {
            return None;
        }
        let (ast, _) = self.file(file);
        let id = ast.at(addr)?;
        let kids = ast.kids(id);
        let declared: Vec<i8> = match ast.tag(id) {
            TYPELAMBDAtype => {
                kids.iter().map(|&k| ast.tag(k)).filter(|&t| is_variance(t)).map(tag_variance).collect()
            }
            LAMBDAtpt => {
                let params = kids.iter().filter(|&&k| ast.tag(k) == TYPEPARAM);
                let flags = |p: NodeId| ast.kids(p).iter().fold(Flags::default(), |f, &m| f.with(ast.tag(m)));
                let v: Vec<i8> = params.map(|&p| variance(flags(p))).collect();
                let bounds = kids.last().is_some_and(|&b| ast.tag(ast.deref(b)) == TYPEBOUNDStpt);
                if !bounds && v.iter().all(|&v| v == 0) {
                    return None;
                }
                v
            }
            _ => return None,
        };
        (!declared.is_empty()).then_some(declared)
    }
}

fn is_variance(tag: u8) -> bool {
    matches!(tag, STABLE | COVARIANT | CONTRAVARIANT)
}

fn tag_variance(tag: u8) -> i8 {
    match tag {
        COVARIANT => 1,
        CONTRAVARIANT => -1,
        _ => 0,
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
