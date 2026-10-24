use super::{Kind, SymbolId, Universe};
use crate::tags::{DEFDEF, OBJECT, SELFDEF, TYPEDEF, VALDEF};
use crate::types::TypeId;

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
