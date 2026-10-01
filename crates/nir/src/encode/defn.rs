use super::{Out, Writer};
use crate::nir::{Attr, Body, Defn, DefnKind, GlobalId};
use crate::prelude::Section;
use crate::tags::{attr, defn};

impl Writer<'_> {
    pub fn defn(&mut self, out: &mut Out, d: &Defn) {
        out.u8(match d.kind {
            DefnKind::Var { .. } => defn::VAR,
            DefnKind::Const { .. } => defn::CONST,
            DefnKind::Declare { .. } => defn::DECLARE,
            DefnKind::Define { .. } => defn::DEFINE,
            DefnKind::Trait { .. } => defn::TRAIT,
            DefnKind::Class { .. } => defn::CLASS,
            DefnKind::Module { .. } => defn::MODULE,
        });
        self.global(out, d.name);
        out.uleb(d.attrs.len() as u64);
        d.attrs.iter().for_each(|a| self.attr(out, a));
        self.position(out, d.pos);
        match &d.kind {
            DefnKind::Var { ty, rhs } | DefnKind::Const { ty, rhs } => {
                self.ty(out, *ty);
                self.val(out, *rhs);
            }
            DefnKind::Declare { ty } => self.ty(out, *ty),
            DefnKind::Define { ty, body } => {
                self.ty(out, *ty);
                self.body(out, body);
            }
            DefnKind::Trait { traits } => self.globals(out, traits),
            DefnKind::Class { parent, traits } | DefnKind::Module { parent, traits } => {
                out.bool(parent.is_some());
                if let Some(p) = parent {
                    self.global(out, *p);
                }
                self.globals(out, traits);
            }
        }
    }

    fn globals(&mut self, out: &mut Out, gs: &[GlobalId]) {
        out.uleb(gs.len() as u64);
        gs.iter().for_each(|&g| self.global(out, g));
    }

    /// The instructions go to the instructions section; the local names and scopes follow the
    /// reference to them.
    fn body(&mut self, out: &mut Out, body: &Body) {
        let mut insts = Out::default();
        insts.uleb(body.insts.len() as u64);
        for (inst, &pos) in body.insts.iter().zip(&body.positions) {
            self.inst(&mut insts, inst, pos);
        }
        let section = self.section(Section::Insts);
        let off = section.len();
        section.extend_from_slice(&insts.0);
        out.uleb(off as u64);
        out.uleb(body.local_names.len() as u64);
        for &(local, name) in &body.local_names {
            self.local(out, local);
            self.string(out, name);
        }
        out.uleb(body.scopes.len() as u64);
        for s in &body.scopes {
            out.uleb(s.id as u64);
            out.uleb(s.parent as u64);
            self.position(out, s.pos);
        }
    }

    fn attr(&mut self, out: &mut Out, a: &Attr) {
        let tag = match a {
            Attr::MayInline => attr::MAY_INLINE,
            Attr::InlineHint => attr::INLINE_HINT,
            Attr::NoInline => attr::NO_INLINE,
            Attr::AlwaysInline => attr::ALWAYS_INLINE,
            Attr::MaySpecialize => attr::MAY_SPECIALIZE,
            Attr::NoSpecialize => attr::NO_SPECIALIZE,
            Attr::UnOpt => attr::UN_OPT,
            Attr::NoOpt => attr::NO_OPT,
            Attr::DidOpt => attr::DID_OPT,
            Attr::BailOpt(_) => attr::BAIL_OPT,
            Attr::Dyn => attr::DYN,
            Attr::Stub => attr::STUB,
            Attr::Extern { .. } => attr::EXTERN,
            Attr::Link(_) => attr::LINK,
            Attr::Define(_) => attr::DEFINE,
            Attr::Abstract => attr::ABSTRACT,
            Attr::Volatile => attr::VOLATILE,
            Attr::Final => attr::FINAL,
            Attr::SafePublish => attr::SAFE_PUBLISH,
            Attr::LinkCppRuntime => attr::LINK_CPP_RUNTIME,
            Attr::LinktimeResolved => attr::LINKTIME_RESOLVED,
            Attr::UsesIntrinsic => attr::USES_INTRINSIC,
            Attr::Align { .. } => attr::ALIGN,
        };
        out.u8(tag);
        match a {
            Attr::BailOpt(s) | Attr::Link(s) | Attr::Define(s) => self.string(out, *s),
            Attr::Extern { blocking } => out.bool(*blocking),
            Attr::Align { size, group } => {
                out.sleb(*size as i64);
                out.bool(group.is_some());
                if let Some(g) = group {
                    self.string(out, *g);
                }
            }
            _ => {}
        }
    }
}
