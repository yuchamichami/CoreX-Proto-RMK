use rmk::event::{Axis, EventSubscriber, LayerChangeEvent, PointingEvent, PointingProcessorEvent};
use rmk::input_device::pointing::{
    CursorConfig, PointingMode, PointingProcessor, PointingProcessorConfig,
    PointingProcessorProcessorEventEnum, ScrollConfig,
};
use rmk::keymap::KeyMap;
use rmk::macros::processor;
use rmk::processor::{PollingProcessor, Processor};
#[processor(subscribe=[LayerChangeEvent, PointingEvent])]
pub struct PointingModeController<'a> {
    keymap: &'a KeyMap<'a>,
    output: PointingProcessor<'a>,
    scrolling: bool,
    last: Option<PointingMode>,
    aml_enabled: bool,
    scale: crate::paw_wire::Scale,
    cursor_gain: i32,
}

impl PollingProcessor for PointingModeController<'_> {
    fn interval(&self) -> embassy_time::Duration {
        embassy_time::Duration::MAX
    }
    async fn update(&mut self) {
        self.refresh().await;
    }

    // RMK's registration uses this entry point; there is no periodic timer.
    async fn polling_loop(&mut self) -> ! {
        use rmk::embassy_futures::select::{Either, select};
        let mut events = Self::subscriber();
        self.refresh().await;
        loop {
            match select(rmk::keymap::wait_for_keymap_change(), events.next_event()).await {
                Either::First(()) => self.refresh().await,
                Either::Second(event) => self.process(event).await,
            }
        }
    }
}
impl<'a> PointingModeController<'a> {
    pub fn new(keymap: &'a KeyMap<'a>) -> Self {
        // The generated startup restores the stored keymap before this constructor.
        // Apply persisted OFF before the pointing tasks can emit motion.
        let aml_enabled = crate::tuning::aml_enabled(keymap);
        rmk::set_auto_mouse_layer_enabled(aml_enabled);
        log::info!(
            "Vial AML {} (layer 0 setting)",
            if aml_enabled { "ON" } else { "OFF" }
        );
        Self {
            keymap,
            output: PointingProcessor::new(
                keymap,
                PointingProcessorConfig {
                    device_id: 0,
                    ..Default::default()
                },
            ),
            scrolling: false,
            last: None,
            aml_enabled,
            scale: Default::default(),
            cursor_gain: 0,
        }
    }
    async fn refresh(&mut self) {
        // Read the current layer, including a stored default layer at startup.
        self.scrolling = self.keymap.active_layer() == 3;
        let aml_enabled = crate::tuning::aml_enabled(self.keymap);
        if self.aml_enabled != aml_enabled {
            self.aml_enabled = aml_enabled;
            rmk::set_auto_mouse_layer_enabled(aml_enabled);
            log::info!(
                "Vial AML {} (layer 0 setting)",
                if aml_enabled { "ON" } else { "OFF" }
            );
        }
        let t = crate::tuning::read(self.keymap);
        if self.cursor_gain != t.cursor {
            self.scale.reset();
            self.cursor_gain = t.cursor;
        }
        let mode = if self.scrolling {
            PointingMode::Scroll(ScrollConfig {
                multiplier_x: 1,
                divisor_x: t.scroll,
                multiplier_y: 1,
                divisor_y: t.scroll,
                invert_x: false,
                invert_y: false,
            })
        } else {
            PointingMode::Cursor(CursorConfig::default())
        };
        // Only apply actual changes: repeated mode events would discard scroll fractions.
        if self.last.as_ref() != Some(&mode) {
            self.scale.reset();
            self.last = Some(mode.clone());
            self.output
                .on_pointing_processor_event(PointingProcessorEvent { device_id: 0, mode })
                .await;
        }
    }
    async fn on_layer_change_event(&mut self, _event: LayerChangeEvent) {
        self.refresh().await;
    }
    async fn on_pointing_event(&mut self, mut event: PointingEvent) {
        if event.device_id != 0 {
            return;
        }
        // All samples arrive as raw sensor counts. Decide both mode and gain
        // here, even when layer changes and motion were queued together.
        self.refresh().await;
        if !self.scrolling {
            let raw_x = event
                .axes
                .iter()
                .find(|axis| matches!(axis.axis, Axis::X))
                .map_or(0, |axis| axis.value);
            let raw_y = event
                .axes
                .iter()
                .find(|axis| matches!(axis.axis, Axis::Y))
                .map_or(0, |axis| axis.value);
            let (x, y) =
                self.scale
                    .cursor_with_gain(raw_x, raw_y, self.cursor_gain, self.cursor_gain);
            for axis in &mut event.axes {
                match axis.axis {
                    Axis::X => axis.value = x,
                    Axis::Y => axis.value = y,
                    _ => {}
                }
            }
        }
        // A zero scaled sample still wakes RMK; its fraction is retained here
        // while PointingProcessor suppresses a zero cursor HID report.
        self.output
            .process(PointingProcessorProcessorEventEnum::Pointing(event))
            .await;
    }
}
