//! Method, polymorphic and type lambda types.

use super::Decoder;
use crate::NodeId;
use crate::tags::*;
use crate::types::Type;

impl Decoder<'_> {
    /// A `POLYtype`, `METHODtype` or `TYPELAMBDAtype`: its result, then each parameter's type
    /// and name, then for a method its modifiers.
    pub fn lambda_type(&mut self, id: NodeId) -> Type {
        let (tag, k) = (self.ast.tag(id), self.ast.kids(id));
        let result = self.ty(k[0]);
        let mut params = Vec::new();
        let (mut implicit, mut contextual) = (false, false);
        let mut rest = k[1..].iter().copied();
        while let Some(c) = rest.next() {
            match self.ast.tag(c) {
                t @ (IMPLICIT | GIVEN) => (implicit, contextual) = (true, t == GIVEN),
                t if is_modifier(t) => {}
                _ => {
                    let ty = self.ty(c);
                    let name = rest.next().map_or_else(String::new, |n| self.name(n));
                    params.push((name, ty));
                }
            }
        }
        let binder = self.binder(id);
        match tag {
            POLYtype => Type::Poly { binder, params, result },
            METHODtype => Type::Method { binder, params, result, implicit, contextual },
            _ => Type::Lambda { binder, params, result },
        }
    }

    /// A `LAMBDAtpt`: its parameters, then its body.
    pub fn lambda_tpt(&mut self, id: NodeId) -> Type {
        let k = self.ast.kids(id);
        let (params, body) = k.split_at(k.len() - 1);
        let params = params.iter().map(|&p| (self.name(p), self.kid(p, 0))).collect();
        Type::Lambda { binder: self.binder(id), params, result: self.ty(body[0]) }
    }
}
