// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BTypes.scala
//   compiler/src/dotty/tools/backend/jvm/PostProcessor.scala
//! The common superclass of two classes for stack map frames (dotty's `jvmWiseLUB`).

use super::Gen;
use tasty::SymbolId;

const OBJECT: &str = "java/lang/Object";

impl Gen<'_, '_> {
    /// The least upper bound of classes `a` and `b`, internal names, as the JVM sees them; none
    /// for a class the universe does not know.
    pub fn jvm_lub(&mut self, a: &str, b: &str) -> Option<String> {
        let (x, y) = (self.class_named(a)?, self.class_named(b)?);
        let u = &mut *self.l.u;
        let lub = match (u.is_trait(x), u.is_trait(y)) {
            (true, true) if u.derives_from(y, x) => a,
            (true, true) if u.derives_from(x, y) => b,
            (true, false) if u.derives_from(y, x) => a,
            (false, true) if u.derives_from(x, y) => b,
            (false, false) => return Some(self.common_superclass(x, y)),
            _ => OBJECT,
        };
        Some(lub.to_owned())
    }

    /// The first class of `x`'s superclass chain that `y` derives from.
    fn common_superclass(&mut self, x: SymbolId, y: SymbolId) -> String {
        let mut c = x;
        loop {
            let name = self.class_name(c);
            if name == OBJECT || self.l.u.derives_from(y, c) {
                return name;
            }
            let parents: Vec<SymbolId> =
                self.l.u.parents(c).into_iter().filter_map(|p| self.l.u.class_symbol(p)).collect();
            match parents.into_iter().find(|&p| !self.l.u.is_trait(p)) {
                Some(p) => c = p,
                None => return OBJECT.into(),
            }
        }
    }

    /// The class of internal name `internal`, if the universe knows it.
    fn class_named(&mut self, internal: &str) -> Option<SymbolId> {
        let binary = internal.replace('/', ".");
        let binary = lower::jdk_class_source_name(&binary);
        let (package, name) = binary.rsplit_once('.').unwrap_or(("", binary));
        let package = self.l.u.package(package)?;
        let decls = self.l.u.decl(package, name).ok()?;
        let class = decls.into_iter().find(|&s| self.l.u.is_class(s))?;
        (self.class_name(class) == internal).then_some(class)
    }
}
