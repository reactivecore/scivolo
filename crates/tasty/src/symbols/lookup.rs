//! Looking up declarations, loading the files of a package that define a name.

use super::companions::module_name;
use super::predicates::is_package_object_name;
use super::{Error, Kind, SymbolId, Universe};
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

impl Universe {
    /// The declarations of `owner` named `name`. In a package, the file defining `name` is
    /// loaded, names no file defines are looked up among the package objects' members, and the
    /// names Scala Native gives JDK classes and `Object`'s methods stand for the JDK names; the
    /// `scala` package synthesizes `FunctionN` and `ContextFunctionN` above the library's arities.
    pub fn decl(&mut self, owner: SymbolId, name: &str) -> Result<Vec<SymbolId>, Error> {
        if self.symbol(owner).kind != Kind::Package {
            if name.starts_with("__")
                && self.symbol(owner).name == "Object"
                && let Some(jdk) = super::jdk::jdk_member(&self.full_name(owner), name)
            {
                return self.decl(owner, &jdk);
            }
            return Ok(self.named(owner, name).to_vec());
        }
        if name.starts_with('_')
            && let Some((pkg, jdk)) = super::jdk::jdk_name(&self.full_name(owner), name)
            && let Some(p) = self.package(pkg)
        {
            return self.decl(p, &jdk);
        }
        if self.pending.get(&owner).is_some_and(|files| !files.is_empty()) {
            let encoded = encode(name);
            let module = module_name(&encoded).filter(|_| name.ends_with('$')).map(str::to_owned);
            self.load_pending(owner, |file| file == encoded || Some(file) == module.as_deref())?;
        }
        let found = self.named(owner, name).to_vec();
        if !found.is_empty() {
            return Ok(found);
        }
        if let Some((n, contextual)) = super::function_classes::synthetic_arity(name)
            && self.packages.get("scala") == Some(&owner)
        {
            let object = self.types.intern(crate::Type::Sym(None, self.builtins.object));
            return Ok(vec![super::function_classes::enter_class(self, (owner, object), n, contextual)]);
        }
        let objects = self.package_objects(owner)?;
        Ok(objects.into_iter().flat_map(|o| self.named(o, name).to_vec()).collect())
    }

    /// The classes of the package objects of package `pkg`, loading only their files.
    pub fn package_objects(&mut self, pkg: SymbolId) -> Result<Vec<SymbolId>, Error> {
        self.load_pending(pkg, |file| file == "package" || file.ends_with("$package"))?;
        let decls = &self.symbol(pkg).decls;
        Ok(decls
            .iter()
            .copied()
            .filter(|&d| self.is_class(d) && is_package_object_name(&self.symbol(d).name))
            .collect())
    }

    /// The declarations of `owner` named `name`, through its name index, built on first use.
    fn named(&mut self, owner: SymbolId, name: &str) -> &[SymbolId] {
        let Universe { by_name, symbols, .. } = self;
        let index = by_name.entry(owner).or_insert_with(|| {
            let mut index: FxHashMap<String, Vec<SymbolId>> = FxHashMap::default();
            for &d in &symbols[owner.0 as usize].decls {
                index.entry(symbols[d.0 as usize].name.clone()).or_default().push(d);
            }
            index
        });
        index.get(name).map_or(&[], Vec::as_slice)
    }

    /// All declarations of `owner`, loading every file of a package.
    pub fn decls(&mut self, owner: SymbolId) -> Result<Vec<SymbolId>, Error> {
        self.load_pending(owner, |_| true)?;
        Ok(self.symbol(owner).decls.clone())
    }

    /// Loads the pending files of `pkg` whose names `wanted` accepts, one at a time; a file that
    /// fails stays pending, so each later lookup reports it again.
    fn load_pending(&mut self, pkg: SymbolId, wanted: impl Fn(&str) -> bool) -> Result<(), Error> {
        loop {
            let Some(files) = self.pending.get(&pkg) else { return Ok(()) };
            let Some(i) = files.iter().position(|(stem, _)| wanted(stem)) else { return Ok(()) };
            let path = files[i].1.clone();
            self.load(&path)?;
            if let Some(files) = self.pending.get_mut(&pkg) {
                files.remove(i);
            }
        }
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
