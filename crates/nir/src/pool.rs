//! Interning arenas shared by every file read into one `Pool`.

use crate::nir::{Global, GlobalId, Pos, PosId, StrId, Type, TypeId, Val, ValId};
use crate::sig::ident as push_ident;
use rustc_hash::FxHashMap;
use std::hash::Hash;

pub struct Interner<T> {
    items: Vec<T>,
    ids: FxHashMap<T, u32>,
}

impl<T: Hash + Eq + Clone> Interner<T> {
    fn new() -> Self {
        Self { items: Vec::new(), ids: FxHashMap::default() }
    }

    pub fn intern(&mut self, item: T) -> u32 {
        if let Some(&id) = self.ids.get(&item) {
            return id;
        }
        let id = self.items.len() as u32;
        self.items.push(item.clone());
        self.ids.insert(item, id);
        id
    }

    pub fn get(&self, id: u32) -> &T {
        &self.items[id as usize]
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl Interner<Box<str>> {
    /// The id of `s`, allocating only when it is new.
    pub fn intern_str(&mut self, s: &str) -> u32 {
        match self.ids.get(s) {
            Some(&id) => id,
            None => self.intern(s.into()),
        }
    }
}

pub struct Pool {
    pub strings: Interner<Box<str>>,
    pub globals: Interner<Global>,
    pub types: Interner<Type>,
    pub vals: Interner<Val>,
    pub positions: Interner<Pos>,
    /// The exact UTF-16 units of strings that are not valid Unicode (lone surrogates); `strings`
    /// holds those as their lossy text followed by a NUL and the units, one entry each.
    pub utf16: FxHashMap<StrId, Box<[u16]>>,
}

impl Default for Pool {
    fn default() -> Self {
        Self {
            strings: Interner::new(),
            globals: Interner::new(),
            types: Interner::new(),
            vals: Interner::new(),
            positions: Interner::new(),
            utf16: FxHashMap::default(),
        }
    }
}

impl Pool {
    pub fn str(&self, id: StrId) -> &str {
        self.strings.get(id.0)
    }

    pub fn global(&self, id: GlobalId) -> &Global {
        self.globals.get(id.0)
    }

    pub fn ty(&self, id: TypeId) -> &Type {
        self.types.get(id.0)
    }

    pub fn val(&self, id: ValId) -> &Val {
        self.vals.get(id.0)
    }

    pub fn pos(&self, id: PosId) -> &Pos {
        self.positions.get(id.0)
    }

    /// Whether function type `id` takes C varargs.
    pub fn is_variadic(&self, id: TypeId) -> bool {
        match self.ty(id) {
            Type::Function(args, _) => args.iter().any(|t| matches!(self.ty(*t), Type::Vararg)),
            _ => false,
        }
    }

    pub fn top_name(&self, id: GlobalId) -> &str {
        match self.global(id) {
            Global::Top(s) => self.str(*s),
            Global::Member(owner, _) => self.top_name(*owner),
            Global::None => "",
        }
    }

    pub fn member_sig(&self, id: GlobalId) -> Option<&str> {
        match self.global(id) {
            Global::Member(_, sig) => Some(self.str(*sig)),
            _ => None,
        }
    }

    pub fn mangle(&self, id: GlobalId) -> String {
        let mut out = String::new();
        match self.global(id) {
            Global::None => {}
            Global::Top(s) => {
                out.push('T');
                push_ident(&mut out, self.str(*s));
            }
            Global::Member(owner, sig) => {
                out.push('M');
                push_ident(&mut out, self.top_name(*owner));
                out.push_str(self.str(*sig));
            }
        }
        out
    }
}
