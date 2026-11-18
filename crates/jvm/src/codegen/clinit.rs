// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//! A class's `<clinit>`: its static values, an object's `MODULE$` and the statements of its
//! constructor (dotty's `genPlainClass`).

use super::body::Body;
use super::descriptors::{class_descriptor, method_descriptor};
use super::{Gen, java_flags, unsupported};
use classfile::{ACC_PRIVATE, ACC_PUBLIC, ACC_STATIC, ClassFile, FieldOp, Invoke};
use lower::Result;
use names::nme;
use tasty::SymbolId;
use typer::tpd::{TNodeId, Tree};

/// What a class's `<clinit>` runs, in this order.
#[derive(Default)]
pub struct StaticInit {
    /// The body of the `<clinit>` the lowering made, storing the static values and the
    /// `VarHandle`s of lazy values.
    pub statics: Option<TNodeId>,
    /// The object whose `MODULE$` is created, then the statements of its constructor.
    pub module: Option<String>,
    pub stats: Vec<TNodeId>,
    /// The class's constructor, whose line the code starts at unless an object's constructor has
    /// statements, which start it at the first one's.
    pub constr: Option<TNodeId>,
    /// The definition of an object written in the source, at whose name the code starts if its
    /// constructor starts with it, as that of an empty body does.
    pub class_def: Option<TNodeId>,
}

impl Gen<'_, '_> {
    /// The `<clinit>` of class `class` running `init`, if it runs anything; private where dotty
    /// takes `MoveStatics`'s.
    pub fn gen_clinit(&mut self, cf: &mut ClassFile, class: &str, init: &StaticInit) -> Result<()> {
        let moved = init.statics.is_some();
        if !moved && init.module.is_none() {
            return Ok(());
        }
        let mut b = Body::new(&mut cf.pool, init.module.as_deref(), 0);
        let first_stat =
            init.stats.iter().map(|&t| self.l.tpd.span(t)).filter(|s| s.exists()).map(|s| s.start()).min();
        match (first_stat, init.constr) {
            (Some(at), _) if init.module.is_some() => b.code.line(self.line_at(at)),
            (_, Some(constr))
                if let Some(def) = init
                    .class_def
                    .filter(|&d| self.l.tpd.span(d).start() == self.l.tpd.span(constr).start()) =>
            {
                self.line(&mut b, def)
            }
            (_, Some(constr)) => self.line(&mut b, constr),
            _ => {}
        }
        b.code.open_block();
        if let Some(t) = init.statics {
            self.stat(&mut b, t)?;
        }
        if init.module.is_some() {
            b.code.new_object(b.pool, class);
            b.code.dup();
            b.code.invoke(Invoke::Special, b.pool, class, nme::CONSTRUCTOR, "()V");
            let desc = class_descriptor(class);
            b.code.field(FieldOp::PutStatic, b.pool, class, nme::MODULE_INSTANCE_FIELD, &desc);
        }
        for &t in &init.stats {
            self.stat(&mut b, t)?;
        }
        b.code.ret("V");
        b.code.close_block();
        let code = b.code;
        let access = if moved { ACC_PRIVATE } else { ACC_PUBLIC };
        cf.method(access | ACC_STATIC, nme::STATIC_CONSTRUCTOR, "()V", Some(code));
        Ok(())
    }

    /// The constructor of a static object up to its superclass constructor call; returns the
    /// statements after it, which go to `<clinit>`.
    pub fn gen_module_constructor(
        &mut self,
        cf: &mut ClassFile,
        dd: TNodeId,
        sym: SymbolId,
    ) -> Result<Vec<TNodeId>> {
        let Tree::DefDef { rhs: Some(rhs), .. } = *self.l.tpd.tree(dd) else { unreachable!("a constructor") };
        let Tree::Block { stats, expr } = *self.l.tpd.tree(rhs) else {
            return unsupported(self.l.tpd.span(rhs), "a constructor body that is not a block");
        };
        let stats = self.l.tpd.items(stats).to_vec();
        let split = stats.iter().position(|&s| self.is_super_call(s)).map_or(0, |i| i + 1);
        let (desc, flags) = (method_descriptor(self.l.u, sym), java_flags(self.l, sym));
        let class = self.l.u.symbol(sym).owner.expect("a constructor's class");
        let mut b = Body::new(&mut cf.pool, None, 1);
        b.this_class = Some(self.class_name(class));
        let start = b.code.mark();
        b.code.open_block();
        for &t in &stats[..split] {
            self.stat(&mut b, t)?;
        }
        self.stat(&mut b, expr)?;
        b.code.ret("V");
        b.code.close_block();
        self.name_this_and_params(&mut b, start, Some(nme::THIS), &[]);
        let code = b.code;
        cf.method(flags, nme::CONSTRUCTOR, &desc, Some(code));
        Ok(stats[split..].to_vec())
    }

    /// Whether `t` calls a superclass constructor, last in a block of its lifted arguments (dotty's
    /// `splitAtSuper`).
    fn is_super_call(&self, t: TNodeId) -> bool {
        let t = match *self.l.tpd.tree(t) {
            Tree::Block { expr, .. } => expr,
            _ => t,
        };
        let Tree::Apply { fun, .. } = *self.l.tpd.tree(t) else { return false };
        let Tree::Select { qualifier, .. } = *self.l.tpd.tree(fun) else { return false };
        matches!(self.l.tpd.tree(qualifier), Tree::Super { .. })
            && self.l.tpd.sym(fun).is_some_and(|m| self.l.u.is_constructor(m))
    }
}
