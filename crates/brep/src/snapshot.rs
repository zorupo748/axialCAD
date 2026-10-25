//! Explicit versioned snapshot contract for evaluated B-Rep representations.

use serde::{Deserialize, Serialize};

use crate::error::BrepError;

/// Format version identifier for B-Rep snapshots.
pub const BREP_FORMAT_VERSION: u32 = 1;

/// Exact pinned git commit hash of the underlying geometry kernel.
pub const KERNEL_REVISION: &str = "78d699b0792b39e27c2bf073b682986701644e91";

/// A serialized snapshot of an evaluated B-Rep solid with explicit version tracking.
///
/// B-Rep data depends directly on internal mathematical structures (NURBS knot vectors,
/// topological pointers, intersection curve representations). Therefore, a snapshot
/// is strictly pinned to a specific kernel revision and serialization schema version.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BrepSnapshot {
    /// Schema format version (starts at 1).
    pub format_version: u32,
    /// Git revision hash of the kernel used to generate this snapshot.
    pub kernel_revision: String,
    /// Serialized binary or JSON payload of the underlying B-Rep.
    pub payload: Vec<u8>,
}

impl BrepSnapshot {
    /// Create a new snapshot with current version and kernel metadata.
    pub fn new(payload: Vec<u8>) -> Self {
        BrepSnapshot {
            format_version: BREP_FORMAT_VERSION,
            kernel_revision: KERNEL_REVISION.to_string(),
            payload,
        }
    }

    /// Validate that this snapshot matches the expected format version and kernel revision.
    pub fn validate_compatibility(&self) -> Result<(), BrepError> {
        if self.format_version != BREP_FORMAT_VERSION {
            return Err(BrepError::UnsupportedKernelVersion {
                expected: format!("format version {BREP_FORMAT_VERSION}"),
                actual: format!("format version {}", self.format_version),
            });
        }

        if self.kernel_revision != KERNEL_REVISION {
            return Err(BrepError::UnsupportedKernelVersion {
                expected: format!("kernel revision {KERNEL_REVISION}"),
                actual: format!("kernel revision {}", self.kernel_revision),
            });
        }

        Ok(())
    }
}
