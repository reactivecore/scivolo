// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeBodyBuilder.scala
//! Method bodies: loads of literals, locals, `this` and objects (dotty's `genMethod` and
//! `genLoad`), straight-line code only.

use super::{Gen, class_internal, java_flags, method_descriptor, method_name, unsupported};
use classfile::{ACC_STATIC, ClassFile, Code, FieldOp, Pool};
use lower::Result;
use syntax::ast::constant::Constant;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

/// The code of the method being generated and the slots of its parameters and locals.
pub struct Body<'p> {
    pub code: Code,
    pub pool: &'p mut Pool,
    /// The class whose `this` is slot 0, none in a static method.
    this_class: Option<String>,
    /// The object whose `MODULE$` stands for `this` in its `<clinit>`.
    module: Option<String>,
    locals: Vec<(SymbolId, u16, String)>,
}

impl<'p> Body<'p> {
    /// A static method's body if `module` is given (an object's `<clinit>`), else that of a method
    /// whose parameters start at slot `first`.
    pub fn new(pool: &'p mut Pool, module: Option<&str>, first: u16) -> Body<'p> {
        let code = Code::new(first);
        Body { code, pool, this_class: None, module: module.map(str::to_owned), locals: Vec::new() }
    }

    fn bind(&mut self, s: SymbolId, desc: String) {
        let slot = self.code.max_locals;
        self.code.max_locals += classfile::value_slots(&desc) as u16;
        self.locals.push((s, slot, desc));
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
        let Some(rhs) = rhs else { return unsupported(self.l.tpd.span(dd), "abstract methods") };
        let mut b = Body::new(&mut cf.pool, None, u16::from(!static_));
        b.this_class = this_class;
        let params = self.l.tpd.clause_items(paramss).iter().flat_map(|&c| self.l.tpd.items(c));
        let params = params.filter(|&&p| matches!(self.l.tpd.tree(p), Tree::ValDef { .. }));
        let (param_descs, result) = classfile::split_method(&desc);
        for (&p, param) in params.zip(param_descs) {
            b.bind(self.l.tpd.sym(p).expect("a parameter's symbol"), param.to_owned());
        }
        match result {
            "V" => self.stat(&mut b, rhs)?,
            _ => _ = self.expr(&mut b, rhs)?,
        }
        b.code.ret(result);
        let code = b.code;
        cf.method(flags, &name, &desc, Some(code));
        Ok(())
    }

    /// Generates `t` and drops its value.
    pub fn stat(&mut self, b: &mut Body, t: TNodeId) -> Result<()> {
        match self.expr(b, t)? {
            0 => {}
            1 => b.code.pop(),
            _ => return unsupported(self.l.tpd.span(t), "a discarded long or double"),
        }
        Ok(())
    }

    /// Generates `t`; returns the stack slots its value takes.
    pub fn expr(&mut self, b: &mut Body, t: TNodeId) -> Result<i32> {
        let span = self.l.tpd.span(t);
        let sym = self.l.tpd.sym(t);
        Ok(match self.l.tpd.tree(t).clone() {
            Tree::Literal { value: Constant::Unit } => 0,
            Tree::Literal { value: Constant::Null } => {
                b.code.aconst_null();
                1
            }
            Tree::Literal { value: Constant::String(s) } => {
                let index = b.pool.string(&s);
                b.code.ldc(index);
                1
            }
            Tree::ClassLiteral { tpe } => {
                let e = self.l.u.erase(tpe);
                let index = b.pool.class(&class_internal(&e));
                b.code.ldc(index);
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
                self.load_module(b, class)
            }
            Tree::Ident { .. }
                if let Some(&(_, slot, ref desc)) = b.locals.iter().find(|l| Some(l.0) == sym) =>
            {
                let desc = desc.clone();
                b.code.load(&desc, slot);
                classfile::value_slots(&desc)
            }
            Tree::Select { qualifier, .. } if let Some(f) = sym.filter(|&s| !self.is_method(s)) => {
                self.field(b, f, qualifier, None)?
            }
            Tree::Assign { lhs, rhs } => match (self.l.tpd.tree(lhs), self.l.tpd.sym(lhs)) {
                (&Tree::Select { qualifier, .. }, Some(f)) => self.field(b, f, qualifier, Some(rhs))?,
                _ => return unsupported(span, "assignments to locals"),
            },
            Tree::Apply { fun, args } => self.apply(b, fun, args)?,
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
    pub fn load_module(&mut self, b: &mut Body, class: SymbolId) -> i32 {
        let name = self.class_name(class);
        match b.this_class.as_deref() == Some(&name) {
            true => b.code.load("L", 0),
            false => b.code.field(FieldOp::GetStatic, b.pool, &name, "MODULE$", &format!("L{name};")),
        }
        1
    }

    fn is_method(&self, s: SymbolId) -> bool {
        self.l.u.symbol(s).kind == tasty::Kind::Def
    }
}
