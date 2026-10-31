// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/scala/quoted/runtime/impl/printers/SourceCode.scala
//! The names local symbols print as (dotty's `splicedName`).

use super::Code;
use tasty::{Kind, SymbolId};

impl Code<'_> {
    /// Whether `s` is owned by a term (dotty's `splicedName` applies).
    pub fn is_local(&self, s: SymbolId) -> bool {
        self.h
            .u
            .symbol(s)
            .owner
            .is_some_and(|o| !matches!(self.h.u.symbol(o).kind, Kind::Class | Kind::Package))
    }

    /// The name local `s` prints as: its own, subscripted from its second symbol on.
    pub fn spliced_name(&mut self, s: SymbolId) -> String {
        if let Some(n) = self.names.get(&s) {
            return n.clone();
        }
        let base = self.h.u.symbol(s).name.clone();
        let index = self.index.entry(base.clone()).or_insert(1);
        let name = match *index {
            1 => base.clone(),
            i => {
                let digits: String = i
                    .to_string()
                    .chars()
                    .map(|d| char::from_u32(d as u32 - '0' as u32 + 0x2080).unwrap_or(d))
                    .collect();
                format!("`{base}{digits}`")
            }
        };
        *index += 1;
        self.names.insert(s, name.clone());
        name
    }
}
