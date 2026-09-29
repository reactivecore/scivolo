//! Scala 3 syntax: a port of dotty's scanner and parser producing untyped trees.

pub mod ast;
pub mod config;
pub mod diag;
pub mod names;
pub mod parse;
pub mod scan;
pub mod source;
pub mod token;
