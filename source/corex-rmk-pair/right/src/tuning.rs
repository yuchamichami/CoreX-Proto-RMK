use rmk::keymap::KeyMap;
use rmk::types::action::{Action, KeyAction};
pub use crate::tuning_values::Tuning;
// Keep the old X and vertical-scroll cells, now controlling both axes.
// The old Y cell now selects AML; old gain values retain AML ON.
// The old H cell remains stored but no longer affects the trackball.
// Right stays in logical rows 0..3; stock left occupies rows 4..7.
pub const CELLS: [(u8,u8);2] = [(0,6),(2,6)];
pub const AML_CELL: (u8,u8) = (1,6);
pub fn aml_enabled(keymap: &KeyMap<'_>) -> bool {
    let id = match keymap.action_at_pos(0, AML_CELL.0, AML_CELL.1) {
        KeyAction::Single(Action::User(id)) => Some(id),
        _ => None,
    };
    crate::tuning_values::aml_enabled_from_id(id)
}
pub fn read(keymap: &KeyMap<'_>) -> Tuning {
    Tuning::from_ids(CELLS.map(|(row,col)| match keymap.action_at_pos(0,row,col) {
        KeyAction::Single(Action::User(id)) => Some(id),
        _ => None,
    }))
}
