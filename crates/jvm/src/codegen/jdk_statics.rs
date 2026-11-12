//! The JDK classes' static members that javalib's objects stand for (ADR 0103, 0106).

use super::descriptors::{descriptor, internal, method_descriptor};
use super::{Gen, unsupported};
use classfile::{ACC_INTERFACE, ACC_PROTECTED, ACC_PUBLIC, ACC_STATIC};
use lower::Result;
use syntax::ast::Span;
use tasty::SymbolId;

impl Gen<'_, '_> {
    /// Reads the JDK's class of internal name `name` through the universe's JDK, else through
    /// the generator's.
    pub fn jdk_class(&mut self, name: &str) -> Option<&classfile::ClassInfo> {
        match self.l.u.jdk_mut() {
            Some(jdk) => jdk.class(name),
            None => self.jdk.class(name),
        }
    }

    /// The class declaring the members of class `owner` in class files: the JDK class an object
    /// standing for its static members names, then whether that is an interface.
    pub fn member_owner(&mut self, owner: SymbolId) -> (String, bool) {
        if !lower::is_jdk_object(self.l.u, owner) {
            return (self.class_name(owner), false);
        }
        let binary = self.l.u.binary_name(owner);
        let interface = self.l.u.companion_class(owner).is_some_and(|c| self.l.u.is_trait(c));
        (internal(binary.strip_suffix('$').unwrap_or(&binary)), interface)
    }

    /// Whether member `m` of an object standing for a JDK class is a static field: a value of the
    /// JDK declarations (`@static`) or of a Java class entered from the library jars, or a
    /// parameterless member the JDK's class file has as a static field of its type; else it is a
    /// static method. Fails for a member the JDK's class files have as neither.
    pub fn jdk_static_field(&mut self, m: SymbolId, span: Span) -> Result<bool> {
        let value = self.l.u.symbol(m).kind == tasty::Kind::Val;
        let owner = self.l.u.symbol(m).owner.expect("a member's owner");
        if self.l.u.is_annotated(m, "scala.annotation.static") || self.l.u.is_java_statics(owner) {
            return Ok(value);
        }
        let class = self.member_owner(owner).0;
        let name = tasty::encode(&self.l.u.symbol(m).name);
        let info = self.l.u.info(m);
        if value || self.l.u.param_clauses(info).is_empty() {
            let field = descriptor(&self.l.u.erase(info));
            if self.jdk_static(&class, &name, &field, true) {
                return Ok(true);
            }
        }
        let method = method_descriptor(self.l.u, m);
        if !value && self.jdk_static(&class, &name, &method, false) {
            return Ok(false);
        }
        if self.jdk_class(&class).is_none() {
            return unsupported(span, format!("JDK class {class}, missing from the JDK's class files"));
        }
        unsupported(span, format!("javalib's {class}.{name}, a static member the JDK lacks"))
    }

    /// Whether JDK class `class` has public or protected static field or method `name` of
    /// descriptor `desc`, found as the JVM resolves it (JVMS §5.4.3.2–5.4.3.4): a field in it or
    /// a class or interface it derives from, a method in it or, for a class, a superclass.
    fn jdk_static(&mut self, class: &str, name: &str, desc: &str, field: bool) -> bool {
        let mut todo = vec![class.to_owned()];
        while let Some(c) = todo.pop() {
            let Some(info) = self.jdk_class(&c) else { continue };
            let members = if field { &info.fields } else { &info.methods };
            let is = |m: &&classfile::MemberInfo| m.name == name && m.descriptor == desc;
            let visible = |m: &&classfile::MemberInfo| m.access & (ACC_PUBLIC | ACC_PROTECTED) != 0;
            if members.iter().any(|m| is(&m) && visible(&m) && m.access & ACC_STATIC != 0) {
                return true;
            }
            if field || info.access & ACC_INTERFACE == 0 {
                todo.extend(info.superclass.clone());
            }
            todo.extend(info.interfaces.iter().filter(|_| field).cloned());
        }
        false
    }
}
