//! The library image shared by the VM (NIR) and the frontend (TASTy), and the upkeep of the
//! cache directory that holds it (ADR 0005).

mod build;
pub mod cache;
mod image;

pub use image::{Image, Source, Spec};
