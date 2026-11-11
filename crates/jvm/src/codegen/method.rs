// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! Method bodies: loads of literals, locals, fields and objects (dotty's `genMethod`, `genLoad`).

use super::body::{Body, Operand};
use super::descriptors::{method_descriptor, with_receiver};
use super::{Gen, java_flags, method_name, unsupported};
use classfile::{ACC_ABSTRACT, ACC_BRIDGE, ACC_FINAL, ACC_PUBLIC, ACC_STATIC, ACC_SYNTHETIC, ClassFile};
use lower::Result;
use tasty::{SymbolId, name_kinds, nme};
use typer::tpd::{TNodeId, Tree};

impl Gen<'_, '_> {
    /// Adds method `sym` defined by `dd` in the class `class_def` defines to `cf`.
    pub fn gen_method(
        &mut self,
        cf: &mut ClassFile,
        (class_def, dd): (TNodeId, TNodeId),
        sym: SymbolId,
    ) -> Result<()> {
        let Tree::DefDef { paramss, rhs, .. } = *self.l.tpd.tree(dd) else { unreachable!("a method") };
        self.check_annotations(sym, self.l.tpd.span(dd))?;
        let (mut desc, mut flags) = (method_descriptor(self.l.u, sym), java_flags(self.l, sym));
        if self.l.is_bridge(sym) {
            flags = (flags & !ACC_FINAL) | ACC_BRIDGE | ACC_SYNTHETIC;
        }
        let owner = self.l.u.symbol(sym).owner.expect("a method's owner");
        let trait_init = self.l.u.is_constructor(sym) && self.l.u.is_trait(owner);
        if trait_init {
            desc = with_receiver(&self.class_name(owner), &desc);
            flags = ACC_PUBLIC | ACC_STATIC;
        }
        let static_ = flags & ACC_STATIC != 0;
        let this_class = (!static_ || trait_init).then(|| self.class_name(owner));
        let name = method_name(self.l.u, sym);
        if name.contains(nme::ANON_FUN) && !self.l.is_bridge_closure(sym) {
            flags |= ACC_SYNTHETIC;
        }
        let params = self.l.tpd.clause_items(paramss).iter().flat_map(|&c| self.l.tpd.items(c));
        let params: Vec<TNodeId> =
            params.filter(|&&p| matches!(self.l.tpd.tree(p), Tree::ValDef { .. })).copied().collect();
        let param_syms: Vec<SymbolId> =
            params.iter().map(|&p| self.l.tpd.sym(p).expect("a parameter's symbol")).collect();
        let mixed_setter = self.l.mixed_in_from(sym).is_some()
            && name.ends_with("_$eq")
            && !name.contains(name_kinds::TRAIT_SETTER_SEPARATOR);
        let erased = flags & ACC_BRIDGE != 0 || trait_init || self.l.is_lifted(sym) || mixed_setter;
        let Some(rhs) = rhs else {
            let flags = (flags & !(ACC_FINAL | ACC_STATIC)) | ACC_ABSTRACT;
            cf.method(flags, &name, &desc, None, &mut |a, b| self.jvm_lub(a, b));
            self.exceptions(cf, sym);
            self.method_attributes(cf, sym, &desc, &param_syms, (erased, false));
            return Ok(());
        };
        let mut b = Body::new(&mut cf.pool, None, u16::from(!static_ || trait_init));
        b.this_class = this_class;
        b.method = Some(sym);
        let (mut param_descs, result) = classfile::split_method(&desc);
        if trait_init {
            param_descs.remove(0);
        }
        if params.len() != param_descs.len() {
            return unsupported(self.l.tpd.span(dd), "parameters other than the descriptor's on the JVM");
        }
        for (p, param) in params.into_iter().zip(param_descs) {
            b.bind(self.l.tpd.sym(p).expect("a parameter's symbol"), param.to_owned());
        }
        b.result = result.to_owned();
        self.method_body(&mut b, rhs, result)?;
        let code = b.code;
        cf.method(flags, &name, &desc, Some(code), &mut |a, b| self.jvm_lub(a, b));
        self.exceptions(cf, sym);
        self.method_attributes(cf, sym, &desc, &param_syms, (erased, trait_init));
        let line = self.line_of(class_def);
        self.trait_forwarder(cf, (sym, flags), (&desc, &param_syms), line);
        Ok(())
    }

    /// Generates `t` and drops its value.
    pub fn stat(&mut self, b: &mut Body, t: TNodeId) -> Result<()> {
        self.line(b, t);
        if let Tree::Apply { fun, args } = *self.l.tpd.tree(t)
            && self.is_synchronized(fun)
        {
            self.gen_synchronized(b, t, (fun, args), None)?;
            return Ok(());
        }
        self.expr_as(b, t, 0)
    }

