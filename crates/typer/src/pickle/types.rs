// Derived from Scala 3.9.0 (Apache-2.0, see NOTICE); translated to Rust and changed:
//   compiler/src/dotty/tools/dotc/core/tasty/TreePickler.scala
//! Types (dotty's `pickleType`, `pickleNewType`, `pickleMethodic`, `pickleConstant`).

use super::names::{param_name, qualified};
use super::{Refusal, TreePickler};
use tasty::tags::*;
use tasty::write::{Addr, Name};
use tasty::{Binder, ParamList, Type, TypeId};

impl TreePickler<'_> {
    fn types(&mut self, ts: &[TypeId], rich: bool) -> Result<(), Refusal> {
        ts.iter().try_for_each(|&t| self.ty(t, rich))
    }

    pub fn new_type(&mut self, t: TypeId, rich: bool, at: Addr) -> Result<(), Refusal> {
        let ty = *self.u.ty(t);
        match ty {
            Type::Applied(tycon, args) => {
                let args = self.u.types_of(args).to_vec();
                self.with_length(APPLIEDtype, &mut |p| {
                    p.ty(tycon, false)?;
                    p.types(&args, false)
                })
            }
            Type::MatchCase(pat, body) => {
                self.with_length(MATCHCASEtype, &mut |p| p.types(&[pat, body], false))
            }
            Type::Constant(..) | Type::Str(_) => self.constant(t),
            Type::Sym(prefix, s) => self.sym_ref(prefix, s),
            Type::TypeRef(prefix, name) => self.named_type(prefix, name, true),
            Type::TermRef(prefix, name) => self.named_type(prefix, name, false),
            Type::Package(p) => {
                self.package_ref(p);
                Ok(())
            }
            Type::PackageName(n) => {
                self.byte(TERMREFpkg);
                let name = qualified(self.u.name(n));
                self.name(&name);
                Ok(())
            }
            Type::This(class) => match *self.u.ty(class) {
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
                let name = Name::utf8(p.u.name(name));
                p.name(&name);
                p.ty(parent, false)?;
                p.ty(info, true)
            }),
            Type::Rec(binder, parent) => {
                self.binders.insert(binder, at);
                self.byte(RECtype);
                self.ty(parent, false)
            }
            Type::Alias(alias) => {
                let variances = self.variances(alias, false)?;
                self.with_length(TYPEBOUNDS, &mut |p| {
                    p.ty(alias, rich)?;
                    p.write_variances(&variances);
                    Ok(())
                })
            }
            Type::Bounds(lo, hi) => {
                let variances = self.variances(hi, true)?;
                self.with_length(TYPEBOUNDS, &mut |p| {
                    p.types(&[lo, hi], rich)?;
                    p.write_variances(&variances);
                    Ok(())
                })
            }
            Type::And(a, b) => self.with_length(ANDtype, &mut |p| p.types(&[a, b], rich)),
            Type::Or(a, b, _) => self.with_length(ORtype, &mut |p| p.types(&[a, b], rich)),
            Type::Flexible(hi) => self.with_length(FLEXIBLEtype, &mut |p| p.ty(hi, rich)),
            Type::ByName(result) => {
                self.byte(BYNAMEtype);
                self.ty(result, false)
            }
            Type::Lambda { binder, params, result } => {
                self.methodic(TYPELAMBDAtype, (binder, at), params, result, None)
            }
            Type::Match { bound, scrutinee, cases } => {
                let parts = [&[bound, scrutinee][..], self.u.types_of(cases)].concat();
                self.with_length(MATCHtype, &mut |p| p.types(&parts, false))
            }
            Type::Poly { binder, params, result } if rich => {
                self.methodic(POLYtype, (binder, at), params, result, None)
            }
            Type::Method { binder, params, result, implicit, contextual } if rich => {
                let mods = if contextual { Some(GIVEN) } else { implicit.then_some(IMPLICIT) };
                self.methodic(METHODtype, (binder, at), params, result, mods)
            }
            Type::Annotated(underlying, class) if self.is_inferred_mark(class) => {
                self.with_length(ANNOTATEDtype, &mut |p| {
                    p.ty(underlying, rich)?;
                    p.new_annotation(class)
                })
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
            Type::Annotated(parent, annot) => self.annotated(t, parent, annot),
            _ => Err("a type outside the subset pickled"),
        }
    }

    fn methodic(
        &mut self,
        tag: u8,
        (binder, at): (Binder, Addr),
        params: ParamList,
        result: TypeId,
        modifier: Option<u8>,
    ) -> Result<(), Refusal> {
        let params = self.u.params_of(params).to_vec();
        self.binders.insert(binder, at);
        self.params.insert(binder, params.iter().map(|&(n, _)| n).collect());
        self.open.push(binder);
        let pickled = self.with_length(tag, &mut |p| {
            p.ty(result, true)?;
            for &(name, t) in &params {
                p.ty(t, false)?;
                let name = param_name(p.u.name(name));
                p.name(&name);
            }
            modifier.into_iter().for_each(|m| p.byte(m));
            Ok(())
        });
        self.open.pop();
        pickled
    }

    /// A reference to parameter `i` of the method or type lambda `binder`, outside the type of
    /// a method of a file the parameter's definition.
    fn param(&mut self, binder: Binder, i: u32) -> Result<(), Refusal> {
        let defined = self.method_binders.get(&binder).and_then(|ps| ps.get(i as usize)).copied();
        if let Some(s) = defined.filter(|_| self.file.is_some() && !self.open.contains(&binder)) {
            return self.direct_ref(s);
        }
        let at = *self.binders.get(&binder).ok_or("a parameter reference outside its binder")?;
        self.with_length(PARAMtype, &mut |p| {
            p.buf.write_ref(at);
            p.nat(i as u64);
            Ok(())
        })
    }

    /// Constant type `t` (dotty's `pickleConstant`).
    pub fn constant(&mut self, t: TypeId) -> Result<(), Refusal> {
        let (tag, bits) = match *self.u.ty(t) {
            Type::Constant(tag, bits) => (tag, bits),
            Type::Str(s) => {
                self.byte(STRINGconst);
                let s = Name::utf8(self.u.name(s));
                self.name(&s);
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
