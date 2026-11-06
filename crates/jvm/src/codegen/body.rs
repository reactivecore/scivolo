// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/backend/jvm/BCodeSkelBuilder.scala
//! The state of the method being generated: its code, locals, labels and pending cleanups.

use classfile::{Code, Label, Pool};
use tasty::SymbolId;

/// Where the value of a tree goes (dotty's `LoadDestination`): on the stack, to a label, or
/// returned from the method.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dest {
    Fall,
    Jump(Label),
    Return,
}

/// The code of the method being generated and the slots of its parameters and locals.
pub struct Body<'p> {
    pub code: Code,
    pub pool: &'p mut Pool,
    /// The class whose `this` is slot 0, none in a static method.
    pub this_class: Option<String>,
    /// The object whose `MODULE$` stands for `this` in its `<clinit>`.
    pub module: Option<String>,
    /// The parameters and locals: symbol, slot and descriptor.
    pub locals: Vec<(SymbolId, u16, String)>,
    /// The enclosing labeled blocks: label, destination and the slots of the value.
    pub labels: Vec<(SymbolId, Dest, i32)>,
    /// The method and the descriptor of its result.
    pub method: Option<SymbolId>,
    pub result: String,
    /// The cleanups of the enclosing `finally` and `synchronized` blocks, innermost last.
    pub cleanups: Vec<Label>,
    /// The slot of the value a `return` within a cleanup's range leaves.
    pub early_return: Option<u16>,
    /// Whether a `return` jumped to a cleanup not yet generated.
    pub emit_cleanup: bool,
}

impl<'p> Body<'p> {
    /// A static method's body if `module` is given (an object's `<clinit>`), else that of a method
    /// whose parameters start at slot `first`.
    pub fn new(pool: &'p mut Pool, module: Option<&str>, first: u16) -> Body<'p> {
        Body {
            code: Code::new(first),
            pool,
            this_class: None,
            module: module.map(str::to_owned),
            locals: Vec::new(),
            labels: Vec::new(),
            method: None,
            result: "V".into(),
            cleanups: Vec::new(),
            early_return: None,
            emit_cleanup: false,
        }
    }

    /// A slot for local `s` of descriptor `desc`.
    pub fn bind(&mut self, s: SymbolId, desc: String) -> u16 {
        let slot = self.temp(&desc);
        self.locals.push((s, slot, desc));
        slot
    }

    /// The slot of local `s`, a new one of descriptor `desc` the first time (dotty's
    /// `getOrMakeLocal`).
    pub fn local(&mut self, s: SymbolId, desc: String) -> u16 {
        match self.locals.iter().find(|l| l.0 == s) {
            Some(&(_, slot, _)) => slot,
            None => self.bind(s, desc),
        }
    }

    /// A new slot for a value of descriptor `desc` without a symbol.
    pub fn temp(&mut self, desc: &str) -> u16 {
        let slot = self.code.max_locals;
        self.code.max_locals += classfile::value_slots(desc) as u16;
        slot
    }
}
