//! Abrupt completion: thrown exceptions and fatal VM errors, and the helpers that throw.

use crate::world::LoadError;
use crate::{Vm, VmError};

/// Why a call did not return normally; boxed so `Flow` stays two words.
pub struct Unwind(Box<Unwound>);

pub enum Unwound {
    Throw(usize),
    Fatal(VmError),
}

pub type Flow<T = u64> = Result<T, Unwind>;

impl Unwind {
    pub fn throw(exc: usize) -> Self {
        Unwind(Box::new(Unwound::Throw(exc)))
    }

    /// The exception object, unless this is a fatal error.
    pub fn thrown(&self) -> Option<usize> {
        match *self.0 {
            Unwound::Throw(exc) => Some(exc),
            Unwound::Fatal(_) => None,
        }
    }

    pub fn into_inner(self) -> Unwound {
        *self.0
    }
}

impl From<VmError> for Unwind {
    fn from(e: VmError) -> Self {
        Unwind(Box::new(Unwound::Fatal(e)))
    }
}

impl From<crate::world::LoadError> for Unwind {
    fn from(e: crate::world::LoadError) -> Self {
        VmError::from(e).into()
    }
}

impl Vm {
    /// The runtime helper `scala.scalanative.runtime.package$::<sig>` raising its exception.
    pub fn throw_runtime(&mut self, sig: &str, args: &[u64]) -> Flow {
        let owner = self.world.top("scala.scalanative.runtime.package$");
        let module = self.module(owner)?;
        let g = self.world.member(owner, sig);
        let mut all = vec![module as u64];
        all.extend_from_slice(args);
        self.call_global(g, &all)?;
        Err(VmError::Unsupported(format!("{sig} returned")).into())
    }

    /// Throws a new instance of `class`, built by its no-argument constructor.
    pub fn throw_new(&mut self, class: &str) -> Flow {
        let cls = self.link_top(class)?;
        let obj = self.alloc_object(cls);
        let init = self.world.member(self.class(cls).name, "RE");
        self.call_global(init, &[obj as u64])?;
        Err(Unwind::throw(obj))
    }

    /// `value` if it is a `Throwable`; otherwise the `NullPointerException` or, as a JVM's cast
    /// to `Nothing$` would, the `ClassCastException` to throw instead.
    pub fn throwable(&mut self, value: usize) -> Flow<usize> {
        if value == 0 {
            return self.null_pointer();
        }
        let throwable = self.link_top("java.lang.Throwable")?;
        let class = self.class_of(value);
        if self.is_subclass(class, throwable) {
            return Ok(value);
        }
        let (from, to) = (self.class(class).rtti as u64, self.class(throwable).rtti as u64);
        self.throw_runtime("D14throwClassCastR_R_nEO", &[from, to]).map(|_| unreachable!("throws"))
    }

    /// A missing definition or native as the `NoClassDefFoundError` or `UnsatisfiedLinkError` a
    /// JVM would throw, another unsupported feature as an `InternalError` (which `NonFatal`
    /// handlers do not catch), a failed static initializer as `ExceptionInInitializerError`;
    /// any other unwinding unchanged.
    pub fn as_linkage_error(&mut self, u: Unwind) -> Unwind {
        if let Unwound::Fatal(VmError::Initializer(cause)) = *u.0 {
            return match self.throw_initializer_error(cause) {
                Err(thrown) if thrown.thrown().is_some() => thrown,
                _ => u,
            };
        }
        let (class, message) = match &*u.0 {
            Unwound::Fatal(VmError::Load(LoadError::Missing(name))) => {
                ("java.lang.NoClassDefFoundError", name.clone())
            }
            Unwound::Fatal(VmError::Unsupported(what)) if what.starts_with("native ") => {
                ("java.lang.UnsatisfiedLinkError", what.clone())
            }
            Unwound::Fatal(VmError::Unsupported(what)) => {
                ("java.lang.InternalError", format!("scalai: unsupported: {what}"))
            }
            _ => return u,
        };
        match self.throw_new_with(class, &message) {
            Err(thrown) if thrown.thrown().is_some() => thrown,
            _ => u,
        }
    }

    /// Throws an `ExceptionInInitializerError` caused by `cause`.
    fn throw_initializer_error(&mut self, cause: usize) -> Flow {
        let cls = self.link_top("java.lang.ExceptionInInitializerError")?;
        let obj = self.alloc_object(cls);
        let init = self.world.member(self.class(cls).name, "RL19java.lang.ThrowableE");
        self.call_global(init, &[obj as u64, cause as u64])?;
        Err(Unwind::throw(obj))
    }

    /// Throws a new instance of `class`, built by its constructor taking a message.
    pub fn throw_new_with(&mut self, class: &str, message: &str) -> Flow {
        let cls = self.link_top(class)?;
        let obj = self.alloc_object(cls);
        let text = self.new_string(message)?;
        let init = self.world.member(self.class(cls).name, "RL16java.lang.StringE");
        self.call_global(init, &[obj as u64, text as u64])?;
        Err(Unwind::throw(obj))
    }

    pub fn null_pointer<T>(&mut self) -> Flow<T> {
        if self.funcs.trace {
            eprintln!("null pointer in:\n  {}", self.backtrace(12).join("\n  "));
        }
        self.throw_runtime("D16throwNullPointernEO", &[]).map(|_| unreachable!("throws"))
    }

    pub fn stack_overflow(&mut self) -> Flow {
        self.funcs.overflowing = true;
        let result = self.throw_new("java.lang.StackOverflowError");
        self.funcs.overflowing = false;
        result
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn flow_fits_two_registers() {
        assert_eq!(size_of::<super::Flow>(), 16);
    }
}
