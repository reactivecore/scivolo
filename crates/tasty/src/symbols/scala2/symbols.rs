// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Unpickler.scala
//! The symbol entries of a pickle (dotty's `readDisambiguatedSymbol`, `finishSym`): the roots
//! and the members of classes entered, those this reader does not read refused.

use super::super::builtin::synthetic;
use super::super::encoded_names::decode;
use super::super::{Kind, SymbolId, Universe};
use super::classes::is_case_companion;
use super::flags as f;
use super::format::{
    ALIASsym, CLASSsym, EXTMODCLASSref, EXTref, MODULEsym, NONEsym, REFINE_CLASS, TYPEsym, VALsym,
};
use super::{Sym, Unpickler};
use crate::tags::OBJECT;
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

/// The names of the extractors of a case class's companion, which pattern matching calls.
const UNAPPLIES: [&str; 2] = ["unapply", "unapplySeq"];

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
            tag @ (EXTref | EXTMODCLASSref) => self.external(u, i, tag == EXTMODCLASSref)?,
            _ => {
                let h = self.header(i)?;
                self.local(u, i, &h)?
            }
        };
        Ok(*self.syms.entry(i).or_insert(s))
    }

    /// Enters symbol entry `i` of header `h` if it is a root or a member of a root.
    fn local(&mut self, u: &mut Universe, i: usize, h: &Header) -> Result<Sym, String> {
        if h.tag == CLASSsym && h.name == REFINE_CLASS {
            return Ok(Sym::Skipped);
        }
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
    pub fn is_own(&self, s: SymbolId) -> bool {
        self.own.contains(&s)
    }

    /// Enters member entry `i` of class `owner`, or refuses its name: a value, variable, method,
    /// type member, or a class or object of an object; a class or object of a class, an expanded
    /// name, a macro, the extractor of a case class and a private lazy value of a trait are
    /// refused; a private class parameter's getter waits for `private_values`; other private
    /// members, existential and stray type parameters (i12641), case accessors `x$access$N` and
    /// extension methods of an object are skipped.
    fn member(&mut self, u: &mut Universe, i: usize, h: &Header, owner: SymbolId) -> Result<Sym, String> {
        let flags = h.flags;
        let method = flags & f::METHOD != 0;
        let mut name = decode(&h.name);
        if method && name == TRAIT_CONSTRUCTOR {
            name = nme::CONSTRUCTOR.to_owned();
            u.set_inits(owner);
        }
        let private = flags & f::PRIVATE != 0 && name != nme::CONSTRUCTOR;
        let module_class = u.symbol(owner).flags.has(OBJECT);
        if private && flags & f::LAZY != 0 && u.is_trait(owner) {
            self.refused.push((owner, name, "a private lazy value".to_owned()));
            return Ok(Sym::Skipped);
        }
        if private && method && flags & f::PARAMACCESSOR != 0 && !module_class && !u.is_trait(owner) {
            self.private_values.push((i, owner));
            return Ok(Sym::Skipped);
        }
        let skipped = private
            || h.tag == TYPEsym && flags & (f::PARAM | f::EXISTENTIAL) != 0
            || flags & f::CASEACCESSOR != 0 && name.contains("$access$")
            || module_class && name.ends_with("$extension");
        if skipped {
            return Ok(Sym::Skipped);
        }
        let refusal = match h.tag {
            CLASSsym | MODULEsym if !module_class => Some("a class or object of a class"),
            CLASSsym | MODULEsym if flags & f::EXPANDEDNAME != 0 => Some("an expanded name"),
            CLASSsym | MODULEsym => return self.class_or_object(u, i, h, owner, &name),
            _ if flags & f::MACRO != 0 => Some("a macro"),
            _ if UNAPPLIES.contains(&name.as_str()) && is_case_companion(u, owner) => {
                Some("the extractor of a case class")
            }
            _ if flags & (f::EXPANDEDNAME | f::SUPERACCESSOR) != 0 => Some("an expanded name"),
            TYPEsym | ALIASsym => None,
            _ if !method => Some("a field without accessor"),
            _ => None,
        };
        if let Some(why) = refusal {
            if flags & f::IMPLICIT != 0 {
                return Err(format!("implicit member `{name}`: {why}"));
            }
            self.refused.push((owner, name, why.to_owned()));
            return Ok(Sym::Skipped);
        }
        let getter = flags & f::ACCESSOR != 0 && !name.ends_with("_=");
        let mut tags = f::visibility_tags(h.flags, h.within.is_some());
        tags.extend(f::member_tags(flags, getter));
        let kind = match h.tag {
            TYPEsym | ALIASsym => Kind::Type,
            _ if getter => Kind::Val,
            _ => Kind::Def,
        };
        let s = synthetic(u, owner, &name, kind, &tags);
        u.set_deferred(s, flags & f::DEFERRED != 0);
        self.enter(u, i, h, s)?;
        Ok(Sym::Known(s))
    }

    /// Records `s` as entered from entry `i` and gives it the access qualifier of `h`.
    pub fn enter(&mut self, u: &mut Universe, i: usize, h: &Header, s: SymbolId) -> Result<(), String> {
        self.entered.push((i, s));
        self.own.insert(s);
        self.syms.insert(i, Sym::Known(s));
        if let Some(within) = h.within {
            let Sym::Known(q) = self.sym(u, within)? else { return Ok(()) };
            u.set_access_qualifier(s, Some(q));
        }
        Ok(())
    }
}
