//! Default arguments as getter calls (dotty's `findDefaultGetter`, `spliceMeth`), where the
//! function needs no lifting.

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::TName;
use tasty::tags::{OBJECT, SYNTHETIC};
use tasty::{Kind, SymbolId, Type};

impl Typer<'_> {
    /// Whether call `f` of a method with default arguments would have its prefix lifted (dotty's
    /// `liftApp` with `LiftComplex`).
    pub fn needs_lift_for_defaults(&mut self, f: TNodeId) -> bool {
        match *self.tpd.tree(f) {
            T::TypeApply { fun, .. } => self.needs_lift_for_defaults(fun),
            T::Select { qualifier, .. } => match *self.tpd.tree(qualifier) {
                T::Literal { .. } | T::This { .. } | T::New { .. } => false,
                T::Ident { .. } | T::Select { .. } => !self.is_idempotent_expr(qualifier),
                _ => true,
            },
            T::Ident { .. } => !self.is_pure_path(f),
            _ => true,
        }
    }

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
        let tpe = self.simplified(tpe);
        let args = self.tpd.list(&trees);
        Ok(self.tpd.node(T::TypeApply { fun: select, args }, span, Some(tpe), None))
    }

    /// A copy of path `t`, its selections and `this` qualifiers copied too.
    fn copied_path(&mut self, t: TNodeId) -> TNodeId {
        let tree = match self.tpd.tree(t).clone() {
            T::Select { qualifier, name } => T::Select { qualifier: self.copied_path(qualifier), name },
            T::This { qual } => T::This { qual: self.copied_path(qual) },
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
                    None => return Ok(self.this_tree(class, at)),
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
        let name = TName { name: self.name(&self.u.symbol(module).name.clone()), is_type: false };
        Ok(self.tpd.node(T::Ident { name }, at, Some(tpe), Some(module)))
    }

    /// The package and the object of package object class `c` (a unit's top-level definitions).
    fn package_object(&mut self, c: SymbolId) -> Option<(SymbolId, SymbolId)> {
        let name = self.u.symbol(c).name.strip_suffix('$').filter(|n| n.ends_with("$package"))?.to_owned();
        let pkg = self.u.symbol(c).owner.filter(|&o| self.u.symbol(o).kind == Kind::Package)?;
        let decls = self.u.decl(pkg, &name).unwrap_or_default();
        let module = decls.into_iter().find(|&d| self.u.symbol(d).kind == Kind::Val)?;
        Some((pkg, module))
    }

    /// A reference to the companion object of the class constructor `ctor` creates in `new_tree`.
    fn companion_ref(
        &mut self,
        new_tree: TNodeId,
        ctor: SymbolId,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let class = self.u.symbol(ctor).owner.ok_or("default argument")?;
        let owner = self.u.symbol(class).owner.ok_or("default argument")?;
        let name = self.u.symbol(class).name.clone();
        let decls = self.u.decl(owner, &name).unwrap_or_default();
        let module = decls
            .into_iter()
            .find(|&d| self.u.symbol(d).kind == Kind::Val && self.u.symbol(d).flags.has(OBJECT))
            .ok_or("default argument without a companion")?;
        let new_tpe = self.tpd.tpe(new_tree);
        let class_tpe = self.u.base_type(new_tpe, class).ok_or("default argument")?;
        let tycon = match *self.u.types().get(class_tpe) {
            Type::Applied(t, _) => t,
            _ => class_tpe,
        };
        let Type::Sym(prefix, _) = *self.u.types().get(tycon) else { return Err("default argument") };
        let tpe = self.ref_type(prefix, module);
        let at = self.tpd.span(new_tree).start_pos();
        let term = TName { name: self.name(&name), is_type: false };
        if self.term_prefix_is_elidable(prefix, module, ctx) {
            Ok(self.tpd.node(T::Ident { name: term }, at, Some(tpe), Some(module)))
        } else {
            Ok(self.this_select(at, term, tpe, module))
        }
    }
}
