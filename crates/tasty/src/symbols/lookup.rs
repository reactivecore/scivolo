//! Looking up declarations, loading the files of a package that define a name.

use super::{Error, Kind, SymbolId, Universe};

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

fn is_package_object(name: &str) -> bool {
    name == "package$" || name.ends_with("$package$")
}

impl Universe {
    /// The declarations of `owner` named `name`. In a package, the file defining `name` is
    /// loaded, and names no file defines are looked up among the package objects' members.
    pub fn decl(&mut self, owner: SymbolId, name: &str) -> Result<Vec<SymbolId>, Error> {
        let named = |u: &Self, owner: SymbolId| -> Vec<SymbolId> {
            u.symbol(owner).decls.iter().copied().filter(|&d| u.symbol(d).name == name).collect()
        };
        if self.symbol(owner).kind != Kind::Package {
            return Ok(named(self, owner));
        }
        let encoded = encode(name);
        let module = encoded.strip_suffix('$').filter(|_| name.ends_with('$')).map(str::to_owned);
        self.load_pending(owner, |file| file == encoded || Some(file) == module.as_deref())?;
        let found = named(self, owner);
        if !found.is_empty() {
            return Ok(found);
        }
        self.load_pending(owner, |file| file == "package" || file.ends_with("$package"))?;
        let objects: Vec<SymbolId> = self
            .symbol(owner)
            .decls
            .iter()
            .copied()
            .filter(|&d| self.symbol(d).kind == Kind::Class && is_package_object(&self.symbol(d).name))
            .collect();
        Ok(objects.into_iter().flat_map(|o| named(self, o)).collect())
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
    use super::encode;

    #[test]
    fn encodes_like_name_transformer() {
        assert_eq!(encode("::"), "$colon$colon");
        assert_eq!(encode("=:="), "$eq$colon$eq");
        assert_eq!(encode("List"), "List");
        assert_eq!(encode("Nothing$"), "Nothing$");
        assert_eq!(encode("a\u{2192}b"), "a$u2192b");
    }
}
