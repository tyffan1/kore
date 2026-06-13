//! Render pipeline: fetch → parse → style → layout → display list.

mod pipeline;
mod error;

pub use pipeline::*;
pub use error::PipelineError;
