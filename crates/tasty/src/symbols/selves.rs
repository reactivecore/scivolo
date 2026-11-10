use super::{Kind, SymbolId, Universe};
use crate::tags::{DEFDEF, OBJECT, SELFDEF, TYPEDEF, VALDEF};
use crate::types::{Type, TypeId};

impl Universe {
    /// The given self type of class `c` (dotty's `givenSelfType`), if it declares one other than
    /// its own type or its object's.
    pub fn given_self_type(&mut self, c: SymbolId) -> Option<TypeId> {
        if let Some(&t) = self.self_types.get(&c) {
            return t;
        }
        let t = self.decode_self_type(c);
        self.self_types.insert(c, t);
        t
    }

    /// Lists the base classes of the classes in the given self type of class `c`: those of `A`
    /// and of `B` for `trait T { this: A & B => }`.
    pub fn self_type_bases(&mut self, c: SymbolId) -> Vec<SymbolId> {
        let Some(self_type) = self.given_self_type(c) else { return Vec::new() };
        let mut todo = vec![self_type];
        let mut out = Vec::new();
        while let Some(t) = todo.pop() {
            if let Type::And(x, y) = *self.types().get(t) {
                todo.extend([x, y]);
            } else if let Some(class) = self.class_symbol(t) {
                out.extend(self.linearization(class));
            }
        }
        out
    }

    fn decode_self_type(&mut self, c: SymbolId) -> Option<TypeId> {
        let (file, typedef) = self.symbol(c).tree.filter(|_| self.symbol(c).kind == Kind::Class)?;
        let (ast, _) = self.file(file);
        let template = *ast.kids(typedef).first()?;
        let selfdef = ast
            .kids(template)
            .iter()
            .copied()
            .take_while(|&k| !matches!(ast.tag(k), DEFDEF | VALDEF | TYPEDEF))
            .find(|&k| ast.tag(k) == SELFDEF)?;
        let tpt = *ast.kids(selfdef).first()?;
        let t = self.type_at(file, tpt);
        let own = self.symbol(c).flags.has(OBJECT) || self.class_symbol(t) == Some(c);
        (!own).then_some(t)
    }
}
