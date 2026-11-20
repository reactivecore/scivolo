// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/unpickleScala2/Scala2Unpickler.scala
//! The order in which a pickle is entered: roots, symbols, classes, type members, the other
//! members, annotations; and the members refused on the way.

use super::super::{Kind, SymbolId, Universe};
use super::{Sym, Unpickler};
use crate::tags::IMPLICIT;

impl Unpickler {
    /// Enters the symbols of the pickle, then reads their types: the roots entered.
    pub fn run(&mut self, u: &mut Universe) -> Result<Vec<SymbolId>, String> {
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
        self.private_values(u)?;
        let entered = self.entered.clone();
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
    pub fn ensure_type_info(&mut self, u: &mut Universe, s: SymbolId) -> Result<(), String> {
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
}
