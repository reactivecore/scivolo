//! Whether constructing a class runs no initialization code (dotty's `NoInits`, as
//! `TreeUnpickler` computes it from the template: `parentsKind` and the body's statements).

use super::{SymbolId, Universe};
use crate::tags::{
    APPLY, BLOCK, DEFDEF, EXPORT, IMPORT, LAZY, PARAM, SPLITCLAUSE, TEMPLATE, TRAIT, TYPEDEF, TYPEPARAM,
    VALDEF,
};
use crate::{Ast, NodeId, View, view};

impl Universe {
    /// Whether class `c` has no initialization code: a trait whose body defines only methods,
    /// lazy or abstract values, types and classes; a class that also calls no parent
    /// constructor with arguments. Classes without TASTy (`Any`, `Object`, a script's) have none
    /// unless `set_inits` recorded some.
    pub fn no_inits(&self, c: SymbolId) -> bool {
        let Some((file, typedef)) = self.symbol(c).tree else { return !self.with_inits.contains(&c) };
        let (ast, _) = self.file(file);
        let Some(&template) = ast.kids(typedef).first() else { return true };
        if ast.tag(template) != TEMPLATE {
            return true;
        }
        let kids = ast.kids(template);
        let body =
            kids.iter().position(|&k| matches!(ast.tag(k), DEFDEF | SPLITCLAUSE)).unwrap_or(kids.len());
        let body_ok = kids[body..].iter().all(|&s| !initializes(ast, s));
        if self.symbol(c).flags.has(TRAIT) {
            return body_ok;
        }
        let parents =
            kids[..body].iter().map(|&k| ast.deref(k)).filter(|&k| !matches!(ast.tag(k), TYPEPARAM | PARAM));
        body_ok && parents.into_iter().all(|p| !calls_with_arguments(ast, p))
    }
}

/// Whether statement `s` of a template body runs code when an instance is made: a concrete,
/// non-lazy value, or an expression.
fn initializes(ast: &Ast, s: NodeId) -> bool {
    match ast.tag(s) {
        VALDEF => match view(ast, s) {
            View::ValDef { rhs, mods, .. } => rhs.is_some() && !mods.iter().any(|&m| ast.tag(m) == LAZY),
            _ => true,
        },
        DEFDEF | TYPEDEF | TYPEPARAM | PARAM | IMPORT | EXPORT | SPLITCLAUSE => false,
        _ => true,
    }
}

/// Whether parent tree `p` calls a constructor with arguments or is a block (dotty's
/// `parentsKind`).
fn calls_with_arguments(ast: &Ast, p: NodeId) -> bool {
    match ast.tag(p) {
        APPLY => ast.kids(p).len() > 1,
        BLOCK => true,
        _ => false,
    }
}
