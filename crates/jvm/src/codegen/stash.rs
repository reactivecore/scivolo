// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSyncAndTry.scala
//! The stack stored in locals around a `try` with handlers, as a handler starts on an empty one.

use super::body::{Body, Operand};
use super::{Gen, unsupported};
use lower::Result;
use syntax::ast::Span;

/// The stashed values: their descriptors and slots, bottom first, and their record.
pub struct Stash {
    locals: Vec<(String, u16)>,
    operands: Vec<(i32, i32, Operand)>,
}

impl Gen<'_, '_> {
    /// Stores the values on the stack into new locals, top first.
    pub fn stash(&mut self, b: &mut Body, at: Span) -> Result<Stash> {
        let Some(operands) = b.known_operands() else {
            return unsupported(at, "a `try` with handlers where values of unknown types are on the stack");
        };
        let mut locals = Vec::new();
        for (slots, o) in operands {
            let desc = match o {
                Operand::Of(t) => self.desc_of(t),
                Operand::Desc(d) => d.to_owned(),
            };
            if classfile::value_slots(&desc) != slots {
                return unsupported(
                    at,
                    "a `try` with handlers where values of unknown types are on the stack",
                );
            }
            let slot = b.temp(&desc);
            locals.push((desc, slot));
        }
        for (desc, slot) in locals.iter().rev() {
            b.code.store(desc, *slot);
        }
        Ok(Stash { locals, operands: std::mem::take(&mut b.operands) })
    }
}

/// Loads the values of `s` back under the `try`'s value of descriptor `desc`, held in `tmp`
/// meanwhile, and clears the references left in locals.
pub fn unstash(b: &mut Body, s: Stash, desc: &str, tmp: Option<u16>) {
    let depth: i32 = s.locals.iter().map(|l| classfile::value_slots(&l.0)).sum();
    let slots = classfile::value_slots(desc);
    if b.code.live() {
        let result = (slots > 0).then(|| tmp.unwrap_or_else(|| b.temp(desc)));
        if let Some(r) = result {
            b.code.store(desc, r);
        }
        for (d, slot) in &s.locals {
            b.code.load(d, *slot);
            clear_ref(b, d, *slot);
        }
        if let Some(r) = result {
            b.code.load(desc, r);
            clear_ref(b, desc, r);
        }
    }
    b.code.set_stack(depth + slots);
    b.operands = s.operands;
}

fn clear_ref(b: &mut Body, desc: &str, slot: u16) {
    if matches!(desc.as_bytes()[0], b'L' | b'[') {
        b.code.aconst_null();
        b.code.store(desc, slot);
    }
}
