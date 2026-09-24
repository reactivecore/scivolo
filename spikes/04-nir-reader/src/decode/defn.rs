use super::Reader;
use crate::cursor::Result;
use crate::nir::{Attr, Body, Defn, DefnKind, LexicalScope};
use crate::prelude::Section;
use crate::tags::{attr, defn};

impl Reader<'_> {
    pub fn defn(&mut self) -> Result<Defn> {
        let tag = self.tag()?;
        let name = self.global()?;
        let attrs = self.seq(Self::attr)?;
        let pos = self.position()?;
        let kind = match tag {
            defn::VAR => DefnKind::Var { ty: self.ty()?, rhs: self.val()? },
            defn::CONST => DefnKind::Const { ty: self.ty()?, rhs: self.val()? },
            defn::DECLARE => DefnKind::Declare { ty: self.ty()? },
            defn::DEFINE => DefnKind::Define { ty: self.ty()?, body: self.body()? },
            defn::TRAIT => DefnKind::Trait { traits: self.seq(Self::global)? },
            defn::CLASS => {
                DefnKind::Class { parent: self.opt(Self::global)?, traits: self.seq(Self::global)? }
            }
            defn::MODULE => {
                DefnKind::Module { parent: self.opt(Self::global)?, traits: self.seq(Self::global)? }
            }
            t => return self.bad_tag("definition", t),
        };
        Ok(Defn { name, attrs, pos, kind })
    }

    fn body(&mut self) -> Result<Body> {
        let off = self.c.uleb32()? as usize;
        if off >= self.prelude.len(Section::Insts) {
            return self.c.error(format!("instruction offset {off} out of bounds"));
        }
        let (insts, positions) = self.at(self.prelude.start(Section::Insts) + off, |r| {
            let n = r.c.len_prefixed()?;
            let (mut insts, mut positions) = (Vec::with_capacity(n), Vec::with_capacity(n));
            for _ in 0..n {
                let (inst, pos) = r.inst()?;
                insts.push(inst);
                positions.push(pos);
            }
            Ok((insts, positions))
        })?;
        let local_names = self.seq(|r| Ok((r.local()?, r.string()?)))?;
        let scopes =
            self.seq(|r| Ok(LexicalScope { id: r.c.uleb32()?, parent: r.c.uleb32()?, pos: r.position()? }))?;
        Ok(Body { insts, positions, local_names, scopes })
    }

    fn attr(&mut self) -> Result<Attr> {
        Ok(match self.tag()? {
            attr::MAY_INLINE => Attr::MayInline,
            attr::INLINE_HINT => Attr::InlineHint,
            attr::NO_INLINE => Attr::NoInline,
            attr::ALWAYS_INLINE => Attr::AlwaysInline,
            attr::MAY_SPECIALIZE => Attr::MaySpecialize,
            attr::NO_SPECIALIZE => Attr::NoSpecialize,
            attr::UN_OPT => Attr::UnOpt,
            attr::NO_OPT => Attr::NoOpt,
            attr::DID_OPT => Attr::DidOpt,
            attr::BAIL_OPT => Attr::BailOpt(self.string()?),
            attr::DYN => Attr::Dyn,
            attr::STUB => Attr::Stub,
            attr::EXTERN => Attr::Extern { blocking: self.c.bool()? },
            attr::LINK => Attr::Link(self.string()?),
            attr::DEFINE => Attr::Define(self.string()?),
            attr::ABSTRACT => Attr::Abstract,
            attr::VOLATILE => Attr::Volatile,
            attr::FINAL => Attr::Final,
            attr::SAFE_PUBLISH => Attr::SafePublish,
            attr::LINK_CPP_RUNTIME => Attr::LinkCppRuntime,
            attr::LINKTIME_RESOLVED => Attr::LinktimeResolved,
            attr::USES_INTRINSIC => Attr::UsesIntrinsic,
            attr::ALIGN => Attr::Align { size: self.c.sleb32()?, group: self.opt(Self::string)? },
            t => return self.bad_tag("attribute", t),
        })
    }
}
