//! Render pipeline: fetch → parse → style → layout → display list.

mod animation;
mod pipeline;
mod error;

pub use animation::*;
pub use pipeline::*;
pub use error::PipelineError;
