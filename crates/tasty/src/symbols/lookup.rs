//! Looking up declarations by name.

use super::{Error, Kind, SymbolId, Universe};
use crate::types::NameId;
use rustc_hash::FxHashMap;

/// A name a lookup takes: its id, or its text, interned on the way.
pub trait LookupName {
    fn id(self, u: &mut Universe) -> NameId;
}

impl LookupName for NameId {
    fn id(self, _: &mut Universe) -> NameId {
        self
    }
}

impl LookupName for &str {
    fn id(self, u: &mut Universe) -> NameId {
        u.intern_name(self)
    }
}

impl LookupName for &String {
    fn id(self, u: &mut Universe) -> NameId {
        u.intern_name(self)
    }
}

impl Universe {
    /// The declarations of `owner` named `name`. In a package, the file defining `name` is
    /// loaded, names no file defines are looked up among the package objects' members, and the
    /// names Scala Native gives JDK classes and `Object`'s methods stand for the JDK names unless a
    /// source defines them; the `scala` package synthesizes `FunctionN` and `ContextFunctionN`
    /// above the library's arities.
    pub fn decl(&mut self, owner: SymbolId, name: impl LookupName) -> Result<Vec<SymbolId>, Error> {
        let name = name.id(self);
        self.decl_id(owner, name)
    }

    fn decl_id(&mut self, owner: SymbolId, name: NameId) -> Result<Vec<SymbolId>, Error> {
        if self.symbol(owner).kind != Kind::Package {
            if self.name(name).starts_with("__")
                && self.symbol(owner).name == "Object"
                && let Some(jdk) = super::jdk::jdk_member(&self.full_name(owner), self.name(name))
            {
                return self.decl(owner, &jdk);
            }
            let constructor = self.name(name) == names::nme::CONSTRUCTOR;
            if self.jdk.is_some() && !(constructor && self.enter_jdk_annotation(owner)) {
                self.jdk_view(owner, name);
            }
            if self.is_pickled(owner) {
                self.look_up_refused_member(owner, name);
            }
            return Ok(self.named(owner, name).to_vec());
        }
        if self.name(name).starts_with('_')
            && self.named(owner, name).is_empty()
            && let Some((pkg, jdk)) = super::jdk::jdk_name(&self.full_name(owner), self.name(name))
            && let Some(p) = self.package(pkg)
        {
            return self.decl(p, &jdk);
        }
        self.load_defining(owner, name)?;
        let found = self.named(owner, name).to_vec();
        if !found.is_empty() {
            return Ok(found);
        }
        if let Some((n, contextual)) = super::function_classes::synthetic_arity(self.name(name))
            && self.packages.get("scala") == Some(&owner)
        {
            let object = self.types.intern(crate::Type::Sym(None, self.defn.object));
            return Ok(vec![super::function_classes::enter_class(self, (owner, object), n, contextual)]);
        }
        let objects = self.package_objects(owner)?;
        let found: Vec<SymbolId> = objects.into_iter().flat_map(|o| self.named(o, name).to_vec()).collect();
        if !found.is_empty() {
            return Ok(found);
        }
        Ok(self.class_file_decl(owner, name))
    }

    /// The declarations named `name` that package `pkg` gets from the JDK or a jar's class files.
    /// A name without any is remembered until the package loses a declaration, unless looking
    /// it up counted a refused class.
    fn class_file_decl(&mut self, pkg: SymbolId, name: NameId) -> Vec<SymbolId> {
        if self.class_file_misses.contains(&(pkg, name)) {
            return Vec::new();
        }
        let refused = self.refused_lookups();
        let jdk = self.jdk_decl(pkg, name);
        let found = if jdk.is_empty() { self.classpath_decl(pkg, name) } else { jdk };
        if found.is_empty() && self.refused_lookups() == refused {
            self.class_file_misses.insert((pkg, name));
        }
        found
    }

    /// Makes the declarations of class `owner` named `name` those of its JDK class file (ADR
    /// 0118): hides the library's that the JDK lacks, then enters the JDK's where none is left,
    /// members or else a nested class. Nothing to do for a class without a JDK class file.
    fn jdk_view(&mut self, owner: SymbolId, name: NameId) {
        if self.jdk_class_of(owner).is_none() {
            return;
        }
        let found = self.named(owner, name).to_vec();
        let lacking = self.jdk_lacking(owner, &found);
        lacking.iter().for_each(|&m| self.unenter(m));
        if !self.named(owner, name).is_empty() {
            return;
        }
        if lacking.is_empty() {
            self.enter_jdk_members(owner);
            if self.named(owner, name).is_empty() {
                let text = self.name(name).to_owned();
                self.enter_jdk_nested(owner, &text);
            }
        } else {
            let text = self.name(name).to_owned();
            self.enter_jdk_named(owner, &text);
        }
    }

    /// The declarations `decl` finds, none where a file fails to load.
    pub fn decls_named(&mut self, owner: SymbolId, name: impl LookupName) -> Vec<SymbolId> {
        self.decl(owner, name).unwrap_or_default()
    }

    /// The declarations of `owner` named `name`, through its name index, built on first use.
    fn named(&mut self, owner: SymbolId, name: NameId) -> &[SymbolId] {
        let Universe { by_name, symbols, types, .. } = self;
        let index = by_name.entry(owner).or_insert_with(|| {
            let mut index: FxHashMap<NameId, Vec<SymbolId>> = FxHashMap::default();
            for &d in &symbols[owner.0 as usize].decls {
                index.entry(types.intern_name(&symbols[d.0 as usize].name)).or_default().push(d);
            }
            index
        });
        index.get(&name).map_or(&[], Vec::as_slice)
    }
}
