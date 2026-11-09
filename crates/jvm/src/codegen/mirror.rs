// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! Static forwarders to the methods of a top-level object, in its companion class or in a mirror
//! class (dotty's `genMirrorClass`, `addForwarders`, `addForwarder`).

use super::{ClassBytes, Gen, method_descriptor, method_name, unsupported};
use classfile::{
    ACC_FINAL, ACC_PUBLIC, ACC_STATIC, ACC_SUPER, ACC_SYNTHETIC, ClassFile, Code, FieldOp, Invoke, Last,
};
use lower::Result;
use std::collections::HashSet;
use tasty::tags::{FIELDaccessor, MACRO, PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified, STATIC};
use tasty::{Kind, SymbolId};
use typer::tpd::TNodeId;

impl Gen<'_, '_> {
    /// The mirror class of top-level object class `module` named `module_name`, which declares
    /// classes `declared`, with the object's TASTy `tasty`.
    pub fn gen_mirror_class(
        &mut self,
        td: TNodeId,
        (module, module_name): (SymbolId, &str),
        declared: &[SymbolId],
        tasty: Option<Vec<u8>>,
    ) -> Result<()> {
        let name = module_name.strip_suffix('$').unwrap_or(module_name);
        let mut cf = ClassFile::new(ACC_PUBLIC | ACC_FINAL | ACC_SUPER, name, "java/lang/Object", &[]);
        self.add_forwarders(&mut cf, module, None);
        let source = self.source_name(td);
        cf.source_file(&source);
        self.nested_attributes(&mut cf, module, declared);
        let class = ClassBytes::new(name.to_owned(), &mut cf, tasty)
            .or_else(|e| unsupported(self.l.tpd.span(td), e))?;
        self.out.push(class);
        Ok(())
    }

    /// Adds to `cf` the static forwarders to the methods of object class `module`, but those
    /// whose names a member of its companion class `companion` has.
    pub fn add_forwarders(&mut self, cf: &mut ClassFile, module: SymbolId, companion: Option<SymbolId>) {
        let module_name = self.class_name(module);
        let module_desc = format!("L{module_name};");
        let mut methods = self.forwarded(module, companion);
        methods.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
        for (method, desc, synthetic, m) in methods {
            let mut code = Code::new(0);
            code.field(FieldOp::GetStatic, &mut cf.pool, &module_name, "MODULE$", &module_desc);
            let (params, result) = classfile::split_method(&desc);
            let mut slot = 0;
            for param in params {
                code.load(param, slot);
                slot += classfile::value_slots(param) as u16;
            }
            code.invoke(Invoke::Virtual, &mut cf.pool, &module_name, &method, &desc);
            code.ret(result);
            let flags = ACC_PUBLIC | ACC_STATIC | if synthetic { ACC_SYNTHETIC } else { 0 };
            cf.method(flags, &method, &desc, Some(code), &mut |_, _| None);
            let target = self.l.mixed_in_from(m).unwrap_or(m);
            let this = self.l.u.this_type(module);
            let info = self.l.u.member_info(this, target);
            self.exceptions(cf, m);
            let thrown = self.thrown(m);
            if !self.l.is_mixin_accessor(m)
                && let Some(sig) = super::signatures::member(self.l.u, (target, target), info, &desc, &thrown)
                && super::signatures::erases_to(&sig, &desc)
            {
                cf.member_signature(Last::Method, &sig);
            }
        }
    }

    /// The names and descriptors of the methods of `module` forwarded to, each with whether its
    /// forwarder is synthetic: the members dotty's `addForwarders` takes.
    fn forwarded(
        &mut self,
        module: SymbolId,
        companion: Option<SymbolId>,
    ) -> Vec<(String, String, bool, SymbolId)> {
        let mut conflicting = HashSet::new();
        for c in companion.map(|c| self.l.u.linearization(c)).unwrap_or_default() {
            for m in self.l.u.decls(c).unwrap_or_default() {
                if self.l.u.symbol(m).kind.is_term() {
                    conflicting.insert(method_name(self.l.u, m));
                }
            }
        }
        let plain = ["java.lang.Object", "scala.Any", "scala.AnyRef", "scala.Matchable"];
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for c in self.l.u.linearization(module) {
            let full = self.l.u.full_name(c);
            if plain.contains(&full.as_str()) || full.starts_with("scala.scalanative.") {
                continue;
            }
            for m in self.l.u.decls(c).unwrap_or_default() {
                if !self.is_method_member(m) {
                    continue;
                }
                let (name, desc) = (method_name(self.l.u, m), method_descriptor(self.l.u, m));
                if !seen.insert((name.clone(), desc.clone())) || !self.has_forwarder(m) {
                    continue;
                }
                if self.l.is_bridge(m) && !self.overrides_concrete(c, &name, &desc) {
                    continue;
                }
                if !conflicting.contains(&name) {
                    let synthetic = self.l.is_bridge(m) || name.contains("$_setter_$");
                    out.push((name, desc, synthetic, m));
                }
            }
        }
        out
    }

    /// Whether a bridge of class `c` named `name` with descriptor `desc` overrides a method that
    /// is not deferred (dotty's `nextOverriddenSymbol` of a bridge).
    fn overrides_concrete(&mut self, c: SymbolId, name: &str, desc: &str) -> bool {
        for b in self.l.u.linearization(c).into_iter().skip(1) {
            for m in self.l.u.decls(b).unwrap_or_default() {
                if self.is_method_member(m)
                    && method_name(self.l.u, m) == name
                    && method_descriptor(self.l.u, m) == desc
                {
                    return self.has_forwarder(m);
                }
            }
        }
        false
    }

    /// Whether `m` is a method or a getter, defined in this unit or in a library.
    fn is_method_member(&self, m: SymbolId) -> bool {
        let s = self.l.u.symbol(m);
        let library = s.tree.is_some();
        s.kind == Kind::Def || s.flags.has(FIELDaccessor) || library && s.kind == Kind::Val
    }

    /// Whether a forwarder calls member method `m`: neither deferred, a constructor, non-public,
    /// static, a macro, erased, lifted, nor of an expanded name.
    fn has_forwarder(&mut self, m: SymbolId) -> bool {
        let s = self.l.u.symbol(m);
        let excluded = [PRIVATE, PROTECTED, PRIVATEqualified, PROTECTEDqualified, STATIC, MACRO];
        let excluded = excluded.iter().any(|&f| s.flags.has(f)) || s.name.contains("$$");
        let deferred = match s.tree.is_some() {
            true => self.l.u.body(m).is_none(),
            false => self.l.u.is_deferred(m),
        };
        !excluded
            && !deferred
            && !self.l.u.is_constructor(m)
            && !lower::is_erased(self.l.u, m)
            && !self.l.is_lifted(m)
    }
}
