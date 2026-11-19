// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeHelpers.scala
//! The methods of a top-level object that get static forwarders (dotty's `addForwarders`).

use super::descriptors::method_descriptor;
use super::{Gen, method_name};
use names::name_kinds;
use std::collections::{HashMap, HashSet};
use tasty::tags::{
    FIELDaccessor, INLINE, MACRO, PRIVATE, PRIVATEqualified, PROTECTED, PROTECTEDqualified, STATIC,
};
use tasty::{Kind, PRIMITIVE_VALUE_CLASSES, SymbolId};

/// A method of an object that gets a static forwarder.
pub struct Forwarded {
    pub name: String,
    pub desc: String,
    /// Whether the forwarder is `ACC_SYNTHETIC`: that of a bridge or a trait setter.
    pub synthetic: bool,
    pub method: SymbolId,
    /// The method whose type, signature and flags the forwarder takes: the method a bridge
    /// overrides, the trait or JDK method a mixin forwarder calls, else `method`.
    pub target: SymbolId,
}

impl Gen<'_, '_> {
    /// Collects the methods of `module` forwarded to, sorted by source name and descriptor: the members
    /// dotty's `addForwarders` takes, but those of names a term member of the erased companion
    /// class `companion` has (a primitive class's has no parents). The library classes compiled
    /// in the same run get their bridges and mixin forwarders first.
    pub fn forwarded(&mut self, module: SymbolId, companion: Option<SymbolId>) -> Vec<Forwarded> {
        let conflicting = companion.map(|c| self.conflicting_names(c)).unwrap_or_default();
        let plain = ["java.lang.Object", "scala.Any", "scala.AnyRef", "scala.Matchable"];
        let mut bases = self.l.u.linearization(module);
        bases.retain(|&c| {
            let full = self.l.u.full_name(c);
            !plain.contains(&full.as_str()) && !full.starts_with("scala.scalanative.")
        });
        for &c in &bases {
            self.l.enter_specialized_applies(c);
            lower::same_run_bridges(self.l, c);
        }
        let mut keys = HashMap::new();
        let mut candidates: Vec<((String, String), Vec<SymbolId>)> = Vec::new();
        for c in bases {
            for m in self.l.u.decls(c).unwrap_or_default() {
                if self.is_method_member(m) {
                    let key = (method_name(self.l.u, m), method_descriptor(self.l.u, m));
                    let i = *keys.entry(key.clone()).or_insert_with(|| {
                        candidates.push((key, Vec::new()));
                        candidates.len() - 1
                    });
                    candidates[i].1.push(m);
                }
            }
        }
        let mut out = Vec::new();
        for ((name, desc), members) in candidates {
            let m = self.selected(module, &members);
            if !self.has_forwarder(m) {
                continue;
            }
            let bridge = self.l.is_bridge(m);
            let owner = self.l.u.symbol(m).owner.expect("a member's owner");
            let target = match bridge || self.is_library_made(m) {
                true => self.bridged(owner, m, &desc),
                false => {
                    let mixed = self.l.mixed_in_from(m).or_else(|| self.l.jdk_forwarded_from(m));
                    Some(mixed.unwrap_or(m))
                }
            };
            let Some(target) = target else { continue };
            if !conflicting.contains(&name) {
                let synthetic = bridge || name.contains(name_kinds::TRAIT_SETTER_SEPARATOR);
                out.push(Forwarded { name, desc, synthetic, method: m, target });
            }
        }
        let source_name = |g: &Self, f: &Forwarded| g.l.u.symbol(f.method).name.clone();
        out.sort_by_cached_key(|f| (source_name(self, f), f.desc.clone()));
        out
    }

    /// The names of the term members of erased class `c`: its own and the non-private ones of its
    /// base classes, but those erasure drops (inline and erased members).
    fn conflicting_names(&mut self, c: SymbolId) -> HashSet<String> {
        let primitive = PRIMITIVE_VALUE_CLASSES.contains(&self.l.u.full_name(c).as_str());
        let bases = if primitive { vec![c] } else { self.l.u.linearization(c) };
        let mut names = HashSet::new();
        for b in bases {
            for m in self.l.u.decls(b).unwrap_or_default() {
                let s = self.l.u.symbol(m);
                let inherited_private = b != c && s.flags.has(PRIVATE);
                if s.kind.is_term() && !inherited_private && !self.is_effectively_erased(m) {
                    names.insert(method_name(self.l.u, m));
                }
            }
        }
        names
    }

