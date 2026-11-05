// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//! A class's skeleton: flags, parents, fields, methods, and an object's `MODULE$` and
//! `<clinit>` (dotty's `genPlainClass`).

use super::method::Body;
use super::{ClassBytes, Gen, internal, is_static_module, java_flags, method_descriptor, unsupported};
use classfile::{ACC_FINAL, ACC_PUBLIC, ACC_STATIC, ClassFile, FieldOp, Invoke};
use lower::Result;
use tasty::tags::MUTABLE;
use tasty::{SymbolId, Type};
use typer::tpd::{TNodeId, Template, Tree};

impl Gen<'_, '_> {
    pub fn gen_class(&mut self, td: TNodeId) -> Result<()> {
        let sym = self.l.tpd.sym(td).expect("a class definition's symbol");
        let Some(Template { constr, body, .. }) = self.l.tpd.class_template(td) else {
            return unsupported(self.l.tpd.span(td), "a type definition that is not a class");
        };
        let name = internal(&self.l.u.binary_name(sym));
        let (superclass, interfaces) = self.parents(sym);
        let interfaces: Vec<&str> = interfaces.iter().map(String::as_str).collect();
        let mut cf = ClassFile::new(java_flags(self.l.u, sym), &name, &superclass, &interfaces);
        let module = is_static_module(self.l.u, sym);
        let module_desc = format!("L{name};");
        if module {
            cf.field(ACC_PUBLIC | ACC_STATIC | ACC_FINAL, "MODULE$", &module_desc);
        }
        let mut clinit_stats = Vec::new();
        for stat in std::iter::once(constr).chain(self.l.tpd.items(body).to_vec()) {
            let Some(s) = self.l.tpd.sym(stat) else { continue };
            match self.l.tpd.tree(stat) {
                Tree::DefDef { .. } if stat == constr && module => {
                    clinit_stats = self.gen_module_constructor(&mut cf, stat, s)?;
                }
                Tree::DefDef { .. } => self.gen_method(&mut cf, stat, s)?,
                Tree::ValDef { .. } => {
                    let info = self.l.u.info(s);
                    let desc = super::descriptor(&self.l.u.erase(info));
                    let name = self.l.u.symbol(s).name.clone();
                    let mutable = self.l.u.symbol(s).flags.has(MUTABLE);
                    let flags = java_flags(self.l.u, s) | if mutable { 0 } else { ACC_FINAL };
                    cf.field(flags, &tasty::encode(&name), &desc);
                }
                Tree::TypeDef { .. } | Tree::Import { .. } => {}
                _ => return unsupported(self.l.tpd.span(stat), "statements in a class body"),
            }
        }
        if module {
            let mut b = Body::new(&mut cf.pool, Some(&name), 0);
            b.code.new_object(b.pool, &name);
            b.code.dup();
            b.code.invoke(Invoke::Special, b.pool, &name, "<init>", "()V");
            b.code.field(FieldOp::PutStatic, b.pool, &name, "MODULE$", &module_desc);
            for &t in &clinit_stats {
                self.stat(&mut b, t)?;
            }
            b.code.ret("V");
            let code = b.code;
            cf.method(ACC_PUBLIC | ACC_STATIC, "<clinit>", "()V", Some(code));
        }
        if let Some(signature) = self.class_signature(sym) {
            cf.signature(&signature);
        }
        let source = self.source_name(td);
        cf.source_file(&source);
        let bytes = cf.bytes().or_else(|e| unsupported(self.l.tpd.span(td), e))?;
        self.out.push(ClassBytes { name: name.clone(), bytes });
        let top = self.l.u.symbol(sym).owner.is_some_and(|o| self.l.u.symbol(o).kind == tasty::Kind::Package);
        if module && top && self.l.u.companion_class(sym).is_none() {
            self.gen_mirror_class(td, sym, &name)?;
        }
        Ok(())
    }

    /// The superclass and interfaces of class `c`, as internal names.
    fn parents(&mut self, c: SymbolId) -> (String, Vec<String>) {
        let classes: Vec<SymbolId> =
            self.l.u.parents(c).into_iter().filter_map(|p| self.l.u.class_symbol(p)).collect();
        let (traits, classes): (Vec<_>, Vec<_>) = classes.into_iter().partition(|&s| self.l.u.is_trait(s));
        let superclass = classes.first().map_or_else(|| "java/lang/Object".into(), |&s| self.class_name(s));
        let mut interfaces: Vec<String> = traits.into_iter().map(|t| self.class_name(t)).collect();
        let serializable = "java/io/Serializable";
        if self.l.u.is_module_class(c) && !interfaces.iter().any(|i| i == serializable) {
            interfaces.push(serializable.into());
        }
        (superclass, interfaces)
    }

    /// The generic signature of class `c` without type parameters and with one parent that takes
    /// no type arguments (dotty's `superSig`); none for other classes.
    fn class_signature(&mut self, c: SymbolId) -> Option<String> {
        let [parent] = self.l.u.parents(c)[..] else { return None };
        let plain =
            self.l.u.type_params(c).is_empty() && !matches!(self.l.u.types().get(parent), Type::Applied(..));
        let class = self.l.u.class_symbol(parent).filter(|_| plain)?;
        let object = if self.l.u.is_trait(class) { "Ljava/lang/Object;" } else { "" };
        Some(format!("{object}L{};", self.class_name(class)))
    }

    pub fn class_name(&self, c: SymbolId) -> String {
        internal(&self.l.u.binary_name(c))
    }

    /// The constructor of a static object up to its superclass constructor call; returns the
    /// statements after it, which go to `<clinit>`.
    fn gen_module_constructor(
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
        let (desc, flags) = (method_descriptor(self.l.u, sym), java_flags(self.l.u, sym));
        let mut b = Body::new(&mut cf.pool, None, 1);
        for &t in &stats[..split] {
            self.stat(&mut b, t)?;
        }
        self.stat(&mut b, expr)?;
        b.code.ret("V");
        let code = b.code;
        cf.method(flags, "<init>", &desc, Some(code));
        Ok(stats[split..].to_vec())
    }

    /// Whether `t` calls a superclass constructor (dotty's `isSuperConstrCall`).
    fn is_super_call(&self, t: TNodeId) -> bool {
        let Tree::Apply { fun, .. } = *self.l.tpd.tree(t) else { return false };
        let Tree::Select { qualifier, .. } = *self.l.tpd.tree(fun) else { return false };
        matches!(self.l.tpd.tree(qualifier), Tree::Super { .. })
            && self.l.tpd.sym(fun).is_some_and(|m| self.l.u.is_constructor(m))
    }

    /// The file name of the source defining `t`.
    pub fn source_name(&self, t: TNodeId) -> String {
        let (file, _) = self.l.sources.locate(self.l.tpd.span(t).start());
        file.name.rsplit('/').next().unwrap_or_default().to_owned()
    }
}
