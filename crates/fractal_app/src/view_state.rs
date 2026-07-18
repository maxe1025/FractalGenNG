/// Transient interaction state that never gets serialized or exported — kept
/// separate from `FractalParams`, which is the pure, serializable description
/// of a render.
#[derive(Default)]
pub struct ViewState {
    pub julia_seed_picker_active: bool,
}
