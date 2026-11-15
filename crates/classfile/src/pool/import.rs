//! Entries of one pool added to another in ASM's order (`SymbolTable.addConstant…`): the parts of
//! an entry before it, a call site's bootstrap arguments before its method handle.

use super::{Entry, Pool};

/// The pool a class file is written with, built from the pool its code was generated against.
pub struct Import<'a> {
    from: &'a Pool,
    pub to: Pool,
    /// The index in `to` of each entry of `from` added, 0 for none yet.
    done: Vec<u16>,
}

impl<'a> Import<'a> {
    pub fn new(from: &'a Pool) -> Import<'a> {
        Import { from, to: Pool::default(), done: vec![0; from.entries.len() + 1] }
    }

    /// Adds entry `i` of the source pool and the entries it refers to; its index in `to`.
    pub fn entry(&mut self, i: u16) -> u16 {
        if let Some(&done) = self.done.get(usize::from(i))
            && done != 0
        {
            return done;
        }
        let Some(e) = self.from.entry(i).cloned() else { return 0 };
        let added = match e {
            Entry::Utf8(_) | Entry::Integer(_) | Entry::Float(_) | Entry::Gap => self.to.add(e),
            Entry::Long(_) | Entry::Double(_) => self.to.add_wide(e),
            Entry::Class(n) => self.single(n, Entry::Class),
            Entry::String(s) => self.single(s, Entry::String),
            Entry::MethodType(d) => self.single(d, Entry::MethodType),
            Entry::Fieldref(c, nt) => self.pair(c, nt, Entry::Fieldref),
            Entry::Methodref(c, nt) => self.pair(c, nt, Entry::Methodref),
            Entry::InterfaceMethodref(c, nt) => self.pair(c, nt, Entry::InterfaceMethodref),
            Entry::NameAndType(n, d) => self.pair(n, d, Entry::NameAndType),
            Entry::MethodHandle(kind, member) => {
                let member = self.entry(member);
                self.to.add(Entry::MethodHandle(kind, member))
            }
            Entry::InvokeDynamic(bootstrap, nt) => {
                let bootstrap = self.bootstrap(bootstrap);
                let nt = self.entry(nt);
                self.to.add(Entry::InvokeDynamic(bootstrap, nt))
            }
        };
        if let Some(slot) = self.done.get_mut(usize::from(i)) {
            *slot = added;
        }
        added
    }

    fn single(&mut self, a: u16, entry: fn(u16) -> Entry) -> u16 {
        let a = self.entry(a);
        self.to.add(entry(a))
    }

    fn pair(&mut self, a: u16, b: u16, entry: fn(u16, u16) -> Entry) -> u16 {
        let a = self.entry(a);
        let b = self.entry(b);
        self.to.add(entry(a, b))
    }

    /// Adds bootstrap method `i` of the source pool: its arguments, then its method handle.
    fn bootstrap(&mut self, i: u16) -> u16 {
        let Some((handle, args)) = self.from.bootstraps.get(usize::from(i)).cloned() else { return 0 };
        let args: Vec<u16> = args.into_iter().map(|a| self.entry(a)).collect();
        let method = (self.entry(handle), args);
        let at = match self.to.bootstraps.iter().position(|b| *b == method) {
            Some(at) => at,
            None => {
                self.to.bootstraps.push(method);
                self.to.bootstraps.len() - 1
            }
        };
        u16::try_from(at).unwrap_or(u16::MAX)
    }
}
