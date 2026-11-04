//! AxDB ETL engine (ADR-0005).
//!
//! A pipeline is `source → steps → target`. The engine depends only on `dbcore`, so the host
//! decides how sessions, permissions, timeouts and approvals wrap it.
pub mod spec;
pub mod transform;

pub use spec::{OnError, Options, Pipeline, Source, Step, Target};
pub use transform::Transform;
