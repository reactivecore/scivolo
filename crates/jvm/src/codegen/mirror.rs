// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! The mirror class of a top-level object without a companion class: static forwarders to the
//! object's methods (dotty's `genMirrorClass`, `addForwarders`, `addForwarder`).

use super::{ClassBytes, Gen, method_descriptor, method_name, unsupported};
use classfile::{ACC_FINAL, ACC_PUBLIC, ACC_STATIC, ACC_SUPER, ClassFile, Code, FieldOp, Invoke};
use lower::Result;
use syntax::ast::Span;
use tasty::tags::{MACRO, PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified, STATIC};
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Template, Tree};

impl Gen<'_, '_> {
    pub fn gen_mirror_class(&mut self, td: TNodeId, module: SymbolId, module_name: &str) -> Result<()> {
        let name = module_name.strip_suffix('$').unwrap_or(module_name);
        let mut cf = ClassFile::new(ACC_PUBLIC | ACC_FINAL | ACC_SUPER, name, "java/lang/Object", &[]);
        let Some(Template { body, .. }) = self.l.tpd.class_template(td) else { unreachable!("a class") };
        let mut methods: Vec<(String, String)> = Vec::new();
        for &stat in self.l.tpd.items(body) {
            let Some(m) =
                self.l.tpd.sym(stat).filter(|_| matches!(self.l.tpd.tree(stat), Tree::DefDef { .. }))
            else {
                continue;
            };
            if self.has_forwarder(m, module) {
                methods.push((method_name(self.l.u, m), method_descriptor(self.l.u, m)));
            }
        }
        methods.sort();
        let module_desc = format!("L{module_name};");
        for (method, desc) in methods {
            let mut code = Code::new(0);
            code.field(FieldOp::GetStatic, &mut cf.pool, module_name, "MODULE$", &module_desc);
            let (params, result) = classfile::split_method(&desc);
            let mut slot = 0;
            for param in params {
                code.load(param, slot);
                slot += classfile::value_slots(param) as u16;
            }
            code.invoke(Invoke::Virtual, &mut cf.pool, module_name, &method, &desc);
            code.ret(result);
            cf.method(ACC_PUBLIC | ACC_STATIC, &method, &desc, Some(code), &mut |_, _| None);
        }
        let source = self.source_name(td);
        cf.source_file(&source);
        let bytes = cf.bytes().or_else(|e| unsupported(self.l.tpd.span(td), e))?;
        self.out.push(ClassBytes { name: name.to_owned(), bytes });
        Ok(())
    }

    /// Fails for top-level object class `module` if dotty gives it static forwarders this backend
    /// does not generate yet: those of methods `module` inherits, or, with a `companion` class,
    /// those in that class.
    pub fn check_forwarders(&mut self, module: SymbolId, companion: bool, span: Span) -> Result<()> {
        let plain =
            ["java.lang.Object", "scala.Any", "scala.AnyRef", "scala.Matchable", "java.io.Serializable"];
        for c in self.l.u.linearization(module) {
            if (c == module && !companion) || plain.contains(&self.l.u.full_name(c).as_str()) {
                continue;
            }
            let decls = self.l.u.decls(c).unwrap_or_default();
            let concrete = |m| self.l.u.symbol(m).kind == Kind::Def && !self.l.u.is_deferred(m);
            if decls.into_iter().any(|m| concrete(m) && self.has_forwarder(m, c)) {
                let what = if companion { "in a companion class" } else { "of inherited methods" };
                return unsupported(span, format!("static forwarders {what} on the JVM"));
            }
        }
        Ok(())
    }

    /// Whether the mirror class of `module` forwards to method `m` of `module`: neither a
    /// constructor, nor non-public, nor static, nor a macro, nor an expanded name.
    fn has_forwarder(&self, m: SymbolId, module: SymbolId) -> bool {
        let s = self.l.u.symbol(m);
        let excluded = [PRIVATE, PROTECTED, PRIVATEqualified, PROTECTEDqualified, STATIC, MACRO];
        let excluded = excluded.iter().any(|&f| s.flags.has(f));
        !excluded && !self.l.u.is_constructor(m) && s.owner == Some(module) && !s.name.contains("$$")
    }
}
