// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BTypes.scala
//   compiler/src/dotty/tools/backend/jvm/PostProcessor.scala
//! The common superclass of two classes for stack map frames (dotty's `jvmWiseLUB`).

use super::Gen;
use tasty::{SymbolId, name_kinds};

const OBJECT: &str = "java/lang/Object";

/// A class as the JVM sees it: internal names.
struct Hierarchy {
    interface: bool,
    superclass: Option<String>,
    interfaces: Vec<String>,
}

impl Gen<'_, '_> {
    /// The least upper bound of classes `a` and `b`, internal names, as the JVM sees them; none
    /// for a class neither the universe nor the JDK's class files know.
    pub fn jvm_lub(&mut self, a: &str, b: &str) -> Option<String> {
        let (x, y) = (self.hierarchy(a)?, self.hierarchy(b)?);
        let lub = match (x.interface, y.interface) {
            (true, true) if self.is_subtype(b, a)? => a,
            (true, true) if self.is_subtype(a, b)? => b,
            (true, false) if self.is_subtype(b, a)? => a,
            (false, true) if self.is_subtype(a, b)? => b,
            (false, false) => {
                let (ca, cb) = (self.superclasses(a)?, self.superclasses(b)?);
                let common = ca.iter().zip(&cb).skip(1).take_while(|(p, q)| p == q).last();
                return Some(common.map_or(OBJECT, |(p, _)| p).to_owned());
            }
            _ => OBJECT,
        };
        Some(lub.to_owned())
    }

    /// Whether class `a` is `b` or derives from it (dotty's `isSubtypeOf`).
    fn is_subtype(&mut self, a: &str, b: &str) -> Option<bool> {
        if a == b {
            return Some(true);
        }
        let (h, b_interface) = (self.hierarchy(a)?, self.hierarchy(b)?.interface);
        if h.interface && b == OBJECT {
            return Some(true);
        }
        if !h.interface
            && let Some(s) = &h.superclass
            && self.is_subtype(s, b)?
        {
            return Some(true);
        }
        if !b_interface {
            return Some(false);
        }
        for i in &h.interfaces {
            if self.is_subtype(i, b)? {
                return Some(true);
            }
        }
        Some(false)
    }

    /// The superclass chain of `a`, `Object` first and `a` last.
    fn superclasses(&mut self, a: &str) -> Option<Vec<String>> {
        let mut chain = vec![a.to_owned()];
        while let Some(s) = self.hierarchy(chain.last()?)?.superclass {
            chain.push(s);
        }
        chain.reverse();
        Some(chain)
    }

    /// Class `name` as the JDK's class files have it for a `java` or `javax` package, else as the
    /// universe has it; the other where the first lacks it.
    fn hierarchy(&mut self, name: &str) -> Option<Hierarchy> {
        let jdk_first = name.starts_with("java/") || name.starts_with("javax/");
        if !jdk_first && let Some(c) = self.class_named(name) {
            return Some(self.universe_hierarchy(c, name));
        }
        if let Some(c) = self.jdk.class(name) {
            let interface = c.access & classfile::ACC_INTERFACE != 0;
            return Some(Hierarchy {
                interface,
                superclass: c.superclass.clone(),
                interfaces: c.interfaces.clone(),
            });
        }
        let c = self.class_named(name).filter(|_| jdk_first)?;
        Some(self.universe_hierarchy(c, name))
    }

    fn universe_hierarchy(&mut self, c: SymbolId, name: &str) -> Hierarchy {
        let parents: Vec<SymbolId> =
            self.l.u.parents(c).into_iter().filter_map(|p| self.l.u.class_symbol(p)).collect();
        let tuple = |g: &mut Self, p| g.l.u.binary_name(p) == "scala.*:";
        let (traits, classes): (Vec<SymbolId>, Vec<SymbolId>) =
            parents.into_iter().partition(|&p| self.l.u.is_trait(p) || tuple(self, p));
        let superclass = classes.first().map(|&p| self.class_name(p));
        let superclass = superclass.map(|s| if s == "scala/AnyVal" { OBJECT.into() } else { s });
        let interfaces = traits.into_iter().map(|t| match tuple(self, t) {
            true => "scala/Product".to_owned(),
            false => self.class_name(t),
        });
        let interfaces = interfaces.collect();
        Hierarchy {
            interface: self.l.u.is_trait(c),
            superclass: superclass.or_else(|| Some(OBJECT.into())).filter(|_| name != OBJECT),
            interfaces,
        }
    }

    /// The class of internal name `internal`, if the universe knows it.
    fn class_named(&mut self, internal: &str) -> Option<SymbolId> {
        let binary = internal.replace('/', ".");
        let binary = lower::jdk_class_source_name(&binary);
        let (package, name) = binary.rsplit_once('.').unwrap_or(("", binary));
        let package = self.l.u.package(package)?;
        if let Some(c) = self.member_class_named(package, name, internal) {
            return Some(c);
        }
        let (prefix, _) = name.split_once(&name_kinds::EXPAND_SEPARATOR.repeat(2))?;
        let tops = self.l.u.decls(package).unwrap_or_default().into_iter();
        let outer = |n: &str| n.starts_with(prefix) || prefix.starts_with(&name_kinds::flat(n, ""));
        let tops: Vec<_> = tops.filter(|&c| outer(&self.l.u.symbol(c).name)).collect();
        tops.into_iter().find_map(|c| self.nested_class_named(c, internal))
    }

    /// The class of internal name `internal` among `c` and the classes nested in it.
    fn nested_class_named(&mut self, c: SymbolId, internal: &str) -> Option<SymbolId> {
        if !self.l.u.is_class(c) {
            return None;
        }
        if self.class_name(c) == internal {
            return Some(c);
        }
        let decls = self.l.u.decls(c).unwrap_or_default();
        decls.into_iter().find_map(|d| self.nested_class_named(d, internal))
    }

    /// The class of internal name `internal` declared in `owner` as `rest`, nested classes
    /// separated by `$` and object classes ending in it, each name as declared or decoded.
    fn member_class_named(&mut self, owner: SymbolId, rest: &str, internal: &str) -> Option<SymbolId> {
        let ends = rest.match_indices('$').map(|(i, _)| i).chain([rest.len()]);
        for end in ends.collect::<Vec<_>>() {
            let names = [(rest.get(..=end), end + 1), (Some(&rest[..end]), end + 1)];
            for (name, next) in names.into_iter().filter_map(|(n, next)| Some((n?, next))) {
                let mut found = self.l.u.decls_named(owner, name);
                if found.is_empty() {
                    found = self.l.u.decls_named(owner, &tasty::decode(name));
                }
                for c in found {
                    let found = match rest.get(next..).filter(|r| !r.is_empty()) {
                        _ if !self.l.u.is_class(c) => None,
                        Some(inner) => self.member_class_named(c, inner, internal),
                        None => (self.class_name(c) == internal).then_some(c),
                    };
                    if found.is_some() {
                        return found;
                    }
                }
            }
        }
        None
    }
}
