//! Decoding a `MATCHtpt`: a case is a `MatchCase`, under a `Lambda` over its pattern's
//! `BIND`s when it has any.

use super::{Binder, Decoder, Type, TypeId};
use crate::tags::*;
use crate::{ANY, NodeId};

impl Decoder<'_> {
    pub fn match_tpt(&mut self, id: NodeId) -> Type {
        let ast = self.ast;
        let (cases, heads): (Vec<NodeId>, Vec<NodeId>) =
            ast.kids(id).iter().map(|&c| ast.deref(c)).partition(|&c| ast.tag(c) == CASEDEF);
        let (bound, scrutinee) = match heads[..] {
            [bound, scrutinee] => (self.ty(bound), self.ty(scrutinee)),
            [scrutinee] => (ANY, self.ty(scrutinee)),
            _ => return Type::Other(MATCHtpt),
        };
        let cases = cases.into_iter().map(|c| self.case(c)).collect();
        Type::Match { bound, scrutinee, cases }
    }

    /// The capture the `BIND` at `id` stands for.
    pub fn capture(&self, id: NodeId) -> Option<(Binder, u32)> {
        self.captures.iter().find(|c| c.0 == id).map(|&(_, b, i)| (b, i))
    }

    fn case(&mut self, id: NodeId) -> TypeId {
        let k = self.ast.kids(id);
        let [pat, body, ..] = k[..] else { return self.intern(Type::Other(CASEDEF)) };
        let mut binds = Vec::new();
        self.binds(pat, &mut binds);
        let binder = self.binder(id);
        let outer = self.captures.len();
        self.captures.extend(binds.iter().enumerate().map(|(i, &b)| (b, binder, i as u32)));
        let params = binds.iter().map(|&b| (self.name(b), self.kid(b, 0))).collect::<Vec<_>>();
        let case = Type::MatchCase(self.ty(pat), self.ty(body));
        self.captures.truncate(outer);
        let case = self.intern(case);
        match params.is_empty() {
            true => case,
            false => self.intern(Type::Lambda { binder, params, result: case }),
        }
    }

    /// The `BIND` trees in the pattern at `id`, in order.
    fn binds(&self, id: NodeId, out: &mut Vec<NodeId>) {
        if self.ast.tag(id) == BIND {
            out.push(id);
        }
        for &k in self.ast.kids(id) {
            self.binds(k, out);
        }
    }
}
