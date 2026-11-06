// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//! Trait methods: the static forwarders that super calls invoke (dotty's `makeStaticForwarder`).

use super::{Gen, method_name};
use classfile::{ACC_PRIVATE, ACC_STATIC, ClassFile, Code, Invoke};
use tasty::SymbolId;

impl Gen<'_, '_> {
    /// Adds `m$` to trait `cf` for method `m` of access `flags` and descriptor `desc` if `m` is
    /// a concrete instance method that is not private.
    pub fn trait_forwarder(
        &mut self,
        cf: &mut ClassFile,
        m: SymbolId,
        flags: u16,
        desc: &str,
        params: &[SymbolId],
    ) {
        let owner = self.l.u.symbol(m).owner.expect("a method's owner");
        if !self.l.u.is_trait(owner) || flags & (ACC_PRIVATE | ACC_STATIC) != 0 {
            return;
        }
        let (trait_name, name) = (self.class_name(owner), method_name(self.l.u, m));
        let (descs, result) = classfile::split_method(desc);
        let mut code = Code::new(1);
        code.load("L", 0);
        for p in descs {
            let slot = code.max_locals;
            code.max_locals += classfile::value_slots(p) as u16;
            code.load(p, slot);
        }
        code.invoke_in(Invoke::Special, true, &mut cf.pool, (&trait_name, &name, desc));
        code.ret(result);
        let static_desc = format!("(L{trait_name};{}", &desc[1..]);
        let flags = (flags & !classfile::ACC_ABSTRACT) | ACC_STATIC;
        cf.method(flags, &format!("{name}$"), &static_desc, Some(code), &mut |a, b| self.jvm_lub(a, b));
        self.method_attributes(cf, m, &static_desc, params, (true, true));
    }

    /// Whether `c` is a Java class: of a JDK package or jar, or a Java class of the Scala library.
    pub fn is_java_class(&self, c: SymbolId) -> bool {
        let name = self.l.u.binary_name(c);
        name.starts_with("java.")
            || name.starts_with("javax.")
            || self.l.u.is_from_jdk_jars(c)
            || self.l.u.is_library_java_stub(c)
    }
}
