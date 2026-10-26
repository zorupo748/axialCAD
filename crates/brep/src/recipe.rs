//! Parametric recipe / construction history contract.
//!
//! Separates the editable parametric intent (the recipe) from the evaluated
//! B-Rep solid and derived tessellations.

use crate::error::BrepError;
use crate::solid::{BrepSolid, SubtractOptions};
use std::collections::HashMap;

pub use cadcraft_doc::solid3d::{BooleanKind, FeatureNode, FeatureOp, Recipe, SOLID3D_SCHEMA_VERSION, Solid3d, Solid3dStatus};

/// Trait to evaluate a parametric recipe into a concrete `BrepSolid`.
pub trait EvaluateRecipe {
    /// Evaluate the parametric recipe from scratch into an evaluated 3D B-Rep solid.
    fn evaluate(&self) -> Result<BrepSolid, BrepError>;
}

impl EvaluateRecipe for Recipe {
    fn evaluate(&self) -> Result<BrepSolid, BrepError> {
        let mut solids: HashMap<u64, BrepSolid> = HashMap::new();
        let mut last_id = None;

        for feature in &self.features {
            let solid = match &feature.op {
                FeatureOp::Box { origin, size } => BrepSolid::box_dimensions(*origin, *size)?,
                FeatureOp::Cylinder { center, radius, height, axis } => BrepSolid::cylinder(*center, *radius, *height, *axis)?,
                FeatureOp::Extrusion { profile, direction } => BrepSolid::extrude(profile, *direction)?,
                FeatureOp::Boolean { kind, target_id, tool_id, .. } => {
                    let target = solids
                        .get(target_id)
                        .ok_or_else(|| BrepError::InvalidGeometry(format!("boolean target feature id {target_id} not found")))?;
                    let tool =
                        solids.get(tool_id).ok_or_else(|| BrepError::InvalidGeometry(format!("boolean tool feature id {tool_id} not found")))?;
                    match kind {
                        BooleanKind::Union => target.union(tool)?,
                        BooleanKind::Difference => target.subtract(tool, SubtractOptions::default())?,
                        BooleanKind::Intersection => target.intersect(tool)?,
                    }
                }
                FeatureOp::Translation { target_id, delta } => {
                    let target = solids
                        .get(target_id)
                        .ok_or_else(|| BrepError::InvalidGeometry(format!("translation target feature id {target_id} not found")))?;
                    target.translate(*delta)?
                }
            };
            solids.insert(feature.id, solid);
            last_id = Some(feature.id);
        }

        match last_id.and_then(|id| solids.remove(&id)) {
            Some(solid) => Ok(solid),
            None => Err(BrepError::InvalidGeometry("empty recipe cannot be evaluated".into())),
        }
    }
}

impl BrepSolid {
    /// Evaluate a parametric recipe from scratch into an evaluated 3D B-Rep solid.
    pub fn evaluate_recipe(recipe: &Recipe) -> Result<BrepSolid, BrepError> {
        recipe.evaluate()
    }

    /// Evaluate a 3D solid entity into an evaluated 3D B-Rep solid.
    pub fn evaluate_solid(solid: &Solid3d) -> Result<BrepSolid, BrepError> {
        solid.recipe.evaluate()
    }
}
