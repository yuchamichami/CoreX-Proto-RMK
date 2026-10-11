//! Vial User8..13 control cursor gain; User14..19 control scroll amount.
//! User0..7 stay reserved for BLE. Old User20..25 scroll values remain readable.
//! User26/27 select AML ON/OFF in the dedicated layer-0 settings cell.
pub const AML_ON: u8 = 26;
pub const AML_OFF: u8 = 27;

pub fn aml_enabled_from_id(id: Option<u8>) -> bool {
    // Older stored maps have No or a legacy Y-gain here. Preserve their AML ON.
    match id { Some(AML_OFF) => false, Some(AML_ON) => true, _ => true }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tuning {
    pub cursor: i32,
    pub scroll: u8,
}
impl Default for Tuning {
    fn default() -> Self {
        Self { cursor: 4, scroll: 60 }
    }
}
impl Tuning {
    pub fn from_ids(ids: [Option<u8>; 2]) -> Self {
        let gains = [1, 2, 3, 4, 6, 8]; // numerator / 5 = 0.5x .. 4x initial gain
        let scroll = [0, 20, 40, 60, 80, 120]; // 0 disables trackball scrolling
        let pick = |id: Option<u8>, first: u8| -> Option<usize> {
            id.filter(|n| (*n >= first) && (*n < first + 6)).map(|n| (n-first) as usize)
        };
        let d = Self::default();
        Self {
            cursor: pick(ids[0], 8).map(|i| gains[i]).unwrap_or(d.cursor),
            scroll: pick(ids[1], 14).or_else(|| pick(ids[1], 20))
                .map(|i| scroll[i]).unwrap_or(d.scroll),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn aml_upgrade_preserves_enabled_and_explicit_off_is_independent() {
        for id in [None, Some(AML_ON), Some(0), Some(11), Some(20), Some(255)] {
            assert!(aml_enabled_from_id(id));
        }
        assert!(!aml_enabled_from_id(Some(AML_OFF)));
        assert_eq!(Tuning::from_ids([Some(12), Some(17)]), Tuning { cursor: 6, scroll: 60 });
    }
    #[test] fn defaults_preserve_existing_cursor_and_scroll_speed() {
        assert_eq!(Tuning::from_ids([Some(11),Some(17)]), Tuning::default());
        assert_eq!(Tuning::from_ids([Some(11),Some(23)]), Tuning::default());
    }
    #[test] fn invalid_or_old_keymap_values_have_safe_fallbacks() {
        assert_eq!(Tuning::from_ids([None,Some(0)]), Tuning::default());
        assert_eq!(Tuning::from_ids([Some(255),Some(11)]), Tuning::default());
        assert_eq!(Tuning::from_ids([Some(17),Some(29)]), Tuning::default());
    }
    #[test] fn all_legacy_vertical_scroll_values_keep_their_amount() {
        for offset in 0..6 {
            assert_eq!(Tuning::from_ids([Some(11),Some(20+offset)]),
                       Tuning::from_ids([Some(11),Some(14+offset)]));
        }
    }
    #[test] fn cursor_and_scroll_can_be_changed_independently() {
        for (offset, cursor) in [1,2,3,4,6,8].into_iter().enumerate() {
            assert_eq!(Tuning::from_ids([Some(8+offset as u8),Some(17)]), Tuning {cursor,scroll:60});
        }
        for (offset, scroll) in [0,20,40,60,80,120].into_iter().enumerate() {
            assert_eq!(Tuning::from_ids([Some(11),Some(14+offset as u8)]), Tuning {cursor:4,scroll});
        }
    }
}
