// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Synthesizer.scala
//! Synthesized `TypeTest[S, T]` (dotty's `synthesizedTypeTest`): `TypeTest.identity[T]` if `S`
//! conforms to `T`, else `(s: S) => if s.isInstanceOf[T] then Some(s.asInstanceOf[s.type & T])
//! else None`.

use super::Synthesizer;
use crate::defn::TYPE_TEST;
use crate::tpd::{TNodeId, Tpd, Tree as T};
use crate::typer::{Pt, Typer};
use syntax::ast::modifiers::Modifiers;
use syntax::ast::{NodeId, TName, Tree};
use tasty::{SymbolId, TypeId, Universe, nme};

const NO_TEST: [&str; 3] = ["scala.AnyVal", "scala.AnyRef", "java.lang.Object"];

impl Synthesizer {
    /// Synthesizes `TypeTest[from, to]` as in the module comment, at a synthetic position; `None`
    /// for a test against `Nothing` or `Null`, and against `AnyVal`, `AnyRef` or `Object` unless
    /// every `from` already is one.
    pub fn type_test(
        self,
        t: &mut Typer<'_>,
        from: TypeId,
        to: TypeId,
    ) -> Result<Option<TNodeId>, &'static str> {
        let b = t.u.defn();
        if t.u.class_symbol(to).is_some_and(|c| c == b.nothing || c == b.null) {
            return Ok(None);
        }
        let from = t.fully_defined_as(from, true)?;
        let to = t.fully_defined_as(to, true)?;
        let to = t.u.normalized(to);
        let class = t.resolve_full_name(TYPE_TEST, true).ok_or("TypeTest")?;
        let pt = crate::defn::applied(t.u, class, vec![from, to]);
        let s = Synthesizer { at: self.at.to_synthetic(), ..self };
        if t.u.is_subtype(from, to) {
            let module = t.resolve_full_name(TYPE_TEST, false).ok_or("TypeTest")?;
            let module_class = t.u.module_class(module).ok_or("TypeTest")?;
            let decl = t.u.decl(module_class, "identity").ok().and_then(|d| d.first().copied());
            let identity = s.ref_splice(t, decl.ok_or("TypeTest.identity")?);
            let targ = s.type_splice(t, to);
            let tree = s.ast_apply(t, identity, targ, true);
            return Ok(Some(t.typed(tree, Pt::Type(pt), s.ctx)));
        }
        let symbol = t.u.class_symbol(to).map(|c| t.u.full_name(c));
        if symbol.is_some_and(|n| NO_TEST.contains(&n.as_str())) {
            return Ok(None);
        }
        let name = TName { name: t.name("s"), is_type: false };
        let tpt = s.type_splice(t, from);
        let mods = t.ast.mods(Modifiers::default());
        let param = s.node(t, Tree::ValDef { name, tpt, rhs: None, mods });
        let reference = s.node(t, Tree::Ident { name });
        let test = s.ast_select(t, reference, nme::IS_INSTANCE_OF);
        let targ = s.type_splice(t, to);
        let cond = s.ast_apply(t, test, targ, true);
        let reference = s.node(t, Tree::Ident { name });
        let singleton = s.node(t, Tree::SingletonTypeTree { reference });
        let and = TName { name: t.name("&"), is_type: true };
        let and = s.node(t, Tree::Ident { name: and });
        let targ = s.type_splice(t, to);
        let args = t.ast.list(&[singleton, targ]);
        let both = s.node(t, Tree::AppliedTypeTree { tpt: and, args });
        let reference = s.node(t, Tree::Ident { name });
        let cast = s.ast_select(t, reference, nme::AS_INSTANCE_OF);
        let cast = s.ast_apply(t, cast, both, true);
        let some = t.resolve_full_name(crate::defn::SOME, false).ok_or("Some")?;
        let some = s.ref_splice(t, some);
        let thenp = s.ast_apply(t, some, cast, false);
        let none = t.resolve_full_name("scala.None", false).ok_or("None")?;
        let elsep = Some(s.ref_splice(t, none));
        let body = s.node(t, Tree::If { cond, thenp, elsep, inline: false });
        let args = t.ast.list(&[param]);
        let function = s.node(t, Tree::Function { args, body, mods: None, wildcard: false });
        let typed = t.typed(function, Pt::Type(pt), s.ctx);
        as_built(&mut t.tpd, t.u, typed);
        Ok(Some(typed))
    }

    fn ast_select(self, t: &mut Typer<'_>, qualifier: NodeId, name: &str) -> NodeId {
        let name = TName { name: t.name(name), is_type: false };
        self.node(t, Tree::Select { qualifier, name })
    }

    /// `fun[arg]` if `types`, else `fun(arg)`.
    fn ast_apply(self, t: &mut Typer<'_>, fun: NodeId, arg: NodeId, types: bool) -> NodeId {
        let args = t.ast.list(&[arg]);
        let tree = if types { Tree::TypeApply { fun, args } } else { Tree::Apply { fun, args } };
        self.node(t, tree)
    }

    fn type_splice(self, t: &mut Typer<'_>, tpe: TypeId) -> NodeId {
        let typed = t.type_tree(tpe, self.at, false);
        let id = self.node(t, Tree::TypeTree);
        t.splices.insert(id, typed);
        id
    }

    fn ref_splice(self, t: &mut Typer<'_>, sym: SymbolId) -> NodeId {
        let tpe = crate::defn::type_ref(t.u, sym);
        let typed = t.build().ident(sym, tpe, self.at);
        let id = self.node(t, Tree::TypeTree);
        t.splices.insert(id, typed);
        id
    }
}

/// Typed closure `tree` of a type test with the trees and flags of dotty's `tpd`: the type
/// arguments of `Some.apply` and `asInstanceOf` as type trees, the target type inferred, the
/// closure method no artifact.
fn as_built(tpd: &mut Tpd, u: &mut Universe, tree: TNodeId) {
    let T::Block { stats, expr } = *tpd.tree(tree) else { return };
    if let T::Closure { tpt: Some(tpt), .. } = *tpd.tree(expr) {
        tpd.get_mut(tpt).tree = T::TypeTree { inferred: true };
    }
    let Some(&def) = tpd.items(stats).first() else { return };
    if let Some(meth) = tpd.sym(def) {
        let flags = u.symbol(meth).flags.without(tasty::tags::ARTIFACT);
        u.set_flags(meth, flags);
    }
    let T::DefDef { rhs: Some(rhs), .. } = *tpd.tree(def) else { return };
    let T::If { thenp, .. } = *tpd.tree(rhs) else { return };
    let T::Apply { fun, args } = *tpd.tree(thenp) else { return };
    let cast = tpd.items(args).first().copied();
    for f in [Some(fun), cast].into_iter().flatten() {
        if let T::TypeApply { args, .. } = *tpd.tree(f) {
            for &a in tpd.items(args).to_vec().iter() {
                tpd.get_mut(a).tree = T::TypeTree { inferred: false };
            }
        }
    }
}
