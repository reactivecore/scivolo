// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Unpickler.scala
//! The external references of a pickle (dotty's `readExtSymbol`): symbols of the universe found
//! by owner and name.

use super::super::encoded_names::decode;
use super::super::{Kind, SymbolId, Universe, module_class_name};
use super::{Sym, Unpickler};

/// The names of the root package in pickles (dotty's `nme.ROOT`, `nme.ROOTPKG`).
const ROOTS: [&str; 2] = ["<root>", "_root_"];

/// The classes Scala 2 types by-name and repeated parameters with, and the refusal of each.
const PARAM_CLASSES: [(&str, &str); 2] =
    [("<byname>", "a by-name parameter"), ("<repeated>", "a repeated parameter")];

impl Unpickler {
    /// Finds the symbol external reference `i` names: its owner's declaration of its name, of a
    /// type for a type name, else of a term, a package for either; for an object's class
    /// (`EXTMODCLASSref`), the class of the object or the package. An owner of `Object` that
    /// lacks the name stands for `Any` (dotty's `fromName`). Refuses a reference this reader does
    /// not resolve: the parameter classes, an overloaded or missing name.
    pub fn external(&mut self, u: &mut Universe, i: usize, module_class: bool) -> Result<SymbolId, String> {
        let saved = self.buf.at;
        let (_, end) = self.seek(i)?;
        let name = self.buf.nat()?;
        let owner = if self.buf.at == end { None } else { Some(self.buf.nat()?) };
        self.buf.at = saved;
        let (name, is_type) = self.name(name)?;
        let name = decode(&name);
        let owner = match owner {
            None => u.root(),
            Some(o) => match self.sym(u, o)? {
                Sym::Known(s) => s,
                _ => return Err(format!("a reference to `{name}` in a symbol not read")),
            },
        };
        if ROOTS.contains(&name.as_str()) {
            return Ok(u.root());
        }
        if owner == u.defn().scala
            && let Some(&(_, why)) = PARAM_CLASSES.iter().find(|(n, _)| *n == name)
        {
            return Err(why.to_owned());
        }
        let found = self.named(u, owner, &name, is_type, module_class);
        let found = match found.is_empty() && owner == u.defn().object {
            true => self.named(u, u.defn().any, &name, is_type, module_class),
            false => found,
        };
        match found[..] {
            [s] => Ok(s),
            [] => Err(format!("`{}.{name}`, which the classpath lacks", u.full_name(owner))),
            _ => Err(format!("an overloaded reference to `{}.{name}`", u.full_name(owner))),
        }
    }

    /// The declarations of `owner` named `name` that a reference of a type or term name (or an
    /// object's class) may mean.
    fn named(
        &self,
        u: &mut Universe,
        owner: SymbolId,
        name: &str,
        is_type: bool,
        module_class: bool,
    ) -> Vec<SymbolId> {
        let packages: Vec<SymbolId> =
            u.decls_named(owner, name).into_iter().filter(|&s| u.symbol(s).kind == Kind::Package).collect();
        if !packages.is_empty() {
            return packages;
        }
        if module_class {
            let found = u.decls_named(owner, &module_class_name(name));
            return found.into_iter().filter(|&s| u.symbol(s).kind == Kind::Class).collect();
        }
        let found = u.decls_named(owner, name);
        found.into_iter().filter(|&s| u.symbol(s).kind.is_type() == is_type).collect()
    }
}