    /// Whether erasure drops member `m`: an erased member, a deferred inline one, or an inline one
    /// that overrides no member that is not inline (dotty's `isEffectivelyErased`).
    fn is_effectively_erased(&mut self, m: SymbolId) -> bool {
        if lower::is_erased(self.l.u, m) {
            return true;
        }
        let s = self.l.u.symbol(m);
        let Some(owner) = s.owner.filter(|_| s.flags.has(INLINE)) else { return false };
        if self.is_deferred_member(m) {
            return true;
        }
        let s = self.l.u.symbol(m);
        let name = s.name.clone();
        let site = self.l.u.this_type(owner);
        let bases = self.l.u.linearization(owner);
        !bases.into_iter().skip(1).any(|b| {
            self.l.u.decls_named(b, &name).into_iter().any(|o| {
                let other = self.l.u.symbol(o);
                other.kind.is_term() && !other.flags.has(INLINE) && self.l.u.info_matches(site, m, o)
            })
        })
    }

    /// Selects the member of `module` that dotty's member lookup gives among `members`, of one
    /// name and descriptor, in linearization order: of the members whose classes a path of
    /// parents from `module` reaches without passing a class declaring another of them, a
    /// non-bridge before a bridge, then a concrete one before a deferred one, then the first
    /// (dotty's `computeMembersNamed` and the `symScore` of `Denotation.meet`).
    fn selected(&mut self, module: SymbolId, members: &[SymbolId]) -> SymbolId {
        let owner = |g: &Self, m: SymbolId| g.l.u.symbol(m).owner;
        let owners: HashSet<SymbolId> = members.iter().filter_map(|&m| owner(self, m)).collect();
        let (mut reached, mut work) = (HashSet::new(), vec![module]);
        while let Some(c) = work.pop() {
            if !reached.insert(c) || owners.contains(&c) {
                continue;
            }
            for p in self.l.u.parents(c) {
                work.extend(self.l.u.class_symbol(p));
            }
        }
        let mut best: Option<(SymbolId, (bool, bool))> = None;
        for &m in members {
            if !owner(self, m).is_some_and(|o| reached.contains(&o)) {
                continue;
            }
            let rank = (self.l.is_bridge(m), self.is_deferred_member(m));
            if best.is_none_or(|(_, r)| rank < r) {
                best = Some((m, rank));
            }
        }
        best.map_or(members[0], |(m, _)| m)
    }

    /// The method that `m` of class `c` with descriptor `desc`, a bridge or a mixin forwarder of a
    /// library class, overrides, if it is not deferred (dotty's `nextOverriddenSymbol`); the bases
    /// of `c` declare the bridges and mixin forwarders of the library classes compiled in the
    /// same run.
    fn bridged(&mut self, c: SymbolId, m: SymbolId, desc: &str) -> Option<SymbolId> {
        let name = method_name(self.l.u, m);
        for b in self.l.u.linearization(c).into_iter().skip(1) {
            for o in self.l.u.decls(b).unwrap_or_default() {
                if self.is_method_member(o)
                    && method_name(self.l.u, o) == name
                    && method_descriptor(self.l.u, o) == desc
                {
                    return self.has_forwarder(o).then_some(o);
                }
            }
        }
        None
    }

    /// Whether `m` is a bridge or mixin forwarder of a library class compiled in the same run,
    /// made after erasure; a trait gets neither.
    pub fn is_library_made(&self, m: SymbolId) -> bool {
        let s = self.l.u.symbol(m);
        let made_in = |o| self.l.u.is_same_run(o) && !self.l.u.is_trait(o);
        s.tree.is_none() && s.owner.is_some_and(made_in)
    }

    /// Whether `m` is a method or a getter, defined in this unit or in a library.
    fn is_method_member(&self, m: SymbolId) -> bool {
        let s = self.l.u.symbol(m);
        let library = s.tree.is_some();
        s.kind == Kind::Def || s.flags.has(FIELDaccessor) || library && s.kind == Kind::Val
    }

    /// Whether a forwarder calls member method `m`: neither deferred, a constructor, private,
    /// protected, Java package-private, static, a macro, inline, erased, lifted, nor of an
    /// expanded name (a trait setter's name is not one, though the name of its value may be).
    fn has_forwarder(&mut self, m: SymbolId) -> bool {
        let s = self.l.u.symbol(m);
        let excluded = [PRIVATE, PROTECTED, PROTECTEDqualified, STATIC, MACRO, INLINE];
        let expanded = s.name.contains(name_kinds::EXPAND_SEPARATOR)
            && !s.name.contains(name_kinds::TRAIT_SETTER_SEPARATOR);
        let package_private = s.flags.has(PRIVATEqualified) && self.l.u.is_java_defined(m);
        let excluded = excluded.iter().any(|&f| s.flags.has(f)) || expanded || package_private;
        !excluded
            && !self.is_deferred_member(m)
            && !self.l.u.is_constructor(m)
            && !lower::is_erased(self.l.u, m)
            && !self.l.is_lifted(m)
    }

    /// Whether member `m`, of this unit or of a library, has no body.
    fn is_deferred_member(&mut self, m: SymbolId) -> bool {
        match self.l.u.symbol(m).tree.is_some() {
            true => self.l.u.body(m).is_none(),
            false => self.l.u.is_deferred(m),
        }
    }
}
