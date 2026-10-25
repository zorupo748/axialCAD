//! Error types for CADCraft B-Rep modeling operations.

use thiserror::Error;

/// Errors arising from B-Rep modeling, boolean operations, tessellation or export.
#[derive(Debug, Error, PartialEq)]
pub enum BrepError {
    /// Input parameters or coordinates are non-finite, degenerate or invalid.
    #[error("invalid geometry: {0}")]
    InvalidGeometry(String),

    /// Boolean operation failed because surfaces do not intersect transversally.
    /// This happens when faces are coincident/coplanar or mutually tangent.
    #[error("non-transversal intersection: {0}")]
    NonTransversalIntersection(String),

    /// The numerical solver failed to converge within the allowed trial iterations.
    #[error("numerical convergence failed: {0}")]
    NumericalConvergenceFailed(String),

    /// An internal panic was caught during kernel evaluation.
    #[error("kernel evaluation failed: {0}")]
    KernelPanic(String),

    /// Serialization or deserialization of B-Rep state failed.
    #[error("serialization error: {0}")]
    SerializationError(String),

    /// The snapshot was produced by an incompatible kernel revision or format version.
    #[error("unsupported kernel revision or format version: expected {expected}, found {actual}")]
    UnsupportedKernelVersion {
        /// Expected kernel revision / version identifier.
        expected: String,
        /// Actual kernel revision / version identifier found in data.
        actual: String,
    },

    /// An I/O error occurred during export or import.
    #[error("I/O error: {0}")]
    IoError(String),

    /// Triangulation / tessellation failed for the requested tolerance.
    #[error("triangulation error: {0}")]
    TriangulationError(String),

    /// STEP AP203/214 export failed.
    #[error("STEP export error: {0}")]
    StepExportError(String),
}
