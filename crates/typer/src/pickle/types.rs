// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Types (dotty's `pickleType`, `pickleNewType`, `pickleMethodic`, `pickleConstant`).

use super::names::qualified;
use super::sharing::BY_NAME;
use super::{Refusal, TreePickler};
use tasty::tags::*;
use tasty::write::{Addr, Name};
use tasty::{Binder, Type, TypeId};

impl TreePickler<'_> {
    fn types(&mut self, ts: &[TypeId], rich: bool) -> Result<(), Refusal> {
        ts.iter().try_for_each(|&t| self.ty(t, rich))
    }

    pub fn new_type(&mut self, t: TypeId, rich: bool, at: Addr) -> Result<(), Refusal> {
        match self.u.types().get(t).clone() {
            Type::Applied(tycon, args) => self.with_length(APPLIEDtype, &mut |p| {
                p.ty(tycon, false)?;
                p.types(&args, false)
            }),
            Type::MatchCase(pat, body) => {
                self.with_length(MATCHCASEtype, &mut |p| p.types(&[pat, body], false))
            }
            Type::Constant(..) | Type::Str(_) => self.constant(t),
            Type::Sym(prefix, s) => self.sym_ref(prefix, s),
            Type::TypeRef(prefix, name) | Type::TermRef(prefix, name) => {
                if self.keying {
                    self.byte(BY_NAME);
                }
                self.named_ref(prefix, &name, matches!(self.u.types().get(t), Type::TypeRef(..)))
            }
            Type::Package(p) => {
                self.package_ref(p);
                Ok(())
            }
            Type::PackageName(n) => {
                self.byte(TERMREFpkg);
                self.name(&qualified(&n));
                Ok(())
            }
            Type::This(class) => match *self.u.types().get(class) {
                Type::Package(p) => {
                    self.new_package_this(p);
                    Ok(())
                }
                Type::PackageName(_) => Err("the this of a package the universe does not have"),
                _ => {
                    self.byte(THIS);
                    self.ty(class, false)
                }
            },
            Type::Super(this, sup) => self.with_length(SUPERtype, &mut |p| p.types(&[this, sup], false)),
            Type::RecThis(binder) => {
                let at = *self.binders.get(&binder).ok_or("a recursive type's this outside it")?;
                self.byte(RECthis);
                self.buf.write_ref(at);
                Ok(())
            }
            Type::Skolem(_, info) => self.ty(info, false),
            Type::Refined(parent, name, info) => self.with_length(REFINEDtype, &mut |p| {
                p.name(&Name::utf8(&name));
                p.ty(parent, false)?;
                p.ty(info, true)
            }),
            Type::Rec(binder, parent) => {
                self.binders.insert(binder, at);
                self.byte(RECtype);
                self.ty(parent, false)
            }
            Type::Alias(hi) | Type::Bounds(_, hi)
                if matches!(self.u.types().get(hi), Type::Lambda { .. }) =>
            {
                Err("bounds of a type lambda, whose variances are not known")
            }
            Type::Alias(alias) => self.with_length(TYPEBOUNDS, &mut |p| p.ty(alias, rich)),
            Type::Bounds(lo, hi) => self.with_length(TYPEBOUNDS, &mut |p| p.types(&[lo, hi], rich)),
            Type::And(a, b) => self.with_length(ANDtype, &mut |p| p.types(&[a, b], rich)),
            Type::Or(a, b, _) => self.with_length(ORtype, &mut |p| p.types(&[a, b], rich)),
            Type::ByName(result) => {
                self.byte(BYNAMEtype);
                self.ty(result, false)
            }
            Type::Lambda { binder, params, result } => {
                self.methodic(TYPELAMBDAtype, (binder, at), &params, result, None)
            }
            Type::Match { bound, scrutinee, cases } => self.with_length(MATCHtype, &mut |p| {
                p.types(&[bound, scrutinee], false)?;
                p.types(&cases, false)
            }),
            Type::Poly { binder, params, result } if rich => {
                self.methodic(POLYtype, (binder, at), &params, result, None)
            }
            Type::Method { binder, params, result, implicit, contextual } if rich => {
                let mods = if contextual { Some(GIVEN) } else { implicit.then_some(IMPLICIT) };
                self.methodic(METHODtype, (binder, at), &params, result, mods)
            }
            Type::Param(binder, i) => self.param(binder, i),
            Type::Local(def, name) => {
                let (binder, i) = self
                    .params
                    .iter()
                    .filter(|(b, _)| b.0 == def.0 && b.1 < def.1)
                    .filter_map(|(&b, names)| Some((b, names.iter().position(|n| *n == name)?)))
                    .max_by_key(|(b, _)| b.1)
                    .ok_or("a parameter reference outside its binder")?;
                self.param(binder, i as u32)
            }
            _ => Err("a type outside the subset pickled"),
        }
    }

    fn methodic(
        &mut self,
        tag: u8,
        (binder, at): (Binder, Addr),
        params: &[(String, TypeId)],
        result: TypeId,
        modifier: Option<u8>,
    ) -> Result<(), Refusal> {
        self.binders.insert(binder, at);
        self.params.insert(binder, params.iter().map(|(n, _)| n.clone()).collect());
        self.with_length(tag, &mut |p| {
            p.ty(result, true)?;
            for (name, t) in params {
                p.ty(*t, false)?;
                p.name(&Name::utf8(name));
            }
            modifier.into_iter().for_each(|m| p.byte(m));
            Ok(())
        })
    }

    /// A reference to parameter `i` of the method or type lambda `binder`.
    fn param(&mut self, binder: Binder, i: u32) -> Result<(), Refusal> {
        let at = *self.binders.get(&binder).ok_or("a parameter reference outside its binder")?;
        self.with_length(PARAMtype, &mut |p| {
            p.buf.write_ref(at);
            p.nat(i as u64);
            Ok(())
        })
    }

    /// Constant type `t` (dotty's `pickleConstant`).
    pub fn constant(&mut self, t: TypeId) -> Result<(), Refusal> {
        let (tag, bits) = match self.u.types().get(t).clone() {
            Type::Constant(tag, bits) => (tag, bits),
            Type::Str(s) => {
                self.byte(STRINGconst);
                self.name(&Name::utf8(&s));
                return Ok(());
            }
            _ => return Err("a constant of another type"),
        };
        self.byte(tag);
        match tag {
            BYTEconst | SHORTconst | INTconst | LONGconst | DOUBLEconst => self.buf.buf.write_long_int(bits),
            FLOATconst => self.buf.buf.write_long_int(i64::from(bits as u32 as i32)),
            CHARconst => self.nat(bits as u64),
            CLASSconst => return self.ty(TypeId(bits as u32), false),
            _ => {}
        }
        Ok(())
    }
}
