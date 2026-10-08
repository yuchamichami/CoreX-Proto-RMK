use rmk::event::{LayerChangeEvent, PointingProcessorEvent, publish_event};
use rmk::input_device::pointing::{CursorConfig, PointingMode, ScrollConfig};
use rmk::keymap::KeyMap;
use rmk::macros::processor;
#[processor(subscribe=[LayerChangeEvent], poll_interval=50)]
pub struct PointingModeController<'a> {
    keymap: &'a KeyMap<'a>,
    scrolling: bool,
    last: Option<PointingMode>,
    aml_enabled: bool,
}
impl<'a> PointingModeController<'a> {
    pub fn new(keymap: &'a KeyMap<'a>) -> Self {
        // The generated startup restores the stored keymap before this constructor.
        // Apply persisted OFF before the pointing tasks can emit motion.
        let aml_enabled = crate::tuning::aml_enabled(keymap);
        rmk::set_auto_mouse_layer_enabled(aml_enabled);
        log::info!("Vial AML {} (layer 0 setting)", if aml_enabled { "ON" } else { "OFF" });
        Self {keymap, scrolling:false, last:None, aml_enabled}
    }
    fn refresh(&mut self) {
        let aml_enabled = crate::tuning::aml_enabled(self.keymap);
        if self.aml_enabled != aml_enabled {
            self.aml_enabled = aml_enabled;
            rmk::set_auto_mouse_layer_enabled(aml_enabled);
            log::info!("Vial AML {} (layer 0 setting)", if aml_enabled { "ON" } else { "OFF" });
        }
        let t=crate::tuning::read(self.keymap);
        let mode=if self.scrolling {
            PointingMode::Scroll(ScrollConfig {
                multiplier_x:1,divisor_x:t.scroll,
                multiplier_y:1,divisor_y:t.scroll,
                invert_x:false,invert_y:false,
            })
        } else { PointingMode::Cursor(CursorConfig::default()) };
        // Only publish actual changes: repeated mode events would discard scroll fractions.
        if self.last.as_ref()!=Some(&mode) {
            self.last=Some(mode.clone());
            publish_event(PointingProcessorEvent {device_id:0,mode});
        }
    }
    async fn poll(&mut self) { self.refresh(); }
    async fn on_layer_change_event(&mut self,e:LayerChangeEvent) {
        self.scrolling=e.0==3;
        self.refresh();
    }
}
