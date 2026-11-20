// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Unpickler.scala
//! The symbol entries of a pickle (dotty's `readDisambiguatedSymbol`, `finishSym`): the roots
//! and the members of the roots entered, nested classes, objects and type members refused.

use super::super::builtin::synthetic;
use super::super::encoded_names::decode;
use super::super::{Kind, SymbolId, Universe, module_class_name};
use super::flags as f;
use super::format::{ALIASsym, CLASSsym, EXTMODCLASSref, EXTref, MODULEsym, NONEsym, TYPEsym, VALsym};
use super::{Sym, Unpickler};
use crate::tags::{
    ABSTRACT, CASE, FIELDaccessor, FINAL, IMPLICIT, LAZY, LOCAL, MUTABLE, OBJECT, OVERRIDE, PRIVATE,
    PRIVATEqualified, PROTECTED, PROTECTEDqualified, SEALED, TRAIT,
};
use names::nme;

/// The header of a symbol entry: tag, name, owner, flags, access qualifier and type (dotty's
/// `SymbolInfo`).
pub struct Header {
    pub tag: u8,
    pub name: String,
    pub owner: usize,
    pub flags: u64,
    pub within: Option<usize>,
    pub info: usize,
}

/// The name of a trait's initializer in pickles (dotty's `nme.TRAIT_CONSTRUCTOR`).
const TRAIT_CONSTRUCTOR: &str = "$init$";

impl Unpickler {
    /// Reads the header of symbol entry `i`.
    pub fn header(&mut self, i: usize) -> Result<Header, String> {
        let saved = self.buf.at;
        let (tag, _) = self.seek(i)?;
        let name = self.buf.nat()?;
        let owner = self.buf.nat()?;
        let flags = self.buf.long_nat()?;
        let r = self.buf.nat()?;
        let (within, info) = if self.is_symbol_ref(r) { (Some(r), self.buf.nat()?) } else { (None, r) };
        let name = self.name(name)?.0;
        self.buf.at = saved;
        Ok(Header { tag, name, owner, flags, within, info })
    }

    /// What symbol entry or external reference `i` stands for, entering it on first use.
    pub fn sym(&mut self, u: &mut Universe, i: usize) -> Result<Sym, String> {
        if let Some(&s) = self.syms.get(&i) {
            return Ok(s);
        }
        let s = match self.tag(i)? {
            NONEsym => Sym::None,
            tag @ (EXTref | EXTMODCLASSref) => Sym::Known(self.external(u, i, tag == EXTMODCLASSref)?),
            _ => {
                let h = self.header(i)?;
                self.local(u, i, &h)?
            }
        };
        Ok(*self.syms.entry(i).or_insert(s))
    }

    /// Enters symbol entry `i` of header `h` if it is a root or a member of a root.
    fn local(&mut self, u: &mut Universe, i: usize, h: &Header) -> Result<Sym, String> {
        let Sym::Known(owner) = self.sym(u, h.owner)? else { return Ok(Sym::Skipped) };
        let param = h.flags & f::PARAM != 0;
        match u.symbol(owner).kind {
            Kind::Package => self.root(u, i, h, owner),
            Kind::Class if self.is_own(owner) => self.member(u, i, h, owner),
            Kind::Def if self.is_own(owner) && param && h.tag == TYPEsym => Ok(Sym::TypeParam),
            Kind::Def if self.is_own(owner) && param && h.tag == VALsym => Ok(Sym::Param),
            _ => Ok(Sym::Skipped),
        }
    }

    /// Whether `s` was entered from this pickle.
    fn is_own(&self, s: SymbolId) -> bool {
        self.entered.iter().any(|&(_, e)| e == s)
    }

    /// Enters the class root, the object's class root or the object root (dotty's `isClassRoot`,
    /// `isModuleClassRoot`, `isModuleRoot`); skips another symbol of the package.
    fn root(&mut self, u: &mut Universe, i: usize, h: &Header, pkg: SymbolId) -> Result<Sym, String> {
        if pkg != self.pkg || decode(&h.name) != self.simple {
            return Ok(Sym::Skipped);
        }
        let module = h.flags & f::MODULE != 0;
        let visible = visibility(h);
        let s = match h.tag {
            CLASSsym if !module => {
                let mut flags = visible;
                flags.extend(class_flags(h.flags));
                let class = synthetic(u, pkg, &self.simple, Kind::Class, &flags);
                self.enter(u, i, h, class)?;
                self.type_params(u, h, class)?;
                return Ok(Sym::Known(class));
            }
            CLASSsym => {
                let flags = [&[OBJECT, FINAL][..], &visible, &case(h.flags)].concat();
                synthetic(u, pkg, &module_class_name(&self.simple), Kind::Class, &flags)
            }
            MODULEsym => {
                synthetic(u, pkg, &self.simple, Kind::Val, &[&[OBJECT, FINAL][..], &visible].concat())
            }
            _ => return Ok(Sym::Skipped),
        };
        self.enter(u, i, h, s)?;
        Ok(Sym::Known(s))
    }

