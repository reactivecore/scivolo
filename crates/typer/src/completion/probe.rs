//! The probe a completion request types a unit with: what it records at the tree under the
//! cursor, which types as an error tree.

use crate::ctx::Ctx;
use crate::tpd::TNodeId;
use crate::typer::Typer;
use syntax::ast::{NodeId, Span};
use syntax::source::Offset;
use tasty::{Type, TypeId};

pub struct Probe {
    target: NodeId,
    cursor: Offset,
    /// The context of the first typing of the target.
    ctx: Option<Ctx>,
    /// The qualifier's type, zonked, at each typing of the target selection.
    quals: Vec<TypeId>,
    /// Whether a construct outside the subset is at or before the cursor, or has no position.
    blocked: bool,
}

impl Probe {
    pub fn new(target: NodeId, cursor: Offset) -> Probe {
        Probe { target, cursor, ctx: None, quals: Vec::new(), blocked: false }
    }
}

impl Typer<'_> {
    pub fn is_probe_target(&self, id: NodeId) -> bool {
        self.probe.as_ref().is_some_and(|p| p.target == id)
    }

    /// Records `ctx` if `id` is the target typed for the first time.
    pub fn probe_context(&mut self, id: NodeId, ctx: Ctx) {
        if let Some(p) = &mut self.probe
            && p.target == id
        {
            p.ctx.get_or_insert(ctx);
        }
    }

    /// Records the type of `qual`, the qualifier of target selection `id`; an error tree for it.
    pub fn probe_selection(&mut self, id: NodeId, qual: TNodeId) -> TNodeId {
        let tpe = self.tpd.tpe(qual);
        let tpe = self.zonk(tpe);
        if let Some(p) = &mut self.probe {
            p.quals.push(tpe);
        }
        self.error_tree(self.ast.span(id))
    }

    /// Notes a construct outside the subset at `span`.
    pub fn probe_unsupported(&mut self, span: Span) {
        if let Some(p) = &mut self.probe {
            p.blocked |= !span.exists() || span.start() <= p.cursor;
        }
    }

    /// The names of the members starting with `prefix` the probe's selection offers; `None`
    /// where they are not all known.
    pub fn completions(&mut self, prefix: &str) -> Option<Vec<String>> {
        let probe = self.probe.take()?;
        let (Some(ctx), [qual, rest @ ..]) = (probe.ctx, &probe.quals[..]) else { return None };
        if probe.blocked || rest.iter().any(|q| q != qual) || self.has_unknowns(*qual) {
            return None;
        }
        self.member_completions(*qual, prefix, ctx)
    }

    /// Whether type `t` holds an error or a type variable.
    fn has_unknowns(&mut self, t: TypeId) -> bool {
        let mut found = false;
        self.u.map_type(t, &mut |u, part| {
            found |= matches!(u.types().get(part), Type::Error | Type::Var(_));
            None
        });
        found
    }
}
