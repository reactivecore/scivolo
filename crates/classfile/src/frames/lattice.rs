//! The verifier's types (JVMS §4.10.1.2) and their merge where control flow joins.

use crate::kind::Kind;
use crate::split_method;

/// The common superclass of two classes, internal names; none if unknown.
pub type Lub<'a> = dyn FnMut(&str, &str) -> Option<String> + 'a;

const OBJECT: &str = "java/lang/Object";

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum VType {
    Top,
    Int,
    Float,
    Long,
    Double,
    Null,
    UninitThis,
    /// An object created by the `new` at this offset, before its constructor call.
    Uninit(u16),
    /// A class or array of this internal name.
    Object(String),
}

impl VType {
    /// The type of a value of field descriptor `d`, none for `V`.
    pub fn of(d: &str) -> Option<VType> {
        Some(match Kind::of_letter(*d.as_bytes().first()?)?.widened() {
            Kind::Int => VType::Int,
            Kind::Float => VType::Float,
            Kind::Long => VType::Long,
            Kind::Double => VType::Double,
            Kind::Reference => match d.strip_prefix('L') {
                Some(class) => VType::Object(class[..class.len() - 1].to_owned()),
                None => VType::Object(d.to_owned()),
            },
            _ => return None,
        })
    }

    pub fn wide(&self) -> bool {
        matches!(self, VType::Long | VType::Double)
    }
}

/// The types of the locals and the stack, a long or double as itself followed by `Top`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frame {
    pub locals: Vec<VType>,
    pub stack: Vec<VType>,
}

impl Frame {
    /// The frame at the start of method `name` of `class` with `descriptor`, with `max_locals`.
    pub fn initial(class: &str, is_static: bool, name: &str, descriptor: &str, max_locals: usize) -> Frame {
        let mut f = Frame { locals: Vec::new(), stack: Vec::new() };
        if !is_static {
            let this = match name == "<init>" && class != OBJECT {
                true => VType::UninitThis,
                false => VType::Object(class.to_owned()),
            };
            f.locals.push(this);
        }
        for t in split_method(descriptor).0.into_iter().filter_map(VType::of) {
            let wide = t.wide();
            f.locals.push(t);
            if wide {
                f.locals.push(VType::Top);
            }
        }
        f.locals.resize(max_locals.max(f.locals.len()), VType::Top);
        f
    }

    pub fn push(&mut self, t: VType) {
        let wide = t.wide();
        self.stack.push(t);
        if wide {
            self.stack.push(VType::Top);
        }
    }

    /// Pushes a value of field descriptor `d`, nothing for `V`.
    pub fn push_desc(&mut self, d: &str) {
        if let Some(t) = VType::of(d) {
            self.push(t)
        }
    }

    /// Pops `slots` slots; returns the deepest of them.
    pub fn pop(&mut self, slots: usize) -> Result<VType, String> {
        let at = self.stack.len().checked_sub(slots).ok_or("stack underflow")?;
        Ok(self.stack.drain(at..).next().unwrap_or(VType::Top))
    }

    /// Merges `from` into `self`; whether `self` changed.
    pub fn merge(&mut self, from: &Frame, lub: &mut Lub) -> Result<bool, String> {
        if self.stack.len() != from.stack.len() {
            return Err("stack heights differ where control flow joins".into());
        }
        let mut changed = false;
        let locals = self.locals.iter_mut().zip(&from.locals).map(|p| (p, false));
        for ((a, b), stack) in locals.chain(self.stack.iter_mut().zip(&from.stack).map(|p| (p, true))) {
            let m = join(a, b, lub)?;
            if stack && m == VType::Top && *a != VType::Top {
                return Err(format!("stack types {a:?} and {b:?} differ where control flow joins"));
            }
            changed |= m != *a;
            *a = m;
        }
        Ok(changed)
    }
}

fn join(a: &VType, b: &VType, lub: &mut Lub) -> Result<VType, String> {
    Ok(match (a, b) {
        _ if a == b => a.clone(),
        (VType::Null, VType::Object(_)) => b.clone(),
        (VType::Object(_), VType::Null) => a.clone(),
        (VType::Object(x), VType::Object(y)) => VType::Object(lub_class(x, y, lub)?),
        _ => VType::Top,
    })
}

/// The common superclass of classes or arrays `x` and `y` (ASM's `Frame.merge`).
fn lub_class(x: &str, y: &str, lub: &mut Lub) -> Result<String, String> {
    if x == y || y == OBJECT {
        return Ok(y.to_owned());
    }
    if x == OBJECT {
        return Ok(x.to_owned());
    }
    fn element(d: &str) -> Option<&str> {
        d.strip_prefix('L').and_then(|d| d.strip_suffix(';')).or(d.starts_with('[').then_some(d))
    }
    match (x.strip_prefix('['), y.strip_prefix('[')) {
        (Some(ex), Some(ey)) => match (element(ex), element(ey)) {
            (Some(cx), Some(cy)) => {
                let c = lub_class(cx, cy, lub)?;
                Ok(if c.starts_with('[') { format!("[{c}") } else { format!("[L{c};") })
            }
            _ => Ok(OBJECT.into()),
        },
        (None, None) => lub(x, y).ok_or_else(|| format!("no common superclass of {x} and {y} known")),
        _ => Ok(OBJECT.into()),
    }
}
