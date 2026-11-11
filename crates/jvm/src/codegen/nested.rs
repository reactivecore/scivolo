// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BTypeLoader.scala
//   compiler/src/dotty/tools/backend/jvm/BackendUtils.scala
//   compiler/src/dotty/tools/backend/jvm/BCodeUtils.scala
//! The `InnerClasses` and `EnclosingMethod` attributes (dotty's `buildNestedInfo`,
//! `addInnerClasses`, `enclosingMethodAttribute`).

use super::descriptors::method_descriptor;
use super::{Gen, java_flags, method_name};
use classfile::{ACC_FINAL, ACC_PUBLIC, ACC_STATIC, ACC_SUPER, ClassFile, InnerClass};
use names::nme;
use std::collections::BTreeMap;
use tasty::{Kind, SymbolId};
use typer::tpd::{TNodeId, Tree};

/// The flags an `InnerClasses` entry keeps: all but `ACC_SUPER` and `ACC_MODULE`.
const INNER_CLASSES_FLAGS: u16 = !(ACC_SUPER | 0x8000);

/// The class of the lookup argument of `invokedynamic` bootstrap methods.
const LOOKUP: &str = "java/lang/invoke/MethodHandles$Lookup";

impl Gen<'_, '_> {
    /// Adds to `cf`, of class `c` declaring classes `declared`, its `EnclosingMethod` attribute if
    /// local and its `InnerClasses` attribute; last, as they list the classes the others name.
    pub fn nested_attributes(&mut self, cf: &mut ClassFile, c: SymbolId, declared: &[SymbolId]) {
        if self.is_local_class(c) {
            let class = self.enclosing_class_of(c);
            let class = self.class_name(class);
            let method =
                self.enclosing_method(c).map(|m| (method_name(self.l.u, m), method_descriptor(self.l.u, m)));
            cf.enclosing_method(&class, method.as_ref().map(|(n, d)| (n.as_str(), d.as_str())));
        }
        let mut entries = BTreeMap::new();
        for &d in declared {
            if let Some((e, _)) = self.inner_entry(d) {
                entries.insert(e.inner.clone(), e);
            }
        }
        for name in cf.referenced_classes() {
            let mut next = self.resolve_class(&name);
            if next.is_none() && name == LOOKUP {
                let (outer, inner) = (Some("java/lang/invoke/MethodHandles".into()), Some("Lookup".into()));
                let flags = ACC_PUBLIC | ACC_STATIC | ACC_FINAL;
                entries.insert(name.clone(), InnerClass { inner: name, outer, name: inner, flags });
                continue;
            }
            while let Some((e, enclosing)) = next.and_then(|s| self.inner_entry(s)) {
                entries.insert(e.inner.clone(), e);
                next = Some(enclosing);
            }
        }
        if !entries.is_empty() {
            cf.inner_classes(&entries.into_values().collect::<Vec<_>>());
        }
    }

    /// The `InnerClasses` entry of class `d` and its enclosing class; none for a top-level class.
    fn inner_entry(&mut self, d: SymbolId) -> Option<(InnerClass, SymbolId)> {
        let enclosing = self.enclosing_class_of(d);
        if self.l.u.symbol(enclosing).kind == Kind::Package {
            return None;
        }
        let static_ = self.l.u.is_module_class(d) || self.is_static_owner(enclosing);
        let anonymous = self.l.u.symbol(d).name.contains(nme::ANON_CLASS);
        let outer = (!self.is_local_class(d)).then(|| {
            let name = self.class_name(enclosing);
            let top_module = self.l.u.is_module_class(enclosing)
                && self.l.u.symbol(enclosing).owner.is_some_and(|o| self.l.u.symbol(o).kind == Kind::Package);
            if top_module { name.strip_suffix('$').unwrap_or(&name).to_owned() } else { name }
        });
        let flags = java_flags(self.l, d) & !ACC_STATIC | if static_ { ACC_STATIC } else { 0 };
        let inner = InnerClass {
            inner: self.class_name(d),
            outer,
            name: (!anonymous).then(|| {
                let target = self.l.u.class_target(d);
                tasty::encode(target.as_deref().unwrap_or(self.l.original_name(d)))
            }),
            flags: flags & INNER_CLASSES_FLAGS,
        };
        Some((inner, enclosing))
    }

    /// The member classes of class `c`, not those lifted into it (dotty's `memberClasses`).
    pub fn member_classes(&self, c: SymbolId) -> Vec<SymbolId> {
        let decls = self.l.u.symbol(c).decls.iter().copied();
        decls.filter(|&d| self.l.u.symbol(d).kind == Kind::Class && !self.is_local_class(d)).collect()
    }

