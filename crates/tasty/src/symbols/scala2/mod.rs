// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Unpickler.scala
//! The classes of a Scala 2 pickle entered eagerly (ADR 0128): the class, its object's class and
//! its object, with their type parameters, parents and members.

mod annotations;
mod buffer;
mod class_info;
mod existentials;
mod ext;
mod flags;
mod format;
mod methods;
mod refinements;
mod symbols;
mod type_members;
mod types;

use super::{Kind, SymbolId, Universe};
use crate::tags::IMPLICIT;
use buffer::PickleBuffer;
use format::{
    CLASSsym, EXTMODCLASSref, EXTref, MODULEsym, NONEsym, REFINE_CLASS, TERMname, TYPEname, VALsym,
};
use rustc_hash::{FxHashMap, FxHashSet};

/// What a symbol entry of the pickle stands for.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Sym {
    /// A symbol of the universe: entered from this pickle or found by an external reference.
    Known(SymbolId),
    /// A value parameter of a method entered.
    Param,
    /// A type parameter of a method entered, a `Type::Param` while its type is read.
    TypeParam,
    /// A symbol this reader does not enter: private, local or a parameter of a skipped one.
    Skipped,
    /// The absent symbol (`NONEsym`).
    None,
    /// The class of by-name parameter types, `<byname>[T]` for `=> T`.
    ByName,
    /// The class of repeated parameter types, `<repeated>[T]` for `T*`.
    Repeated,
}

/// The reading of one pickle: its entries, the package and name of the class file, the symbol
/// of each symbol entry read, the method type parameters in scope, the symbols entered by entry
/// and as a set, and the members refused.
pub struct Unpickler {
    buf: PickleBuffer,
    index: Vec<usize>,
    pkg: SymbolId,
    simple: String,
    syms: FxHashMap<usize, Sym>,
    params: FxHashMap<usize, crate::TypeId>,
    entered: Vec<(usize, SymbolId)>,
    own: FxHashSet<SymbolId>,
    reading: FxHashSet<SymbolId>,
    /// The value parameters of the member whose type is read, named and whether they have a
    /// default.
    value_params: Vec<(String, bool)>,
    /// The symbol entries of the pickle by owner entry, once a refinement needs them.
    owned: Option<FxHashMap<usize, Vec<usize>>>,
    /// The bounds of the symbols the enclosing existential types bind, by entry.
    existentials: FxHashMap<usize, crate::TypeId>,
    /// The symbols the external references resolve to.
    used: FxHashSet<SymbolId>,
    /// The members entered, then unentered as refused.
    unentered: Vec<SymbolId>,
    refused: Vec<(SymbolId, String, String)>,
}

/// The symbols a pickle entered, those of its members it unentered as refused, and the symbols
/// of other classes it refers to.
#[derive(Default)]
pub struct Footprint {
    pub own: Vec<SymbolId>,
    pub refused: Vec<SymbolId>,
    pub used: Vec<SymbolId>,
}

/// Enters the class `simple` of package `pkg`, its object's class and its object from the text
/// `text` of the class file's pickle annotation (dotty's `Scala2Unpickler.run`): the symbols
/// entered at the package. A member whose type this reader refuses is not entered; a lookup
/// of its name in its class counts as a refused lookup. A pickle of another version than 5.0 to
/// 5.2, a class whose type parameters or parents are refused, and a malformed pickle are refused
/// whole, with nothing entered. `footprint` receives the symbols entered and those of other
/// classes the pickle refers to, also when it is refused.
pub fn enter(
    u: &mut Universe,
    pkg: SymbolId,
    simple: &str,
    text: &[u8],
    footprint: &mut Footprint,
) -> Result<Vec<SymbolId>, String> {
    let mut buf = PickleBuffer { bytes: buffer::decode(text), at: 0 };
    let (major, minor) = (buf.nat()?, buf.nat()?);
    if major != format::MAJOR_VERSION || minor > format::MINOR_VERSION {
        return Err(format!("a pickle of version {major}.{minor}"));
    }
    let index = buf.index()?;
    let mut up = Unpickler {
        buf,
        index,
        pkg,
        simple: simple.to_owned(),
        syms: FxHashMap::default(),
        params: FxHashMap::default(),
        entered: Vec::new(),
        own: FxHashSet::default(),
        reading: FxHashSet::default(),
        value_params: Vec::new(),
        owned: None,
        existentials: FxHashMap::default(),
        used: FxHashSet::default(),
        unentered: Vec::new(),
        refused: Vec::new(),
    };
    let roots = up.run(u);
    footprint.own = up.entered.iter().map(|&(_, s)| s).collect();
    footprint.used = up.used.iter().copied().collect();
    footprint.refused = up.unentered.clone();
    let roots = roots.inspect_err(|_| up.entered.iter().rev().for_each(|&(_, s)| u.unenter(s)))?;
    for (owner, name, why) in up.refused {
        let name = u.intern_name(&name);
        u.refuse_member(owner, name, why);
    }
    Ok(roots)
}

