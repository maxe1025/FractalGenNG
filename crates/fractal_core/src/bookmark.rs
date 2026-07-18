use serde::{Deserialize, Serialize};

use crate::params::FractalParams;

/// A named, saved focus point (full render state) the user can return to or
/// use as the target of a zoom animation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bookmark {
    pub name: String,
    pub params: FractalParams,
}