    /// Generates `t`; returns the stack slots its value takes.
    pub fn expr(&mut self, b: &mut Body, t: TNodeId) -> Result<i32> {
        let at = b.code.stack();
        let slots = self.value(b, t)?;
        b.operand(at, slots, Operand::Of(t));
        Ok(slots)
    }

    fn value(&mut self, b: &mut Body, t: TNodeId) -> Result<i32> {
        self.line(b, t);
        let span = self.l.tpd.span(t);
        let sym = self.l.tpd.sym(t);
        Ok(match self.l.tpd.tree(t).clone() {
            Tree::Literal { value } => self.literal(b, &value),
            Tree::ClassLiteral { tpe } => self.class_literal(b, tpe),
            Tree::Typed { expr, .. } => self.expr(b, expr)?,
            Tree::Block { stats, expr } => {
                for s in self.l.tpd.items(stats).to_vec() {
                    self.stat(b, s)?;
                }
                self.expr(b, expr)?
            }
            Tree::This { .. } if let Some(c) = sym.filter(|&c| self.l.u.is_module_class(c)) => {
                self.load_module(b, c, span)?
            }
            Tree::This { .. } => self.load_this(b),
            Tree::Ident { .. } | Tree::Select { .. }
                if sym.is_some_and(|s| self.l.u.is_module(s) && !self.l.u.is_static_symbol(s)) =>
            {
                let no_args = self.l.tpd.list(&[]);
                self.apply(b, t, no_args)?
            }
            Tree::Ident { .. } | Tree::Select { .. }
                if let Some(s) = sym.filter(|&s| self.l.u.is_module(s)) =>
            {
                let class = self.l.u.module_class(s).expect("an object's class");
                self.load_module(b, class, span)?
            }
            Tree::Ident { .. }
                if let Some(&(_, slot, ref desc)) = b.locals.iter().find(|l| Some(l.0) == sym) =>
            {
                let desc = desc.clone();
                b.code.load(&desc, slot);
                super::callee::adapt_phantom(b, &desc);
                classfile::value_slots(&desc)
            }
            Tree::Select { qualifier, .. } if let Some(f) = sym.filter(|&s| !self.is_method(s)) => {
                self.field(b, f, (Some(qualifier), None), None)?
            }
            Tree::Ident { .. } if let Some(f) = sym.filter(|&s| self.is_class_field(s)) => {
                let prefix = self.ident_prefix(t);
                self.field(b, f, (None, prefix), None)?
            }
            Tree::Assign { lhs, rhs } => match (self.l.tpd.tree(lhs), self.l.tpd.sym(lhs)) {
                (&Tree::Select { qualifier, .. }, Some(f)) => {
                    self.field(b, f, (Some(qualifier), None), Some((rhs, t)))?
                }
                (&Tree::Ident { .. }, Some(f)) if self.is_class_field(f) => {
                    let prefix = self.ident_prefix(lhs);
                    self.field(b, f, (None, prefix), Some((rhs, t)))?
                }
                _ => match self.assign_local(b, t, (lhs, rhs))? {
                    Some(slots) => slots,
                    None => return unsupported(span, "an assignment to a variable of another method"),
                },
            },
            Tree::ValDef { rhs, .. } => self.gen_val_def(b, t, rhs)?,
            Tree::If { cond, thenp, elsep, .. } => self.gen_if(b, t, cond, thenp, elsep)?,
            Tree::WhileDo { cond, body } => self.gen_while(b, t, cond, body)?,
            Tree::Labeled { expr } => self.gen_labeled(b, t, expr)?,
            Tree::Return { expr, from } => self.gen_return(b, t, expr, from)?,
            Tree::Match { selector: Some(selector), cases, .. } => {
                self.gen_match(b, t, selector, cases, None)?
            }
            Tree::Try { expr, cases, finalizer } => self.gen_try(b, t, expr, cases, finalizer)?,
            Tree::Apply { fun, args } if self.is_synchronized(fun) => {
                let desc = self.desc_of(t);
                self.gen_synchronized(b, t, (fun, args), Some(desc))?
            }
            Tree::SeqLiteral { elems, .. } => self.array_literal(b, t, elems)?,
            Tree::TypeApply { fun, args } => self.type_test(b, t, fun, args)?,
            Tree::Apply { fun, args } => self.gen_apply(b, t, fun, args)?,
            Tree::Closure { env, meth, tpt } => self.closure(b, t, env, meth, tpt)?,
            tree => return unsupported(span, format!("{} on the JVM", typer::tpd::print::class_name(&tree))),
        })
    }

    fn is_method(&self, s: SymbolId) -> bool {
        self.l.u.symbol(s).kind == tasty::Kind::Def
    }

    fn is_class_field(&self, s: SymbolId) -> bool {
        !self.is_method(s) && self.l.u.symbol(s).owner.is_some_and(|o| self.l.u.is_class(o))
    }
}
