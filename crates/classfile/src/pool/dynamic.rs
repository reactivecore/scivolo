//! Method handles, method types and call sites with their bootstrap methods (JVMS §4.4.8–10,
//! §4.7.23).

use super::{Entry, Pool, Ref};

pub const REF_INVOKE_VIRTUAL: u8 = 5;
pub const REF_INVOKE_STATIC: u8 = 6;
pub const REF_INVOKE_SPECIAL: u8 = 7;
pub const REF_INVOKE_INTERFACE: u8 = 9;

/// A method handle: its reference kind (`REF_invoke…`), the method's class, name and descriptor,
/// and whether the class is an interface.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Handle {
    pub kind: u8,
    pub owner: String,
    pub name: String,
    pub descriptor: String,
    pub interface: bool,
}

impl Handle {
    /// The handle of static method `name` of class `owner`.
    pub fn static_method(owner: &str, name: &str, descriptor: &str) -> Handle {
        let (owner, name, descriptor) = (owner.into(), name.into(), descriptor.into());
        Handle { kind: REF_INVOKE_STATIC, owner, name, descriptor, interface: false }
    }
}

/// A static argument of a bootstrap method.
pub enum Constant<'a> {
    Int(i32),
    /// UTF-16 units, lone surrogates included.
    String(&'a [u16]),
    MethodType(&'a str),
    Handle(&'a Handle),
}

const ATTRIBUTE: &str = "BootstrapMethods";

impl Pool {
    pub fn handle(&mut self, h: &Handle) -> u16 {
        let r = if h.interface { Ref::InterfaceMethod } else { Ref::Method };
        let member = self.member(r, &h.owner, &h.name, &h.descriptor);
        self.add(Entry::MethodHandle(h.kind, member))
    }

    pub fn method_type(&mut self, descriptor: &str) -> u16 {
        let d = self.utf8(descriptor);
        self.add(Entry::MethodType(d))
    }

    fn constant(&mut self, c: &Constant) -> u16 {
        match *c {
            Constant::Int(v) => self.integer(v),
            Constant::String(s) => self.string(s),
            Constant::MethodType(d) => self.method_type(d),
            Constant::Handle(h) => self.handle(h),
        }
    }

    /// The call site `name` of type `descriptor` that `bootstrap` links with arguments `args`.
    pub fn invoke_dynamic(
        &mut self,
        name: &str,
        descriptor: &str,
        bootstrap: &Handle,
        args: &[Constant],
    ) -> u16 {
        let method = (self.handle(bootstrap), args.iter().map(|a| self.constant(a)).collect());
        let i = match self.bootstraps.iter().position(|b| *b == method) {
            Some(i) => i,
            None => {
                self.utf8(ATTRIBUTE);
                self.bootstraps.push(method);
                self.bootstraps.len() - 1
            }
        };
        let nt = self.name_and_type(name, descriptor);
        self.add(Entry::InvokeDynamic(u16::try_from(i).unwrap_or(u16::MAX), nt))
    }

    /// The `BootstrapMethods` attribute's name and bytes, none without call sites.
    pub fn bootstrap_methods(&self) -> Option<(u16, Vec<u8>)> {
        let name = *self.index.get(&Entry::Utf8(ATTRIBUTE.encode_utf16().collect()))?;
        let mut bytes = Vec::new();
        bytes.extend((self.bootstraps.len() as u16).to_be_bytes());
        for (handle, args) in &self.bootstraps {
            bytes.extend(handle.to_be_bytes());
            bytes.extend((args.len() as u16).to_be_bytes());
            bytes.extend(args.iter().flat_map(|a| a.to_be_bytes()));
        }
        Some((name, bytes))
    }
}
