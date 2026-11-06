// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/ast/TreeInfo.scala
//   compiler/src/dotty/tools/dotc/core/tasty/TreeUnpickler.scala
//! Whether constructing a class runs no initialization code (dotty's `NoInits`, as
//! `TreeUnpickler` computes it from the template: `parentsKind` and the body's statements).

use super::{SymbolId, Universe};
use crate::tags::{
    APPLY, BLOCK, DEFDEF, EXPORT, IMPORT, LAZY, OBJECT, PARAM, SPLITCLAUSE, TEMPLATE, TRAIT, TYPEDEF,
    TYPEPARAM, VALDEF,
};
use crate::{Ast, NodeId, View, view};

impl Universe {
    /// Whether class `c` has no initialization code: a trait whose body defines only methods,
    /// lazy or abstract values, types and classes; a class that also calls no parent
    /// constructor with arguments. Classes without TASTy (`Any`, `Object`, a script's) have none
    /// unless `set_inits` recorded some. While the Scala library is built, a class of a Scala 2
    /// library TASTy file initializes as its source does (`TreeInfo.defKind`): also a lazy value,
    /// a concrete method or a class does.
    pub fn no_inits(&self, c: SymbolId) -> bool {
        let Some((file, typedef)) = self.symbol(c).tree else { return !self.with_inits.contains(&c) };
        let (ast, names) = self.file(file);
        let Some(&template) = ast.kids(typedef).first() else { return true };
        if ast.tag(template) != TEMPLATE {
            return true;
        }
        let scala2 = self.builds_scala_library && self.files[file.0 as usize].scala2;
        let kids = ast.kids(template);
        let body =
            kids.iter().position(|&k| matches!(ast.tag(k), DEFDEF | SPLITCLAUSE)).unwrap_or(kids.len());
        let body_ok = kids[body..].iter().all(|&s| !initializes(ast, names, s, scala2));
        if self.symbol(c).flags.has(TRAIT) {
            return body_ok;
        }
        let parents =
            kids[..body].iter().map(|&k| ast.deref(k)).filter(|&k| !matches!(ast.tag(k), TYPEPARAM | PARAM));
        body_ok && parents.into_iter().all(|p| !calls_with_arguments(ast, p))
    }

    /// Whether class `c` is `Some`, one of `Tuple1` to `Tuple22`, or the module class of one,
    /// which initialize nothing whatever `no_inits` says (dotty's `isAssuredNoInits`).
    pub fn is_assured_no_inits(&self, c: SymbolId) -> bool {
        let name = self.full_name(c);
        let name = name.strip_suffix('$').unwrap_or(&name);
        let tuple = name.strip_prefix("scala.Tuple").and_then(|n| n.parse::<u8>().ok());
        name == "scala.Some" || tuple.is_some_and(|n| (1..=22).contains(&n))
    }

    /// Records that a unit replaces a class of the Scala library.
    pub fn set_builds_scala_library(&mut self) {
        self.builds_scala_library = true;
    }
}

/// Whether statement `s` of a template body runs code when an instance is made: a concrete,
/// value neither lazy nor an object's, or an expression; if `scala2`, also a lazy value, a
/// concrete method other than the constructor, or a class.
fn initializes(ast: &Ast, names: &[String], s: NodeId, scala2: bool) -> bool {
    match ast.tag(s) {
        VALDEF => match view(ast, s) {
            View::ValDef { rhs, mods, .. } => {
                let lazy = |m: &NodeId| ast.tag(*m) == LAZY && !scala2;
                rhs.is_some() && !mods.iter().any(|m| ast.tag(*m) == OBJECT || lazy(m))
            }
            _ => true,
        },
        DEFDEF if scala2 => match view(ast, s) {
            View::DefDef { name, parts } => parts.body.len() > 1 && names[name.0 as usize] != "<init>",
            _ => true,
        },
        TYPEDEF if scala2 => matches!(view(ast, s), View::TypeDef { is_class: true, .. }),
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