impl Unpickler {
    /// Enters the symbols of the pickle, then reads their types: the roots entered.
    fn run(&mut self, u: &mut Universe) -> Result<Vec<SymbolId>, String> {
        for i in 0..self.index.len() {
            if self.is_root_entry(i)? {
                self.sym(u, i)?;
            }
        }
        for i in 0..self.index.len() {
            if self.is_symbol_entry(i)? {
                self.sym(u, i)?;
            }
        }
        let entered = self.entered.clone();
        let pkg = Some(self.pkg);
        let roots = entered.iter().map(|&(_, s)| s).filter(|&s| u.symbol(s).owner == pkg).collect();
        for &(i, s) in &entered {
            if u.symbol(s).kind == Kind::Class {
                self.class_info(u, i, s)?;
            }
        }
        for &(_, s) in &entered {
            if u.symbol(s).kind == Kind::Type {
                self.ensure_type_info(u, s)?;
            }
        }
        for &(i, s) in &entered {
            if matches!(u.symbol(s).kind, Kind::Type | Kind::Class | Kind::TypeParam) || !self.is_own(s) {
                continue;
            }
            if let Err(why) = self.member_info(u, i, s) {
                self.refuse(u, i, s, why)?;
            }
        }
        self.symbol_annotations(u)?;
        Ok(roots)
    }

    /// Reads the type of type member `s` of this pickle unless it is read or being read.
    fn ensure_type_info(&mut self, u: &mut Universe, s: SymbolId) -> Result<(), String> {
        if u.infos.contains_key(&s) || !self.reading.insert(s) {
            return Ok(());
        }
        let Some(&(i, _)) = self.entered.iter().find(|&&(_, e)| e == s) else { return Ok(()) };
        if let Err(why) = self.type_member_info(u, i, s) {
            self.refuse(u, i, s, why)?;
        }
        Ok(())
    }

    /// Unenters member `s` of entry `i` whose type is refused for `why` and poisons its name;
    /// refuses the pickle if `s` is implicit.
    fn refuse(&mut self, u: &mut Universe, i: usize, s: SymbolId, why: String) -> Result<(), String> {
        if u.symbol(s).flags.has(IMPLICIT) {
            return Err(format!("implicit member `{}`: {why}", u.symbol(s).name));
        }
        let (owner, name) = (u.symbol(s).owner.expect("a member has an owner"), u.symbol(s).name.clone());
        u.unenter(s);
        self.unentered.push(s);
        self.own.remove(&s);
        self.syms.insert(i, Sym::Skipped);
        self.refused.push((owner, name, why));
        Ok(())
    }

    /// The tag of entry `i` and the end of its body, the buffer at the body's start.
    fn seek(&mut self, i: usize) -> Result<(u8, usize), String> {
        self.buf.at = *self.index.get(i).ok_or_else(|| format!("pickle entry {i}"))?;
        let tag = self.buf.byte()?;
        let len = self.buf.nat()?;
        Ok((tag, self.buf.at + len))
    }

    /// The tag of entry `i`.
    fn tag(&self, i: usize) -> Result<u8, String> {
        let at = *self.index.get(i).ok_or_else(|| format!("pickle entry {i}"))?;
        self.buf.bytes.get(at).copied().ok_or_else(|| format!("pickle entry {i}"))
    }

    /// The text of name entry `i` and whether it is a type name.
    fn name(&mut self, i: usize) -> Result<(String, bool), String> {
        let saved = self.buf.at;
        let (tag, end) = self.seek(i)?;
        let bytes = self.buf.bytes.get(self.buf.at..end).ok_or("a truncated Scala 2 pickle")?;
        let text = String::from_utf8_lossy(bytes).into_owned();
        self.buf.at = saved;
        match tag {
            TERMname => Ok((text, false)),
            TYPEname => Ok((text, true)),
            _ => Err(format!("pickle entry {i} of tag {tag} is no name")),
        }
    }

    /// Whether entry `i` is a symbol of the pickle, but a refinement class (dotty's
    /// `isSymbolEntry`).
    fn is_symbol_entry(&mut self, i: usize) -> Result<bool, String> {
        let tag = self.tag(i)?;
        if !(NONEsym..=VALsym).contains(&tag) {
            return Ok(false);
        }
        if tag != CLASSsym {
            return Ok(true);
        }
        self.seek(i)?;
        let name = self.buf.nat()?;
        Ok(self.name(name)?.0 != REFINE_CLASS)
    }

    /// Whether entry `i` is a class or object named as the class file and owned by a package,
    /// which is entered first so that the pickles it leads to find it.
    fn is_root_entry(&mut self, i: usize) -> Result<bool, String> {
        if !matches!(self.tag(i)?, CLASSsym | MODULEsym) {
            return Ok(false);
        }
        let h = self.header(i)?;
        let package_owner = matches!(self.tag(h.owner)?, EXTref | EXTMODCLASSref);
        Ok(package_owner && h.name != REFINE_CLASS && super::decode(&h.name) == self.simple)
    }

    /// Whether entry `i` is a symbol of the pickle or an external one (dotty's `isSymbolRef`).
    fn is_symbol_ref(&self, i: usize) -> bool {
        self.tag(i).is_ok_and(|t| (NONEsym..=EXTMODCLASSref).contains(&t))
    }
}
