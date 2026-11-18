// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/typer/Namer.scala

use crate::symbols::Completion;
use crate::typer::Typer;
use tasty::{Kind, SymbolId, TypeId};

impl Typer<'_> {
    /// Whether `sym` is the value of an object.
    pub fn is_module_val(&self, sym: SymbolId) -> bool {
        let s = self.u.symbol(sym);
        s.kind == Kind::Val && s.flags.has(tasty::tags::OBJECT)
    }

    /// The type of object `sym`'s value, its class through the owner, without resolving the
    /// class's name (dotty's `moduleValSig`), given to `sym` before the class completes; `None`
    /// for another symbol. The class completes now, or after the import path being typed.
    pub fn module_val_info(&mut self, sym: SymbolId) -> Option<TypeId> {
        if !self.is_module_val(sym) {
            return None;
        }
        let owner = self.u.symbol(sym).owner?;
        let class = self.u.module_class(sym)?;
        let prefix = (!self.u.symbol(owner).kind.is_term()).then(|| crate::defn::owner_prefix(self.u, owner));
        let info = self.u.intern_type(tasty::Type::Sym(prefix, class));
        if let Some(classes) = &mut self.import_path_objects {
            classes.push(class);
            return Some(info);
        }
        let completing = self.syms.completions.insert(sym, Completion::Done);
        self.u.set_info(sym, info);
        self.object_classes_completing += 1;
        self.complete(class);
        self.object_classes_completing -= 1;
        if let Some(c) = completing {
            self.syms.completions.insert(sym, c);
        }
        Some(info)
    }
}
