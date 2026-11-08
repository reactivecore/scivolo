// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Applications.scala
//! Default arguments as getter calls (dotty's `findDefaultGetter`, `spliceMeth`).

use super::Typer;
use crate::ctx::Ctx;
use crate::tpd::{TNodeId, Tree as T};
use syntax::ast::TName;
use tasty::tags::{GIVEN, OBJECT, SYNTHETIC};
use tasty::{Kind, SymbolId, Type};

/// A clause a call applies: its value or type arguments.
enum Clause {
    Values(Vec<TNodeId>),
    Types(Vec<TNodeId>),
}

impl Typer<'_> {
    /// The default argument for parameter `n` of `method`, which call `f` applies (dotty's
    /// `defaultArgument` in `tryDefault`); its type keeps `@uncheckedVariance` if the call's
    /// arguments were all typed before (`ApplyToTyped`), else is simplified as dotty's `typedArg`
    /// simplifies the getter call in `ApplyToUntyped`.
    pub fn default_arg(
        &mut self,
        f: TNodeId,
        (method, n): (SymbolId, usize),
        args_typed: bool,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let mut clauses = Vec::new();
        let mut mref = f;
        let mut leading = false;
        loop {
            match *self.tpd.tree(mref) {
                T::Apply { fun, args } => {
                    let args = self.tpd.items(args).to_vec();
                    let tpe = self.tpd.tpe(mref);
                    leading = args.is_empty()
                        && !matches!(self.tpd.tree(fun), T::Apply { .. })
                        && matches!(self.u.types().get(tpe), Type::Method { implicit: true, .. });
                    clauses.push(Clause::Values(self.spliced_args(fun, &args)?));
                    mref = fun;
                }
                T::TypeApply { fun, args } => {
                    clauses.push(Clause::Types(self.tpd.items(args).to_vec()));
                    mref = fun;
                }
                _ => break,
            }
        }
        let span = self.tpd.span(f);
        let sym = self.u.symbol(method);
        let ctor = sym.name == "<init>";
        let values = clauses.iter().map(|c| if let Clause::Values(args) = c { args.len() } else { 0 });
        let n = n + values.sum::<usize>();
        let base = if ctor || sym.name == "apply" && sym.flags.has(SYNTHETIC) {
            "$lessinit$greater"
        } else {
            &sym.name
        };
        let mut getter_name = format!("{base}$default${}", n + 1);
        let local = matches!(*self.tpd.tree(mref), T::Ident { .. })
            && matches!(*self.u.types().get(self.tpd.tpe(mref)), Type::Sym(None, _));
        let mut getter = if local {
            let getter = self.levels.scope_mate(method, &getter_name, |_| true).ok_or("default getter")?;
            self.complete(getter);
            let tpe = self.ref_type(None, getter);
            self.build().ident(getter, tpe, span)
        } else {
            let receiver = match *self.tpd.tree(mref) {
                T::Select { qualifier, .. } if ctor => {
                    self.companion_ref(qualifier, method, span.start_pos(), ctx)?
                }
                T::Select { qualifier, .. } => self.copied_path(qualifier),
                T::Ident { .. } => self.ident_receiver(mref, span.start_pos(), ctx)?,
                _ => return Err("default argument"),
            };
            let (mut receiver, mut site) = (receiver, self.tpd.tpe(receiver));
            let mut sym_name = getter_name.replace("$lessinit$greater", "<init>");
            if self.type_members(site, &sym_name).is_empty()
                && let Some(module) = self.given_class_companion(method)
            {
                (site, sym_name) =
                    (crate::defn::type_ref(self.u, module), format!("<init>$default${}", n + 1));
                getter_name = sym_name.replace("<init>", "$lessinit$greater");
                receiver = self.build().ident(module, site, span);
            }
            let [getter] = self.type_members(site, &sym_name)[..] else { return Err("default getter") };
            self.check_synthetic_reference(getter, Some(site), ctx)?;
            let tpe = self.ref_type(Some(site), getter);
            let name = TName { name: self.name(&getter_name), is_type: false };
            self.tpd.node(T::Select { qualifier: receiver, name }, span, Some(tpe), Some(getter))
        };
        for clause in clauses.into_iter().rev() {
            let args = match clause {
                Clause::Values(args) => args,
                Clause::Types(targs) => {
                    let types: Vec<_> = targs.iter().map(|&a| self.tpd.tpe(a)).collect();
                    let trees: Vec<TNodeId> = targs
                        .iter()
                        .zip(&types)
                        .map(|(&a, &t)| self.type_tree(t, self.tpd.span(a), true))
                        .collect();
                    let tpe = self.instantiated(getter, &types)?;
                    getter = self.build().type_apply(getter, &trees, tpe, span.to_synthetic());
                    continue;
                }
            };
            let t = self.tpd.tpe(getter);
            let info = self.method_info(t).unwrap_or(t);
            let nullary =
                matches!(self.u.types().get(info), Type::Method { params, .. } if params.is_empty());
            if std::mem::take(&mut leading) && ctor && !nullary {
                if matches!(self.tpd.tree(getter), T::TypeApply { .. }) {
                    return Err(
                        "default getter of a constructor with type arguments and a leading implicit clause",
                    );
                }
                continue;
            }
            let Some(m) = self.u.method(info) else { return Err("default getter") };
            if m.params.len() != args.len() {
                return Err("default getter of another clause shape");
            }
            let (binder, result, names) = (m.binder, m.result, m.names());
            let arg_types: Vec<_> = args.iter().map(|&a| self.tpd.tpe(a)).collect();
            let result = self.applied_result(result, (binder, &names), self.tpd.sym(getter), &arg_types);
            getter = self.build().apply(getter, &args, result, span.to_synthetic());
        }
        if !args_typed {
            let tpe = self.tpd.tpe(getter);
            self.tpd.get_mut(getter).tpe = Some(self.simplified(tpe));
        }
        Ok(getter)
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

    /// A reference at `at` to the companion object of the class constructor `ctor` creates in
    /// `new_tree`.
    fn companion_ref(
        &mut self,
        new_tree: TNodeId,
        ctor: SymbolId,
        at: syntax::ast::Span,
        ctx: Ctx,
    ) -> Result<TNodeId, &'static str> {
        let class = self.u.symbol(ctor).owner.ok_or("default argument")?;
        if self.u.symbol(class).owner.is_none() {
            return Err("default argument");
        }
        let module = self.u.companion_module(class).ok_or("default argument without a companion")?;
        let new_tpe = self.tpd.tpe(new_tree);
        let new_tpe = self.u.dealias(new_tpe);
        let class_tpe = self.u.base_type(new_tpe, class).ok_or("default argument")?;
        let tycon = match *self.u.types().get(class_tpe) {
            Type::Applied(t, _) => t,
            _ => class_tpe,
        };
        let prefix = match *self.u.types().get(tycon) {
            Type::Sym(prefix, _) => prefix,
            Type::TypeRef(prefix, _) => Some(prefix),
            _ => return Err("default argument of a class of another type"),
        };
        let tpe = self.ref_type(prefix, module);
        Ok(self.ref_(prefix, module, tpe, at, ctx))
    }

    /// The companion of the given class given method `method` wraps (dotty's `findDefaultGetter`
    /// of an implicit wrapper).
    fn given_class_companion(&mut self, method: SymbolId) -> Option<SymbolId> {
        let info = self.u.info(method);
        let result = self.final_result(info);
        let class = self.u.class_symbol(result).filter(|_| self.u.symbol(method).flags.has(GIVEN))?;
        let flags = self.u.symbol(class).flags;
        (flags.has(GIVEN) && flags.has(SYNTHETIC)).then(|| self.u.companion_module(class)).flatten()
    }
}
