//! The state of the method being generated: fresh locals, lexical scopes, the values of
//! symbols, the instructions.

use super::Gen;
use super::buf::Buf;
use nir::{LexicalScope, Local, Next, Param, PosId, StrId, ValId};
use rustc_hash::{FxHashMap, FxHashSet};
use tasty::SymbolId;

pub struct Method {
    pub sym: Option<SymbolId>,
    pub this: Option<ValId>,
    fresh: u64,
    fresh_scope: i64,
    pub scope: u32,
    pub scopes: Vec<LexicalScope>,
    pub unwind_handler: Option<Local>,
    pub local_names: Vec<(Local, StrId)>,
    /// The values of parameters and locals; a variable's is its slot.
    pub env: FxHashMap<SymbolId, ValId>,
    /// The local variables, held in slots.
    pub mutable: FxHashSet<SymbolId>,
    pub buf: Buf,
    /// The position of what is being generated.
    pub pos: PosId,
}

impl Method {
    /// A method whose block scopes count from `first_scope`.
    pub fn new(sym: Option<SymbolId>, first_scope: i64, pos: PosId) -> Method {
        Method {
            sym,
            this: None,
            fresh: 0,
            fresh_scope: first_scope,
            scope: 0,
            scopes: Vec::new(),
            unwind_handler: None,
            local_names: Vec::new(),
            env: FxHashMap::default(),
            mutable: FxHashSet::default(),
            buf: Buf::default(),
            pos,
        }
    }
}

impl Gen<'_, '_> {
    pub fn fresh(&mut self) -> Local {
        self.m.fresh += 1;
        Local(self.m.fresh)
    }

    /// A fresh local named `name`.
    pub fn fresh_named(&mut self, name: &str) -> Local {
        let id = self.fresh();
        self.name_local(id, name);
        id
    }

    pub fn name_local(&mut self, id: Local, name: &str) {
        let name = self.str(name);
        self.m.local_names.push((id, name));
    }

    /// The unwind edge of an instruction: to the enclosing handler, if any.
    pub fn unwind(&mut self) -> Next {
        let Some(handler) = self.m.unwind_handler else { return Next::None };
        let ty = self.throwable_type();
        let exc = Param { id: self.fresh(), ty };
        let arg = self.val(nir::Val::Local(exc.id, ty));
        Next::Unwind(exc, Box::new(Next::Label(handler, vec![arg])))
    }

    /// Enters a fresh block scope at `pos`; returns the scope to restore.
    pub fn enter_scope(&mut self, pos: PosId) -> u32 {
        self.m.fresh_scope += 1;
        let id = self.m.fresh_scope as u32;
        let parent = if id == 0 { id } else { self.m.scope };
        let scope = LexicalScope { id, parent, pos };
        if !self.m.scopes.iter().any(|s| s.id == id && s.parent == parent && s.pos == pos) {
            self.m.scopes.push(scope);
        }
        std::mem::replace(&mut self.m.scope, id)
    }

    /// Sets the position of what follows; returns the one to restore.
    pub fn at(&mut self, pos: PosId) -> PosId {
        std::mem::replace(&mut self.m.pos, pos)
    }

    /// Sets the position of what follows to `span`'s if it has one; returns the one to restore.
    pub fn at_span(&mut self, span: syntax::ast::Span) -> PosId {
        let pos = self.pos_or(span, self.m.pos);
        self.at(pos)
    }
}
