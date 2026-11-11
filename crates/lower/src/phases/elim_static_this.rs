// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/transform/ElimStaticThis.scala
//! dotty's `ElimStaticThis`: in a method `LambdaLift` or `HoistSuperArgs` made static, `C.this` of an object and
//! references to its members through it lose their spans, and the enclosing tree is copied as
//! dotty's tree copiers do, which gives them one.

use super::walk::{children, subtrees};
use crate::Lower;
use crate::symbols::{has, is_jdk_object};
use syntax::ast::Span;
use tasty::tags::STATIC;
use tasty::{SymbolId, Type, defn, nme};
use typer::tpd::{TNodeId, Tree};

pub fn transform(l: &mut Lower, td: TNodeId) {
    let class = l.tpd.sym(td);
    let defs: Vec<(TNodeId, SymbolId)> = subtrees(l.tpd, td)
        .into_iter()
        .filter(|&t| matches!(l.tpd.tree(t), Tree::DefDef { .. }))
        .filter_map(|t| Some((t, l.tpd.sym(t)?)))
        .filter(|&(_, s)| l.u.symbol(s).owner == class)
        .collect();
    for (m, s) in defs {
        if !is_lifted_static(l, s) {
            continue;
        }
        for parent in subtrees(l.tpd, m) {
            let refs: Vec<TNodeId> =
                children(l.tpd, parent).into_iter().filter(|&c| is_object_this_ref(l, c)).collect();
            if refs.is_empty() {
                continue;
            }
            for r in refs {
                l.tpd.get_mut(r).span = Span::NONE;
            }
            l.recopy(parent);
        }
    }
}

/// Whether method `m` is static but neither `@static` nor a static initializer.
fn is_lifted_static(l: &mut Lower, m: SymbolId) -> bool {
    has(l.u, m, STATIC)
        && l.u.symbol(m).name != nme::STATIC_CONSTRUCTOR
        && !l.u.is_annotated(m, defn::STATIC_ANNOT)
}

/// Whether `t` is `C.this` of an object `C`, or an identifier of a member through it.
fn is_object_this_ref(l: &mut Lower, t: TNodeId) -> bool {
    let class = match *l.tpd.tree(t) {
        Tree::This { .. } => l.tpd.sym(t),
        Tree::Ident { name } if !name.is_type => match *l.u.ty(l.tpd.tpe(t)) {
            Type::Sym(Some(p), _) | Type::TermRef(p, _) => l.u.this_of(p).and_then(|c| l.u.class_symbol(c)),
            _ => None,
        },
        _ => None,
    };
    class.is_some_and(|c: SymbolId| l.u.is_module_class(c) && !is_jdk_object(l.u, c))
}
