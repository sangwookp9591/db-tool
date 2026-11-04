//! AxDB ETL engine (ADR-0005).
//!
//! A pipeline is `source → steps → target`. The engine reads bounded batches from an
//! [`engine::Extractor`], applies compiled [`transform::Transform`] steps and writes each batch
//! atomically through an [`engine::Loader`]. It depends only on `dbcore`, so the host decides how
//! sessions, permissions, timeouts and approvals wrap it.
pub mod engine;
pub mod spec;
pub mod transform;

pub use engine::{
    Batch, Extractor, LoadOutput, LoadRequest, Loader, Observer, PreviewReport, Progress,
    RunReport, RunStatus,
};
pub use spec::{OnError, Options, Pipeline, Source, Step, Target};
pub use transform::Transform;