    /// Enters member entry `i` of class `owner`, or refuses its name: a value, variable or
    /// method; a nested class or object, a type member and a macro are refused, private
    /// members, stray type parameters (i12641), case accessors `x$access$N` and extension methods
    /// of an object are skipped.
    fn member(&mut self, u: &mut Universe, i: usize, h: &Header, owner: SymbolId) -> Result<Sym, String> {
        let flags = h.flags;
        let method = flags & f::METHOD != 0;
        let mut name = decode(&h.name);
        if method && name == TRAIT_CONSTRUCTOR {
            name = nme::CONSTRUCTOR.to_owned();
        }
        let private = flags & f::PRIVATE != 0 && name != nme::CONSTRUCTOR;
        let module_class = u.symbol(owner).flags.has(OBJECT);
        let skipped = private
            || h.tag == TYPEsym && flags & f::PARAM != 0
            || flags & f::CASEACCESSOR != 0 && name.contains("$access$")
            || module_class && name.ends_with("$extension");
        if skipped {
            return Ok(Sym::Skipped);
        }
        let refusal = match h.tag {
            CLASSsym if flags & f::MODULE != 0 => Some("a nested object"),
            CLASSsym => Some("a nested class"),
            MODULEsym => Some("a nested object"),
            TYPEsym => Some("an abstract type member"),
            ALIASsym => Some("a type alias member"),
            _ if flags & f::MACRO != 0 => Some("a macro"),
            _ if flags & (f::EXPANDEDNAME | f::SUPERACCESSOR) != 0 => Some("an expanded name"),
            _ if name.contains("$default$") => Some("a default argument"),
            _ if !method => Some("a field without accessor"),
            _ => None,
        };
        if let Some(why) = refusal {
            self.refused.push((owner, name, why.to_owned()));
            return Ok(Sym::Skipped);
        }
        let getter = flags & f::ACCESSOR != 0 && !name.ends_with("_=");
        let mut tags = visibility(h);
        tags.extend(member_flags(flags));
        if getter && flags & f::STABLE == 0 {
            tags.push(MUTABLE);
        }
        if flags & f::ACCESSOR != 0 {
            tags.push(FIELDaccessor);
        }
        let kind = if getter { Kind::Val } else { Kind::Def };
        let s = synthetic(u, owner, &name, kind, &tags);
        u.set_deferred(s, flags & f::DEFERRED != 0);
        self.enter(u, i, h, s)?;
        Ok(Sym::Known(s))
    }

    /// Records `s` as entered from entry `i` and gives it the access qualifier of `h`.
    fn enter(&mut self, u: &mut Universe, i: usize, h: &Header, s: SymbolId) -> Result<(), String> {
        self.entered.push((i, s));
        self.syms.insert(i, Sym::Known(s));
        if let Some(within) = h.within {
            let Sym::Known(q) = self.sym(u, within)? else { return Ok(()) };
            u.set_access_qualifier(s, Some(q));
        }
        Ok(())
    }
}

/// The visibility tags of header `h`; a qualifier is the access qualifier `enter` gives.
fn visibility(h: &Header) -> Vec<u8> {
    let (flags, qualified) = (h.flags, h.within.is_some());
    let local = if flags & f::LOCAL != 0 { Some(LOCAL) } else { None };
    match flags {
        _ if flags & f::PRIVATE != 0 => [PRIVATE].into_iter().chain(local).collect(),
        _ if flags & f::PROTECTED != 0 && qualified => vec![PROTECTED, PROTECTEDqualified],
        _ if flags & f::PROTECTED != 0 => [PROTECTED].into_iter().chain(local).collect(),
        _ if qualified => vec![PRIVATEqualified],
        _ => Vec::new(),
    }
}

/// The tags of a class's pickled flags `flags`.
fn class_flags(flags: u64) -> Vec<u8> {
    let mut tags = case(flags);
    let pairs = [(f::FINAL, FINAL), (f::SEALED, SEALED), (f::ABSTRACT, ABSTRACT), (f::TRAIT, TRAIT)];
    tags.extend(pairs.iter().filter(|&&(b, _)| flags & b != 0).map(|&(_, t)| t));
    if flags & f::TRAIT != 0 && flags & f::ABSTRACT == 0 {
        tags.push(ABSTRACT);
    }
    tags
}

/// The `case` tag if pickled flags `flags` have it.
fn case(flags: u64) -> Vec<u8> {
    if flags & f::CASE != 0 { vec![CASE] } else { Vec::new() }
}

/// The tags of a member's pickled flags `flags`.
fn member_flags(flags: u64) -> Vec<u8> {
    let pairs = [(f::FINAL, FINAL), (f::IMPLICIT, IMPLICIT), (f::OVERRIDE, OVERRIDE), (f::LAZY, LAZY)];
    pairs.iter().filter(|&&(b, _)| flags & b != 0).map(|&(_, t)| t).collect()
}
