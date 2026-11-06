// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//! A class's skeleton: flags, parents, fields, methods, and an object's `MODULE$` and
//! `<clinit>` (dotty's `genPlainClass`).

use super::body::Body;
use super::clinit::StaticInit;
use super::{ClassBytes, Gen, internal, is_static_module, java_flags, method_descriptor, unsupported};
use classfile::{ACC_FINAL, ACC_PUBLIC, ACC_STATIC, ACC_VOLATILE, ClassFile, Last};
use lower::Result;
use tasty::SymbolId;
use tasty::tags::MUTABLE;
use typer::tpd::{TNodeId, Template, Tree};

impl Gen<'_, '_> {
    pub fn gen_class(&mut self, td: TNodeId) -> Result<()> {
        let sym = self.l.tpd.sym(td).expect("a class definition's symbol");
        let Some(Template { constr, body, .. }) = self.l.tpd.class_template(td) else {
            return unsupported(self.l.tpd.span(td), "a type definition that is not a class");
        };
        self.check_annotations(sym, self.l.tpd.span(td))?;
        let name = internal(&self.l.u.binary_name(sym));
        let (superclass, interfaces) = self.parents(sym);
        let interfaces: Vec<&str> = interfaces.iter().map(String::as_str).collect();
        let mut cf = ClassFile::new(java_flags(self.l, sym), &name, &superclass, &interfaces);
        let module = is_static_module(self.l.u, sym);
        let module_desc = format!("L{name};");
        if module {
            cf.field(ACC_PUBLIC | ACC_STATIC | ACC_FINAL, "MODULE$", &module_desc);
        }
        let module_name = module.then(|| name.clone());
        let mut init = StaticInit { module: module_name, constr: Some(constr), ..StaticInit::default() };
        let mut nested = Vec::new();
        for stat in std::iter::once(constr).chain(self.l.tpd.items(body).to_vec()) {
            let Some(s) = self.l.tpd.sym(stat) else { continue };
            match self.l.tpd.tree(stat) {
                Tree::DefDef { .. } if stat == constr && module => {
                    init.stats = self.gen_module_constructor(&mut cf, stat, s)?;
                }
                &Tree::DefDef { rhs, .. } if self.l.u.symbol(s).name == "<clinit>" => init.statics = rhs,
                Tree::DefDef { rhs: None, .. } if stat == constr && self.l.u.is_trait(sym) => {}
                Tree::DefDef { .. } if self.l.u.symbol(s).name == "$init$" => {
                    return unsupported(self.l.tpd.span(stat), "trait initializers on the JVM");
                }
                Tree::DefDef { .. } => self.gen_method(&mut cf, (td, stat), s)?,
                Tree::ValDef { .. } => {
                    self.check_annotations(s, self.l.tpd.span(stat))?;
                    let info = self.l.u.info(s);
                    let desc = super::descriptor(&self.l.u.erase(info));
                    let name = self.l.u.symbol(s).name.clone();
                    let mutable = self.l.u.symbol(s).flags.has(MUTABLE);
                    let volatile = if self.l.var_handle(s).is_some() { ACC_VOLATILE } else { 0 };
                    let flags = java_flags(self.l, s) | if mutable { volatile } else { ACC_FINAL };
                    cf.field(flags, &tasty::encode(&name), &desc);
                    if let Some(sig) = self.generic_signature(s, &desc) {
                        cf.member_signature(Last::Field, &sig);
                    }
                    if self.l.u.is_annotated(s, "scala.deprecated") {
                        cf.member_deprecated(Last::Field);
                    }
                    if self.l.var_handle_field(s).is_some() {
                        init.handles.push(s);
                    }
                }
                Tree::TypeDef { .. } if self.l.tpd.class_template(stat).is_some() => nested.push(stat),
                Tree::TypeDef { .. } | Tree::Import { .. } => {}
                _ => return unsupported(self.l.tpd.span(stat), "statements in a class body"),
            }
        }
        self.gen_clinit(&mut cf, &name, &init)?;
        self.deserialize_lambda(&mut cf);
        let top = self.l.u.symbol(sym).owner.is_some_and(|o| self.l.u.symbol(o).kind == tasty::Kind::Package);
        let companion = self.l.u.companion_module(sym).and_then(|m| self.l.u.module_class(m));
        if let Some(companion) = companion.filter(|&c| !module && is_static_module(self.l.u, c)) {
            self.add_forwarders(&mut cf, companion, Some(sym));
        }
        let signature = super::signatures::class(self.l.u, sym);
        cf.signature(&signature);
        if self.l.u.is_annotated(sym, "scala.deprecated") {
            cf.deprecated();
        }
        let source = self.source_name(td);
        cf.source_file(&source);
        let declared: Vec<SymbolId> = nested.iter().filter_map(|&n| self.l.tpd.sym(n)).collect();
        self.nested_attributes(&mut cf, sym, &declared);
        let bytes = cf.bytes().or_else(|e| unsupported(self.l.tpd.span(td), e))?;
        self.out.push(ClassBytes { name: name.clone(), bytes });
        if module && top && self.l.u.companion_class(sym).is_none() {
            self.gen_mirror_class(td, sym, &name, &declared)?;
        }
        nested.into_iter().try_for_each(|n| self.gen_class(n))
    }

    /// The superclass and interfaces of class `c`, as internal names; no interface another parent
    /// inherits.
    fn parents(&mut self, c: SymbolId) -> (String, Vec<String>) {
        let mut classes = Vec::new();
        for p in self.l.u.parents(c) {
            if let Some(s) = self.l.u.class_symbol(p) {
                classes.push((s, super::class_internal(&self.l.u.erase(p))));
            }
        }
        let (traits, classes): (Vec<_>, Vec<_>) = classes.into_iter().partition(|p| self.l.u.is_trait(p.0));
        let inherited = |u: &mut tasty::Universe, t| {
            classes.iter().chain(&traits).any(|&(o, _)| o != t && u.derives_from(o, t))
        };
        let traits: Vec<_> = traits.iter().filter(|p| !inherited(self.l.u, p.0)).cloned().collect();
        let superclass = match classes.first() {
            Some(p) if !self.l.u.is_trait(c) => p.1.clone(),
            _ => "java/lang/Object".into(),
        };
        let mut interfaces: Vec<String> = Vec::new();
        for (_, i) in traits {
            if i != "java/lang/Object" && !interfaces.contains(&i) {
                interfaces.push(i);
            }
        }
        let serializable = "java/io/Serializable";
        if self.l.u.is_module_class(c) && !interfaces.iter().any(|i| i == serializable) {
            interfaces.push(serializable.into());
        }
        (superclass, interfaces)
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
        let (desc, flags) = (method_descriptor(self.l.u, sym), java_flags(self.l, sym));
        let class = self.l.u.symbol(sym).owner.expect("a constructor's class");
        let mut b = Body::new(&mut cf.pool, None, 1);
        b.this_class = Some(self.class_name(class));
        for &t in &stats[..split] {
            self.stat(&mut b, t)?;
        }
        self.stat(&mut b, expr)?;
        b.code.ret("V");
        let code = b.code;
        cf.method(flags, "<init>", &desc, Some(code), &mut |a, b| self.jvm_lub(a, b));
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