    /// The class or package lexically enclosing class `c` before local classes were lifted.
    fn enclosing_class_of(&self, c: SymbolId) -> SymbolId {
        let mut s = self.l.original_owner(c).expect("a class's owner");
        while !matches!(self.l.u.symbol(s).kind, Kind::Class | Kind::Package) {
            s = self.l.original_owner(s).expect("a term's owner");
        }
        s
    }

    /// Whether class `c` is anonymous or defined in a term (dotty's `isAnonymousOrLocalClass`).
    fn is_local_class(&self, c: SymbolId) -> bool {
        let owner = self.l.original_owner(c);
        self.l.u.symbol(c).name.contains(nme::ANON_CLASS)
            || owner.is_some_and(|o| self.l.u.symbol(o).kind.is_term())
    }

    /// The method local class `c` is defined in, the getter of a field whose initializer defines
    /// it, if any.
    fn enclosing_method(&self, c: SymbolId) -> Option<SymbolId> {
        let mut s = self.l.original_owner(c)?;
        loop {
            match self.l.u.symbol(s).kind {
                Kind::Class | Kind::Package => return None,
                Kind::Def if !self.l.u.symbol(s).flags.has(tasty::tags::SYNTHETIC) => {
                    return Some(self.l.lifted_method(s));
                }
                Kind::Val
                    if self.l.has_getter(s) && !self.l.u.symbol(s).flags.has(tasty::tags::SYNTHETIC) =>
                {
                    return Some(s);
                }
                _ => s = self.l.original_owner(s)?,
            }
        }
    }

    /// Whether the members of class `c` are static: a package, or an object reached through
    /// static paths.
    fn is_static_owner(&self, c: SymbolId) -> bool {
        let u = &self.l.u;
        u.symbol(c).kind == Kind::Package
            || u.is_module_class(c)
                && u.symbol(c).owner.is_none_or(|o| self.is_static_owner(self.enclosing_or_self(o)))
    }

    fn enclosing_or_self(&self, s: SymbolId) -> SymbolId {
        match self.l.u.symbol(s).kind {
            Kind::Class | Kind::Package => s,
            _ => self.enclosing_class_of(s),
        }
    }

    /// Records the classes unit `root` defines by internal name.
    pub fn collect_classes(&mut self, root: TNodeId) {
        let mut todo = vec![root];
        while let Some(t) = todo.pop() {
            match *self.l.tpd.tree(t) {
                Tree::PackageDef { stats, .. } => todo.extend(self.l.tpd.items(stats)),
                Tree::TypeDef { .. } if let Some(template) = self.l.tpd.class_template(t) => {
                    let c = self.l.tpd.sym(t).expect("a class definition's symbol");
                    self.classes.insert(self.class_name(c), Some(c));
                    todo.extend(self.l.tpd.items(template.body));
                }
                _ => {}
            }
        }
    }

    /// The class of internal name `name`, if the unit defines it or the universe has it.
    fn resolve_class(&mut self, name: &str) -> Option<SymbolId> {
        if let Some(&c) = self.classes.get(name) {
            return c;
        }
        let (pkg, rest) = name.rsplit_once('/').unwrap_or(("", name));
        let mut todo = self
            .l
            .u
            .package(&pkg.replace('/', "."))
            .map(|p| (p, rest.to_owned()))
            .into_iter()
            .collect::<Vec<_>>();
        let mut found = None;
        while let Some((owner, rest)) = todo.pop() {
            let cuts = rest.match_indices('$').map(|(i, _)| i).chain([rest.len()]);
            for cut in cuts.collect::<Vec<_>>() {
                let seg = &rest[..cut];
                let mut cands = self.l.u.decls_named(owner, seg);
                cands.extend(self.l.u.decls_named(owner, &tasty::module_class_name(seg)));
                let decls = self.l.u.symbol(owner).decls.iter().copied();
                let target = |t: String| t.strip_suffix('$').unwrap_or(&t) == seg;
                cands.extend(decls.filter(|&d| self.l.u.class_target(d).is_some_and(target)));
                for c in cands.into_iter().filter(|&c| self.l.u.symbol(c).kind == Kind::Class) {
                    let own = self.class_name(c);
                    if own == name {
                        found = Some(c);
                    } else if name.starts_with(&own) && cut < rest.len() {
                        todo.push((c, rest[cut + 1..].to_owned()));
                    }
                }
            }
        }
        self.classes.insert(name.to_owned(), found);
        found
    }
}
