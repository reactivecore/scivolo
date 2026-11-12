// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//! A class's skeleton: flags, parents, fields, methods, and an object's `MODULE$` and
//! `<clinit>` (dotty's `genPlainClass`).

use super::clinit::StaticInit;
use super::descriptors::{class_descriptor, class_internal, internal};
use super::{ClassBytes, Gen, java_flags, unsupported};
use classfile::{ACC_FINAL, ACC_PUBLIC, ACC_STATIC, ACC_VOLATILE, ClassFile, Last};
use lower::Result;
use names::nme;
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
        let targets = self.super_call_targets(body);
        let (superclass, interfaces) = self.parents(sym, &targets);
        let interfaces: Vec<&str> = interfaces.iter().map(String::as_str).collect();
        let mut cf = ClassFile::new(java_flags(self.l, sym), &name, &superclass, &interfaces);
        self.serial_version_uid(&mut cf, sym)?;
        let module = self.l.u.is_static_module(sym);
        let module_desc = class_descriptor(&name);
        if module {
            cf.field(ACC_PUBLIC | ACC_STATIC | ACC_FINAL, nme::MODULE_INSTANCE_FIELD, &module_desc);
        }
        let module_name = module.then(|| name.clone());
        let mut init = StaticInit {
            module: module_name,
            constr: Some(constr),
            class_def: Some(td),
            ..StaticInit::default()
        };
        let mut nested = Vec::new();
        for stat in std::iter::once(constr).chain(self.l.tpd.items(body).to_vec()) {
            let Some(s) = self.l.tpd.sym(stat) else { continue };
            match self.l.tpd.tree(stat) {
                Tree::DefDef { .. } if stat == constr && module => {
                    init.stats = self.gen_module_constructor(&mut cf, stat, s)?;
                }
                &Tree::DefDef { rhs, .. } if self.l.u.symbol(s).name == nme::STATIC_CONSTRUCTOR => {
                    init.statics = rhs
                }
                Tree::DefDef { rhs: None, .. } if stat == constr && self.l.u.is_trait(sym) => {}
                Tree::DefDef { .. } if self.l.u.symbol(s).name == nme::TRAIT_CONSTRUCTOR => {
                    return unsupported(self.l.tpd.span(stat), "trait initializers on the JVM");
                }
                Tree::DefDef { .. } => self.gen_method(&mut cf, (td, stat), s)?,
                Tree::ValDef { .. } => {
                    self.check_annotations(s, self.l.tpd.span(stat))?;
                    let desc = self.storage_descriptor(s);
                    let name = self.l.u.symbol(s).name.clone();
                    let mutable = self.l.u.symbol(s).flags.has(MUTABLE);
                    let volatile = if self.l.var_handle(s).is_some() { ACC_VOLATILE } else { 0 };
                    let flags = java_flags(self.l, s) | if mutable { volatile } else { ACC_FINAL };
                    let flags = flags | self.annotated_field_flags(s);
                    cf.field(flags, &tasty::encode(&name), &desc);
                    if let Some(sig) = self.generic_signature(s, &desc) {
                        cf.member_signature(Last::Field, &sig);
                    }
                    if self.is_deprecated(s) {
                        cf.member_deprecated(Last::Field);
                    }
                    if let Some(holder) = self.field_annotations_of(s) {
                        self.emit_annotations(&mut cf, Some(Last::Field), holder);
                    }
                    if self.l.var_handle_field(s).is_some() {
                        init.handles.push(s);
                    }
                }
                Tree::TypeDef { .. } if self.l.tpd.class_template(stat).is_some() => {
                    if module {
                        self.module_field(&mut cf, s);
                    }
                    nested.push(stat)
                }
                Tree::TypeDef { .. } | Tree::Import { .. } => {}
                _ => return unsupported(self.l.tpd.span(stat), "statements in a class body"),
            }
        }
        self.gen_clinit(&mut cf, &name, &init)?;
        self.deserialize_lambda(&mut cf);
        let top = self.l.u.symbol(sym).owner.is_some_and(|o| self.l.u.symbol(o).kind == tasty::Kind::Package);
        let companion = self.l.u.companion_module(sym).and_then(|m| self.l.u.module_class(m));
        if let Some(companion) = companion.filter(|&c| !module && self.l.u.is_static_module(c)) {
            self.add_forwarders(&mut cf, companion, Some(sym));
        }
        let original_top = self.original_top(sym);
        let signature = super::signatures::class(self.l.u, sym, original_top);
        cf.signature(&signature);
        if self.is_deprecated(sym) {
            cf.deprecated();
        }
        self.emit_annotations(&mut cf, None, sym);
        let source = self.source_name(td);
        cf.source_file(&source);
        let mut declared: Vec<SymbolId> = nested.iter().filter_map(|&n| self.l.tpd.sym(n)).collect();
        if let Some(companion) = companion.filter(|&c| top && !module && self.l.u.is_static_module(c)) {
            declared.extend(self.member_classes(companion));
        }
        self.nested_attributes(&mut cf, sym, &declared);
        let mirror = module && top && self.l.u.companion_class(sym).is_none();
        let tasty = self.tasty.remove(&sym);
        let (own, mirrored) = if mirror { (None, tasty) } else { (tasty, None) };
        let class =
            ClassBytes::new(name.clone(), &mut cf, own).or_else(|e| unsupported(self.l.tpd.span(td), e))?;
        self.out.push(class);
        if mirror {
            self.gen_mirror_class(td, (sym, &name), &declared, mirrored)?;
        }
        nested.into_iter().try_for_each(|n| self.gen_class(n))
    }

    /// The superclass and interfaces of class `c`, as internal names: no interface another parent
    /// inherits but super call targets `targets`, which are added.
    fn parents(&mut self, c: SymbolId, targets: &[String]) -> (String, Vec<String>) {
        let mut classes = Vec::new();
        for p in self.l.u.parents(c) {
            if let Some(s) = self.l.u.class_symbol(p) {
                classes.push((s, class_internal(&self.l.u.erase(p))));
            }
        }
        let (traits, classes): (Vec<_>, Vec<_>) = classes.into_iter().partition(|p| self.l.u.is_trait(p.0));
        let inherited = |u: &mut tasty::Universe, t| {
            classes.iter().chain(&traits).any(|&(o, _)| o != t && u.derives_from(o, t))
        };
        let traits: Vec<_> =
            traits.iter().filter(|p| !inherited(self.l.u, p.0) || targets.contains(&p.1)).cloned().collect();
        let superclass = match classes.first() {
            Some(p) if !self.l.u.is_trait(c) => p.1.clone(),
            _ => "java/lang/Object".into(),
        };
        let mut interfaces: Vec<String> = Vec::new();
        for i in traits.into_iter().map(|t| t.1).chain(targets.iter().cloned()) {
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

    /// Declares the static field of nested object `c` of a static object, which stays unassigned
    /// (dotty's `isStaticModuleField` for the module val).
    fn module_field(&mut self, cf: &mut ClassFile, c: SymbolId) {
        if !self.l.u.is_module_class(c) {
            return;
        }
        let Some(module) = self.l.u.source_module(c) else { return };
        let name = tasty::encode(&self.l.u.symbol(module).name.clone());
        let flags = java_flags(self.l, module) | ACC_STATIC | self.annotated_field_flags(module);
        cf.field(flags, &name, &class_descriptor(&self.class_name(c)));
        if self.is_deprecated(module) {
            cf.member_deprecated(Last::Field);
        }
    }

    pub fn class_name(&self, c: SymbolId) -> String {
        internal(&self.l.u.binary_name(c))
    }

    /// The file name of the source defining `t`.
    pub fn source_name(&self, t: TNodeId) -> String {
        let (file, _) = self.l.sources.locate(self.l.tpd.span(t).start());
        file.name.rsplit('/').next().unwrap_or_default().to_owned()
    }
}
