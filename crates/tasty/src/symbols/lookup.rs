//! Looking up declarations by name.

use super::{Error, Kind, SymbolId, Universe};
use crate::types::NameId;
use rustc_hash::FxHashMap;

/// The operator characters of `NameTransformer` and their encodings in file names.
const OPERATORS: [(char, &str); 18] = [
    ('~', "$tilde"),
    ('=', "$eq"),
    ('<', "$less"),
    ('>', "$greater"),
    ('!', "$bang"),
    ('#', "$hash"),
    ('%', "$percent"),
    ('^', "$up"),
    ('&', "$amp"),
    ('|', "$bar"),
    ('*', "$times"),
    ('/', "$div"),
    ('+', "$plus"),
    ('-', "$minus"),
    (':', "$colon"),
    ('\\', "$bslash"),
    ('?', "$qmark"),
    ('@', "$at"),
];

/// `name` as the compiler spells it in file names: operators by their names, other characters
/// that are not Java identifier parts as `$uXXXX`.
pub fn encode(name: &str) -> String {
    if name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'$') {
        return name.to_owned();
    }
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match OPERATORS.iter().find(|(op, _)| *op == c) {
            Some((_, code)) => out.push_str(code),
            None if c.is_alphanumeric() || c == '_' || c == '$' => out.push(c),
            None => c.encode_utf16(&mut [0; 2]).iter().for_each(|u| out.push_str(&format!("$u{u:04X}"))),
        }
    }
    out
}

/// The name `encode` spells as `encoded`.
pub fn decode(encoded: &str) -> String {
    let mut out = String::with_capacity(encoded.len());
    let mut rest = encoded;
    while let Some(c) = rest.chars().next() {
        if let Some((op, code)) = OPERATORS.iter().find(|(_, code)| rest.starts_with(code)) {
            out.push(*op);
            rest = &rest[code.len()..];
            continue;
        }
        let unicode =
            rest.strip_prefix("$u").and_then(|r| r.get(..4)).and_then(|h| u32::from_str_radix(h, 16).ok());
        if let Some(c) = unicode.and_then(char::from_u32) {
            out.push(c);
            rest = &rest[6..];
            continue;
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

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
            if self.jdk.is_some() {
                self.jdk_view(owner, name);
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
        let jdk = self.jdk_decl(owner, name);
        Ok(if jdk.is_empty() { self.classpath_decl(owner, name) } else { jdk })
    }

    /// Makes the declarations of class `owner` named `name` those of its JDK class file (ADR
    /// 0118): hides the library's that the JDK lacks, then enters the JDK's where none is left.
    fn jdk_view(&mut self, owner: SymbolId, name: NameId) {
        let found = self.named(owner, name).to_vec();
        let lacking = self.jdk_lacking(owner, &found);
        lacking.iter().for_each(|&m| self.unenter(m));
        if !self.named(owner, name).is_empty() {
            return;
        }
        if lacking.is_empty() {
            self.enter_jdk_members(owner);
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

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn encodes_like_name_transformer() {
        assert_eq!(encode("::"), "$colon$colon");
        assert_eq!(encode("=:="), "$eq$colon$eq");
        assert_eq!(encode("List"), "List");
        assert_eq!(encode("Nothing$"), "Nothing$");
        assert_eq!(encode("a\u{2192}b"), "a$u2192b");
    }

    #[test]
    fn decodes_what_encode_spells() {
        for name in ["::", "=:=", "<:<", "List", "Nothing$", "a\u{2192}b", "^up"] {
            assert_eq!(decode(&encode(name)), name);
        }
    }
}
