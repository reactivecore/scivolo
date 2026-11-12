// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/SymDenotations.scala

use super::compactify::compactify;
use crate::symbols::{defined_name, encode, module_class_name, module_name};
use crate::tags::{OBJECT as MODULE, SYNTHETIC};
use crate::{Kind, SymbolId, Universe};

impl Universe {
    /// The binary name of class `c`: `pkg.Name`, `Outer$Inner`, `Object$Inner`; target names encoded,
    /// a JDK class Scala Native renames under its defined name when outer; a nested name
    /// `compactify`d.
    pub fn binary_name(&self, c: SymbolId) -> String {
        self.class_name_with(c, compactify)
    }

    /// `binary_name` without `compactify`, the prefix of an expanded name (`C$$$outer`).
    pub fn expanded_binary_name(&self, c: SymbolId) -> String {
        self.class_name_with(c, str::to_owned)
    }

    fn class_name_with(&self, c: SymbolId, compactify: fn(&str) -> String) -> String {
        let s = self.symbol(c);
        let name = encode(self.class_target(c).as_deref().unwrap_or(&s.name));
        match s.owner {
            Some(o) if self.symbol(o).kind == Kind::Class => {
                let mut outer = self.class_name_with(o, compactify);
                let module = module_name(&outer);
                match defined_name(module.unwrap_or(&outer)) {
                    Some(d) if module.is_some() => outer = module_class_name(d),
                    Some(d) => outer = d.to_owned(),
                    None => {}
                }
                let flat_start = outer.rfind('.').map_or(0, |i| i + 1);
                let module = s.flags.has(MODULE).then(|| module_name(&name)).flatten();
                let simple = module.unwrap_or(&name);
                let mut flat = String::with_capacity(outer.len() - flat_start + 1 + simple.len());
                flat.push_str(&outer[flat_start..]);
                if !flat.ends_with('$') {
                    flat.push('$');
                }
                flat.push_str(simple);
                outer.truncate(if flat_start > 1 { flat_start } else { 0 });
                outer.push_str(&compactify(&flat));
                if module.is_some() {
                    outer.push('$');
                }
                outer
            }
            Some(o) if o != self.root() => {
                let mut full = self.full_name(o);
                full.push('.');
                full.push_str(&name);
                full
            }
            _ => name,
        }
    }

    /// The `@targetName` of class `c`; a module class's is its object's, or its companion class's
    /// when synthetic, followed by `$` (dotty's `targetName`).
    pub fn class_target(&self, c: SymbolId) -> Option<String> {
        let s = self.symbol(c);
        if !s.flags.has(MODULE) {
            return s.target.clone();
        }
        let name = module_name(&s.name)?;
        let synthetic = s.flags.has(SYNTHETIC);
        let decls = &self.symbol(s.owner?).decls;
        let carrier = decls.iter().map(|&d| self.symbol(d)).find(|d| {
            d.name == name && d.kind == if synthetic { Kind::Class } else { Kind::Val } && d.target.is_some()
        });
        carrier.and_then(|d| d.target.as_deref()).map(module_class_name)
    }
}
