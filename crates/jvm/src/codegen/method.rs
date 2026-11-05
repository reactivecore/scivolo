// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! Method bodies: loads of literals, locals, fields and objects (dotty's `genMethod`, `genLoad`).

use super::{Gen, class_internal, java_flags, method_descriptor, method_name, primitive_box, unsupported};
use classfile::{ACC_ABSTRACT, ACC_FINAL, ACC_STATIC, ClassFile, Code, FieldOp, Label, Pool};
use lower::Result;
use syntax::ast::Span;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

/// The code of the method being generated and the slots of its parameters and locals.
pub struct Body<'p> {
    pub code: Code,
    pub pool: &'p mut Pool,
    /// The class whose `this` is slot 0, none in a static method.
    pub this_class: Option<String>,
    /// The object whose `MODULE$` stands for `this` in its `<clinit>`.
    module: Option<String>,
    /// The parameters and locals: symbol, slot and descriptor.
    pub locals: Vec<(SymbolId, u16, String)>,
    /// The enclosing labeled blocks: label, end and the slots of the value.
    pub labels: Vec<(SymbolId, Label, i32)>,
    /// The method and the descriptor of its result.
    pub method: Option<SymbolId>,
    pub result: String,
}

impl<'p> Body<'p> {
    /// A static method's body if `module` is given (an object's `<clinit>`), else that of a method
    /// whose parameters start at slot `first`.
    pub fn new(pool: &'p mut Pool, module: Option<&str>, first: u16) -> Body<'p> {
        Body {
            code: Code::new(first),
            pool,
            this_class: None,
            module: module.map(str::to_owned),
            locals: Vec::new(),
            labels: Vec::new(),
            method: None,
            result: "V".into(),
        }
    }

    /// A slot for local `s` of descriptor `desc`.
    pub fn bind(&mut self, s: SymbolId, desc: String) -> u16 {
        let slot = self.code.max_locals;
        self.code.max_locals += classfile::value_slots(&desc) as u16;
        self.locals.push((s, slot, desc));
        slot
    }
}

impl Gen<'_, '_> {
    pub fn gen_method(&mut self, cf: &mut ClassFile, dd: TNodeId, sym: SymbolId) -> Result<()> {
        let Tree::DefDef { paramss, rhs, .. } = *self.l.tpd.tree(dd) else { unreachable!("a method") };
        let (desc, flags) = (method_descriptor(self.l.u, sym), java_flags(self.l.u, sym));
        let static_ = flags & ACC_STATIC != 0;
        let owner = self.l.u.symbol(sym).owner.expect("a method's owner");
        let this_class = (!static_).then(|| self.class_name(owner));
        let name = method_name(self.l.u, sym);
        let Some(rhs) = rhs else {
            let flags = (flags & !(ACC_FINAL | ACC_STATIC)) | ACC_ABSTRACT;
            cf.method(flags, &name, &desc, None, &mut |a, b| self.jvm_lub(a, b));
            return Ok(());
        };
        let mut b = Body::new(&mut cf.pool, None, u16::from(!static_));
        b.this_class = this_class;
        b.method = Some(sym);
        let params = self.l.tpd.clause_items(paramss).iter().flat_map(|&c| self.l.tpd.items(c));
        let params: Vec<TNodeId> =
            params.filter(|&&p| matches!(self.l.tpd.tree(p), Tree::ValDef { .. })).copied().collect();
        let (param_descs, result) = classfile::split_method(&desc);
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
        self.trait_forwarder(cf, sym, flags, &desc);
        Ok(())
    }

    /// Generates `t` and drops its value.
    pub fn stat(&mut self, b: &mut Body, t: TNodeId) -> Result<()> {
        self.expr_as(b, t, 0)
    }

    /// Generates `t`; returns the stack slots its value takes.
    pub fn expr(&mut self, b: &mut Body, t: TNodeId) -> Result<i32> {
        let span = self.l.tpd.span(t);
        let sym = self.l.tpd.sym(t);
        Ok(match self.l.tpd.tree(t).clone() {
            Tree::Literal { value } => self.literal(b, &value),
            Tree::ClassLiteral { tpe } => {
                let e = self.l.u.erase(tpe);
                match primitive_box(&e) {
                    Some(class) => {
                        b.code.field(FieldOp::GetStatic, b.pool, class, "TYPE", "Ljava/lang/Class;")
                    }
                    None => {
                        let index = b.pool.class(&class_internal(&e));
                        b.code.ldc(index);
                    }
                }
                1
            }
            Tree::Typed { expr, .. } => self.expr(b, expr)?,
            Tree::Block { stats, expr } => {
                for s in self.l.tpd.items(stats).to_vec() {
                    self.stat(b, s)?;
                }
                self.expr(b, expr)?
            }
            Tree::This { .. } => self.load_this(b),
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
                    self.field(b, f, (Some(qualifier), None), Some(rhs))?
                }
                (&Tree::Ident { .. }, Some(f)) if self.is_class_field(f) => {
                    let prefix = self.ident_prefix(lhs);
                    self.field(b, f, (None, prefix), Some(rhs))?
                }
                _ => match self.assign_local(b, lhs, rhs)? {
                    Some(slots) => slots,
                    None => return unsupported(span, "an assignment to a variable of another method"),
                },
            },
            Tree::ValDef { rhs, .. } => self.gen_val_def(b, t, rhs)?,
            Tree::If { cond, thenp, elsep, .. } => self.gen_if(b, t, cond, thenp, elsep)?,
            Tree::WhileDo { cond, body } => self.gen_while(b, t, cond, body)?,
            Tree::Labeled { expr } => self.gen_labeled(b, t, expr)?,
            Tree::Return { expr, from } => self.gen_return(b, t, expr, from)?,
            Tree::SeqLiteral { elems, .. } => self.array_literal(b, t, elems)?,
            Tree::TypeApply { fun, args } => self.type_test(b, t, fun, args)?,
            Tree::Apply { fun, args } => match self.primitive(b, t)? {
                Some(slots) => slots,
                None => match self.array_op(b, t, fun, args)? {
                    Some(slots) => slots,
                    None => self.apply(b, fun, args)?,
                },
            },
            tree => return unsupported(span, format!("{} on the JVM", typer::tpd::print::class_name(&tree))),
        })
    }

    pub fn load_this(&mut self, b: &mut Body) -> i32 {
        match (&b.this_class, &b.module) {
            (_, Some(module)) => {
                let module = module.clone();
                b.code.field(FieldOp::GetStatic, b.pool, &module, "MODULE$", &format!("L{module};"));
            }
            _ => b.code.load("L", 0),
        }
        1
    }

    /// Loads object class `class`: `this` in its own instance methods (dotty's `genLoadModule`).
    pub fn load_module(&mut self, b: &mut Body, class: SymbolId, span: Span) -> Result<i32> {
        if lower::is_jdk_object(self.l.u, class) {
            return unsupported(span, "objects standing for JDK classes on the JVM");
        }
        let name = self.class_name(class);
        match b.this_class.as_deref() == Some(&name) {
            true => b.code.load("L", 0),
            false => b.code.field(FieldOp::GetStatic, b.pool, &name, "MODULE$", &format!("L{name};")),
        }
        Ok(1)
    }

    fn is_method(&self, s: SymbolId) -> bool {
        self.l.u.symbol(s).kind == tasty::Kind::Def
    }

    fn is_class_field(&self, s: SymbolId) -> bool {
        !self.is_method(s) && self.l.u.symbol(s).owner.is_some_and(|o| self.l.u.is_class(o))
    }
}
