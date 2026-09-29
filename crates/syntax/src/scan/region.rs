//! Regions: what encloses the current token (dotty's `Region` hierarchy), kept in a `Vec` and addressed by `RegionId`.

use super::indent::IndentWidth;
use crate::token::Token;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RegionId(u32);

#[derive(Clone, PartialEq, Debug)]
pub enum RegionKind {
    InString { multi_line: bool },
    InParens { prefix: Token },
    InBraces,
    InCase,
    SingleLineLambda,
    Indented { width: IndentWidth, prefix: Token },
}

pub struct Region {
    pub kind: RegionKind,
    pub outer: Option<RegionId>,
    pub known_width: Option<IndentWidth>,
    pub commas_expected: bool,
    /// Other indentation widths, greater than `width`, of lines in an `Indented` region.
    pub other_widths: Vec<IndentWidth>,
}

#[derive(Default)]
pub struct Regions {
    items: Vec<Region>,
}

impl Regions {
    pub fn push(&mut self, kind: RegionKind, outer: Option<RegionId>) -> RegionId {
        let known_width = match &kind {
            RegionKind::Indented { width, .. } => Some(width.clone()),
            _ => None,
        };
        let r = Region { kind, outer, known_width, commas_expected: false, other_widths: Vec::new() };
        self.items.push(r);
        RegionId(self.items.len() as u32 - 1)
    }

    pub fn get(&self, id: RegionId) -> &Region {
        &self.items[id.0 as usize]
    }

    pub fn get_mut(&mut self, id: RegionId) -> &mut Region {
        &mut self.items[id.0 as usize]
    }

    pub fn is_outermost(&self, id: RegionId) -> bool {
        self.get(id).outer.is_none()
    }

    pub fn enclosing(&self, id: RegionId) -> RegionId {
        self.get(id).outer.expect("an enclosing region")
    }

    pub fn is_indented(&self, id: RegionId) -> bool {
        matches!(self.get(id).kind, RegionKind::Indented { .. })
    }

    pub fn closed_by(&self, id: RegionId) -> Token {
        match self.get(id).kind {
            RegionKind::InString { .. } | RegionKind::InBraces => Token::RBrace,
            RegionKind::InParens { prefix } => prefix.closer(),
            RegionKind::InCase | RegionKind::Indented { .. } => Token::Outdent,
            RegionKind::SingleLineLambda => Token::EndLambda,
        }
    }

    /// The known indentation width, zero if unknown.
    pub fn indent_width(&self, id: RegionId) -> IndentWidth {
        self.get(id).known_width.clone().unwrap_or_else(IndentWidth::zero)
    }

    pub fn propose_known_width(&mut self, id: RegionId, width: &IndentWidth, last_token: Token) {
        if self.get(id).known_width.is_some() {
            return;
        }
        match self.get(id).kind {
            RegionKind::InParens { .. } if last_token != Token::LParen => self.use_outer_width(id),
            _ => self.get_mut(id).known_width = Some(width.clone()),
        }
    }

    fn use_outer_width(&mut self, id: RegionId) {
        let outer = self.enclosing(id);
        if self.get(outer).known_width.is_none() {
            self.use_outer_width(outer);
        }
        self.get_mut(id).known_width = self.get(outer).known_width.clone();
    }

    /// Whether `width` undents an enclosing indentation region.
    pub fn is_closed_by_undent_at(&self, id: RegionId, width: &IndentWidth) -> bool {
        if self.is_indented(id) {
            !self.is_outermost(id)
                && width.le(&self.indent_width(id))
                && self.covers_indent(self.enclosing(id), width)
        } else {
            self.is_closed_by_undent_at(self.enclosing(id), width)
        }
    }

    fn covers_indent(&self, id: RegionId, w: &IndentWidth) -> bool {
        let r = self.get(id);
        match &r.kind {
            RegionKind::Indented { width, .. } => width == w || r.other_widths.contains(w),
            _ => r.known_width.as_ref() == Some(w),
        }
    }

    pub fn commas_expected_in_enclosing(&self, id: RegionId) -> bool {
        let r = self.get(id);
        r.commas_expected
            || self.is_indented(id) && r.outer.is_some_and(|o| self.commas_expected_in_enclosing(o))
    }

    /// `id` and its enclosing regions, innermost first.
    pub fn chain(&self, id: RegionId) -> impl Iterator<Item = RegionId> + '_ {
        std::iter::successors(Some(id), |&r| self.get(r).outer)
    }
}
