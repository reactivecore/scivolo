// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//   compiler/src/dotty/tools/dotc/typer/Inferencing.scala
//! Extractors named by a type alias of a class, as `Block(stats, expr)` for
//! `type Block = Trees.Block[Int]` (dotty's `followTypeAlias` in `typedUnApply`).

use super::super::Typer;
use super::UnapplyPattern;
use super::fun::has_errors_since;
use crate::ctx::Mode;
use crate::defn::owner_prefix;
use syntax::ast::{NodeId, TName, Tree};
use tasty::{Kind, SymbolId, Type, TypeId};

impl UnapplyPattern {
    /// Reads extractor `qual`, `C` or `p.C`, as a type and, if it names a type alias of a class
    /// with a companion object, returns a reference to that companion at `qual`'s span; `None`
    /// for other extractors and when `C` as a type does not type without errors.
    pub fn followed_type_alias(&self, t: &mut Typer<'_>, qual: NodeId) -> Option<NodeId> {
        let as_type = |name: TName| TName { is_type: true, ..name };
        let renamed = match *t.ast.tree(qual) {
            Tree::Ident { name } => Tree::Ident { name: as_type(name) },
            Tree::Select { qualifier, name } => Tree::Select { qualifier, name: as_type(name) },
            _ => return None,
        };
        let span = t.ast.span(qual);
        let renamed = t.ast.node(renamed);
        t.ast.with_span(renamed, span);

        let ctx = self.ctx.with_mode(Mode::NONE);
        let reported = t.diags.items.len();
        let (tpe, clean, _) = t.speculatively(|t| {
            let typed = t.typed_type(renamed, ctx);
            t.tpd.tpe(typed)
        });
        let errors = has_errors_since(t, reported);
        t.retract_diags(reported);
        if !clean || errors {
            return None;
        }

        t.u.referenced_alias(tpe)?;
        let (prefix, module) = companion(t, tpe)?;
        let module_tpe = t.ref_type(prefix, module);
        let typed = t.ref_(prefix, module, module_tpe, span, ctx);
        let splice = t.ast.node(Tree::TypeTree);
        t.ast.with_span(splice, span);
        t.splices.insert(splice, typed);
        Some(splice)
    }
}

/// Follows aliases, type applications, refinements and annotations from `tpe` to a class and
/// returns the class's companion object and its prefix, `this` of a static owner, else that of
/// the class reference (dotty's `companionRef`); `None` if it arrives at no class or the class
/// has no companion.
fn companion(t: &mut Typer<'_>, tpe: TypeId) -> Option<(Option<TypeId>, SymbolId)> {
    let mut tpe = t.u.dealias(tpe);
    loop {
        match *t.u.ty(tpe) {
            Type::Applied(tycon, _) => tpe = t.u.dealias(tycon),
            Type::Refined(parent, ..) | Type::Annotated(parent, _) | Type::Rec(_, parent) => {
                tpe = t.u.dealias(parent);
            }
            _ => break,
        }
    }
    let (prefix, class) = match *t.u.ty(tpe) {
        Type::Sym(prefix, s) => (prefix, s),
        Type::TypeRef(prefix, _) => (Some(prefix), t.u.resolve_ref(tpe)?),
        _ => return None,
    };
    if t.u.symbol(class).kind != Kind::Class {
        return None;
    }
    let module = t.u.companion_module(class)?;
    let owner = t.u.symbol(module).owner?;
    if t.u.is_static_owner(owner) {
        return Some((Some(owner_prefix(t.u, owner)), module));
    }
    Some((prefix, module))
}
