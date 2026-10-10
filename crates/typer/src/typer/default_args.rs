//! Default arguments as getter calls (dotty's `findDefaultGetter`, `spliceMeth`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::TName;
use tasty::tags::{OBJECT, SYNTHETIC};
use tasty::{Kind, SymbolId, Type};

impl Typer<'_> {
    /// The default argument for parameter `n` of `method`, which call `f` applies (dotty's
    /// `defaultArgument`).
    pub fn default_arg(
        &mut self,
        f: TNodeId,
        method: SymbolId,
        n: usize,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let (mref, targs) = match *self.tpd.tree(f) {
            T::TypeApply { fun, args } => (fun, Some(args)),
            T::Apply { .. } => return Err("default argument after an argument list"),
            _ => (f, None),
        };
        let span = self.tpd.span(f);
        let sym = self.u.symbol(method);
        let ctor = sym.name == "<init>";
        let base = if ctor || sym.name == "apply" && sym.flags.has(SYNTHETIC) {
            "$lessinit$greater"
        } else {
            &sym.name
        };
        let getter_name = format!("{base}$default${}", n + 1);
        let receiver = match *self.tpd.tree(mref) {
            T::Select { qualifier, .. } if ctor => self.companion_ref(qualifier, method, ctx)?,
            T::Select { qualifier, .. } => self.copied_path(qualifier),
            T::Ident { .. } => self.ident_receiver(mref, span.start_pos(), ctx)?,
            _ => return Err("default argument"),
        };
        let site = self.tpd.tpe(receiver);
        let sym_name = getter_name.replace("$lessinit$greater", "<init>");
        let [getter] = self.type_members(site, &sym_name)[..] else { return Err("default getter") };
        self.check_reference(getter, Some(site), ctx)?;
        let tpe = self.ref_type(Some(site), getter);
        let name = TName { name: self.name(&getter_name), is_type: false };
        let select = self.tpd.node(T::Select { qualifier: receiver, name }, span, Some(tpe), Some(getter));
        let Some(targs) = targs else { return Ok(select) };
        let targs = self.tpd.items(targs).to_vec();
        let types: Vec<_> = targs.iter().map(|&a| self.tpd.tpe(a)).collect();
        let trees: Vec<TNodeId> =
            targs.iter().zip(&types).map(|(&a, &t)| self.type_tree(t, self.tpd.span(a), true)).collect();
        let tpe = self.instantiated(select, &types)?;
        let tpe = if ctor { tpe } else { self.simplified(tpe) };
        Ok(self.build().type_apply(select, &trees, tpe, span))
    }

    /// A copy of path or call `t`, its qualifiers, functions and arguments copied too.
    pub fn copied_path(&mut self, t: TNodeId) -> TNodeId {
        let tree = match self.tpd.tree(t).clone() {
            T::Select { qualifier, name } => T::Select { qualifier: self.copied_path(qualifier), name },
            T::This { qual } => T::This { qual: self.copied_path(qual) },
            T::Apply { fun, args } | T::TypeApply { fun, args } => {
                let fun = self.copied_path(fun);
                let items: Vec<TNodeId> =
                    self.tpd.items(args).to_vec().into_iter().map(|a| self.copied_path(a)).collect();
                let args = self.tpd.list(&items);
                match self.tpd.tree(t) {
                    T::Apply { .. } => T::Apply { fun, args },
                    _ => T::TypeApply { fun, args },
                }
            }
            tree => tree,
        };
        let (span, tpe, sym) = (self.tpd.span(t), self.tpd.get(t).tpe, self.tpd.sym(t));
        self.tpd.node(tree, span, tpe, sym)
    }

    /// The receiver of the default getters of method reference `mref`, an identifier: its
    /// prefix as a tree at `at` (dotty's `reifyPrefix`), an object's `this` or a reference to it.
    fn ident_receiver(
        &mut self,
        mref: TNodeId,
        at: syntax::ast::Span,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let tpe = self.tpd.tpe(mref);
        let Type::Sym(Some(prefix), _) = *self.u.types().get(tpe) else {
            return Err("default argument of a local method");
        };
        let module = match *self.u.types().get(prefix) {
            Type::This(class) => {
                let Some(c) = self.u.class_symbol(class) else { return Err("default argument") };
                match self.package_object(c) {
                    Some((_, module)) => module,
                    None => return Ok(self.build().this(class, at)),
                }
            }
            Type::Sym(_, m) if self.u.symbol(m).flags.has(OBJECT) => m,
            _ => return Err("default argument through a path"),
        };
        let owner = self.u.symbol(module).owner.ok_or("default argument")?;
        let module_prefix = crate::defn::owner_prefix(self.u, owner);
        if !self.term_prefix_is_elidable(Some(module_prefix), module, ctx) {
            return Err("default argument through a path");
        }
        let tpe = self.ref_type(Some(module_prefix), module);
        Ok(self.build().ident(module, tpe, at))
    }

    /// The package and the object of package object class `c` (a unit's top-level definitions).
    fn package_object(&mut self, c: SymbolId) -> Option<(SymbolId, SymbolId)> {
        if !tasty::module_name(&self.u.symbol(c).name).is_some_and(|n| n.ends_with("$package")) {
            return None;
        }
        let pkg = self.u.symbol(c).owner.filter(|&o| self.u.symbol(o).kind == Kind::Package)?;
        Some((pkg, self.u.source_module(c)?))
    }

    /// A reference to the companion object of the class constructor `ctor` creates in `new_tree`.
    fn companion_ref(
        &mut self,
        new_tree: TNodeId,
        ctor: SymbolId,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let class = self.u.symbol(ctor).owner.ok_or("default argument")?;
        if self.u.symbol(class).owner.is_none() {
            return Err("default argument");
        }
        let module = self.u.companion_module(class).ok_or("default argument without a companion")?;
        let new_tpe = self.tpd.tpe(new_tree);
        let class_tpe = self.u.base_type(new_tpe, class).ok_or("default argument")?;
        let tycon = match *self.u.types().get(class_tpe) {
            Type::Applied(t, _) => t,
            _ => class_tpe,
        };
        let Type::Sym(prefix, _) = *self.u.types().get(tycon) else { return Err("default argument") };
        let tpe = self.ref_type(prefix, module);
        let at = self.tpd.span(new_tree).start_pos();
        if self.term_prefix_is_elidable(prefix, module, ctx) {
            return Ok(self.build().ident(module, tpe, at));
        }
        let term = self.build().term_name(module);
        Ok(self.this_select(at, term, tpe, module))
    }
}
