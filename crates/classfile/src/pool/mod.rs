//! The constant pool, each entry added once.

mod dynamic;
mod write;

pub use dynamic::{
    Constant, Handle, REF_INVOKE_INTERFACE, REF_INVOKE_SPECIAL, REF_INVOKE_STATIC, REF_INVOKE_VIRTUAL,
};

use rustc_hash::FxHashMap;

#[derive(Clone, PartialEq, Eq, Hash)]
pub enum Entry {
    Utf8(Box<[u16]>),
    Integer(i32),
    Float(u32),
    Long(i64),
    Double(u64),
    /// The unusable slot after a long or double.
    Gap,
    Class(u16),
    String(u16),
    Fieldref(u16, u16),
    Methodref(u16, u16),
    InterfaceMethodref(u16, u16),
    NameAndType(u16, u16),
    /// A method handle: its reference kind and member reference.
    MethodHandle(u8, u16),
    MethodType(u16),
    /// A call site: its bootstrap method's index and its name and type.
    InvokeDynamic(u16, u16),
}

/// The kind of a member reference.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ref {
    Field,
    Method,
    InterfaceMethod,
}

#[derive(Default)]
pub struct Pool {
    entries: Vec<Entry>,
    index: FxHashMap<Entry, u16>,
    /// The bootstrap methods: method handle and arguments, each a pool index.
    bootstraps: Vec<(u16, Vec<u16>)>,
}

impl Pool {
    fn add(&mut self, e: Entry) -> u16 {
        if let Some(&i) = self.index.get(&e) {
            return i;
        }
        self.entries.push(e.clone());
        let i = u16::try_from(self.entries.len()).unwrap_or(u16::MAX);
        self.index.insert(e, i);
        i
    }

    pub fn utf8(&mut self, s: &str) -> u16 {
        self.add(Entry::Utf8(s.encode_utf16().collect()))
    }

    pub fn integer(&mut self, v: i32) -> u16 {
        self.add(Entry::Integer(v))
    }

    pub fn float(&mut self, v: f32) -> u16 {
        self.add(Entry::Float(v.to_bits()))
    }

    /// A long constant, which takes two slots.
    pub fn long(&mut self, v: i64) -> u16 {
        self.add_wide(Entry::Long(v))
    }

    /// A double constant, which takes two slots.
    pub fn double(&mut self, v: f64) -> u16 {
        self.add_wide(Entry::Double(v.to_bits()))
    }

    fn add_wide(&mut self, e: Entry) -> u16 {
        if let Some(&i) = self.index.get(&e) {
            return i;
        }
        let i = self.add(e);
        self.entries.push(Entry::Gap);
        i
    }

    /// The class of internal name `name`, e.g. `java/lang/Object` or `[I`.
    pub fn class(&mut self, name: &str) -> u16 {
        let name = self.utf8(name);
        self.add(Entry::Class(name))
    }

    /// The `String` constant of UTF-16 units `s`, lone surrogates included.
    pub fn string(&mut self, s: &[u16]) -> u16 {
        let s = self.add(Entry::Utf8(s.into()));
        self.add(Entry::String(s))
    }

    pub fn name_and_type(&mut self, name: &str, descriptor: &str) -> u16 {
        let (name, descriptor) = (self.utf8(name), self.utf8(descriptor));
        self.add(Entry::NameAndType(name, descriptor))
    }

    /// The reference to member `name` of class `owner` with `descriptor`.
    pub fn member(&mut self, kind: Ref, owner: &str, name: &str, descriptor: &str) -> u16 {
        let (owner, nt) = (self.class(owner), self.name_and_type(name, descriptor));
        self.add(match kind {
            Ref::Field => Entry::Fieldref(owner, nt),
            Ref::Method => Entry::Methodref(owner, nt),
            Ref::InterfaceMethod => Entry::InterfaceMethodref(owner, nt),
        })
    }

    fn entry(&self, i: u16) -> Option<&Entry> {
        self.entries.get(usize::from(i).checked_sub(1)?)
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The index of `Utf8` entry `s`, if the pool has it.
    pub fn find_utf8(&self, s: &str) -> Option<u16> {
        self.index.get(&Entry::Utf8(s.encode_utf16().collect())).copied()
    }

    pub fn text(&self, i: u16) -> String {
        let Some(Entry::Utf8(s)) = self.entry(i) else { return String::new() };
        String::from_utf16_lossy(s)
    }

    /// The internal name of `Class` entry `i`.
    pub fn class_name(&self, i: u16) -> String {
        let Some(&Entry::Class(n)) = self.entry(i) else { return String::new() };
        self.text(n)
    }

    /// The class, name and descriptor of member reference `i`; of a call site, no class.
    pub fn member_parts(&self, i: u16) -> (String, String, String) {
        let parts = match self.entry(i) {
            Some(&(Entry::Fieldref(c, nt) | Entry::Methodref(c, nt) | Entry::InterfaceMethodref(c, nt))) => {
                self.entry(nt).map(|e| (Some(c), e))
            }
            Some(&Entry::InvokeDynamic(_, nt)) => self.entry(nt).map(|e| (None, e)),
            _ => None,
        };
        let Some((c, &Entry::NameAndType(n, d))) = parts else { return Default::default() };
        (c.map(|c| self.class_name(c)).unwrap_or_default(), self.text(n), self.text(d))
    }

    /// The descriptor of the value `ldc` of entry `i` pushes, `V` for none.
    pub fn constant_descriptor(&self, i: u16) -> &'static str {
        match self.entry(i) {
            Some(Entry::Integer(_)) => "I",
            Some(Entry::Float(_)) => "F",
            Some(Entry::Long(_)) => "J",
            Some(Entry::Double(_)) => "D",
            Some(Entry::String(_)) => "Ljava/lang/String;",
            Some(Entry::Class(_)) => "Ljava/lang/Class;",
            Some(Entry::MethodType(_)) => "Ljava/lang/invoke/MethodType;",
            Some(Entry::MethodHandle(..)) => "Ljava/lang/invoke/MethodHandle;",
            _ => "V",
        }
    }
}
