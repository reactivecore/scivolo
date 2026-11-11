// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//! Trait methods: the static forwarders that super calls invoke (dotty's `makeStaticForwarder`),
//! and the Java interfaces super calls name.

use super::callee::Receiver;
use super::descriptors::with_receiver;
use super::{Gen, method_name};
use classfile::{ACC_PRIVATE, ACC_STATIC, ClassFile, Code, Invoke};
use tasty::{SymbolId, name_kinds};
use typer::tpd::{List, TNodeId, Tree};

impl Gen<'_, '_> {
    /// Adds `m$` to trait `cf` for method `m` of access `flags`, descriptor `desc` and parameters
    /// `params` if `m` is a concrete instance method that is not private; its code is of the
    /// trait's source line `line`.
    pub fn trait_forwarder(
        &mut self,
        cf: &mut ClassFile,
        (m, flags): (SymbolId, u16),
        (desc, params): (&str, &[SymbolId]),
        line: Option<u16>,
    ) {
        let owner = self.l.u.symbol(m).owner.expect("a method's owner");
        if !self.l.u.is_trait(owner) || flags & (ACC_PRIVATE | ACC_STATIC) != 0 {
            return;
        }
        let (trait_name, name) = (self.class_name(owner), method_name(self.l.u, m));
        let (descs, result) = classfile::split_method(desc);
        let mut code = Code::new(1);
        if let Some(line) = line {
            code.line(line);
        }
        code.load("L", 0);
        for p in descs {
            let slot = code.max_locals;
            code.max_locals += classfile::value_slots(p) as u16;
            code.load(p, slot);
        }
        code.invoke_in(Invoke::Special, true, &mut cf.pool, (&trait_name, &name, desc));
        code.ret(result);
        let static_desc = with_receiver(&trait_name, desc);
        let flags = (flags & !classfile::ACC_ABSTRACT) | ACC_STATIC;
        let static_name = name_kinds::trait_super_accessor(&name);
        cf.method(flags, &static_name, &static_desc, Some(code), &mut |a, b| self.jvm_lub(a, b));
        self.method_attributes(cf, m, &static_desc, params, (true, true));
    }

    /// The Java interfaces whose methods class body `stats` calls on `super` via a trait, which the
    /// class names as interfaces (dotty's `superCallTargets`).
    pub fn super_call_targets(&mut self, stats: List) -> Vec<String> {
        let mut out = Vec::new();
        let mut todo: Vec<TNodeId> = self.l.tpd.items(stats).iter().rev().copied().collect();
        while let Some(t) = todo.pop() {
            match *self.l.tpd.tree(t) {
                Tree::TypeDef { .. } if self.l.tpd.class_template(t).is_some() => continue,
                Tree::Apply { fun, .. } => {
                    if let Some(target) = self.java_super_target(fun).filter(|t| !out.contains(t)) {
                        out.push(target);
                    }
                }
                _ => {}
            }
            todo.extend(self.l.tpd.children(t).into_iter().rev());
        }
        out
    }

    /// The Java interface owning the method `fun` selects on a `super` via a trait.
    pub fn java_super_target(&mut self, mut fun: TNodeId) -> Option<String> {
        while let Tree::TypeApply { fun: f, .. } = *self.l.tpd.tree(fun) {
            fun = f;
        }
        let Tree::Select { qualifier, .. } = *self.l.tpd.tree(fun) else { return None };
        let Tree::Super { .. } = self.l.tpd.tree(qualifier) else { return None };
        let m = self.l.tpd.sym(fun)?;
        let owner = self.l.u.symbol(m).owner?;
        let receiver = self.super_receiver(Some(qualifier), m, owner);
        let via_trait = self.l.u.is_trait(receiver);
        let java = via_trait && self.l.u.is_trait(owner) && self.is_java_class(owner);
        java.then(|| self.class_name(owner))
    }

    /// The class named in the descriptor of `m` if `qualifier` is a `super`, else `owner`; also
    /// `owner` for a trait's `m` called on a `super` without mixin (dotty's `adaptIfSuper`).
    pub fn super_receiver(&mut self, qualifier: Option<TNodeId>, m: SymbolId, owner: SymbolId) -> SymbolId {
        let qualifier = qualifier.filter(|&q| match *self.l.tpd.tree(q) {
            Tree::Super { mix, .. } => {
                let unmixed =
                    matches!(*self.l.tpd.tree(mix), Tree::Ident { name } if self.l.names.text(name.name).is_empty());
                !(unmixed && self.l.u.is_trait(owner))
            }
            _ => false,
        });
        match qualifier.and_then(|q| self.receiver_class(q, m)) {
            Some(Receiver::Class(c)) => c,
            _ => owner,
        }
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
