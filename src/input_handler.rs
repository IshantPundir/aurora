use crate::{state::Backend, AuroraState};

use smithay::{
    backend::input::{
        Event, InputBackend, InputEvent, KeyboardKeyEvent,
    },
    input::keyboard::FilterResult,

    utils::SERIAL_COUNTER,
};


impl <BackendData: Backend> AuroraState<BackendData> {
    fn keyboard_key_to_action<B: InputBackend>(&mut self, evt: B::KeyboardKeyEvent) {
        let keycode = evt.key_code();
        let state = evt.state();
        tracing::debug!(?keycode, ?state, "key");

        let serial = SERIAL_COUNTER.next_serial();
        let time = Event::time_msec(&evt);

        let keyboard = self.seat.get_keyboard().unwrap();
        keyboard.input(self, keycode, state, serial, time, |_, _modifiers, _handle| {
            FilterResult::Forward
        }).unwrap_or(KeyAction::None);
    }

    pub fn process_input_event_windowed<B: InputBackend>(&mut self, event: InputEvent<B>, _output_name: &str) {
        match event {
            InputEvent::PointerButton { event } => {
                // TODO: Implement this event.
            },

            InputEvent::Keyboard { event } => {
                // Add keyboard focus to active window
                let keyboard = self.seat.get_keyboard().unwrap();

                if !self.window_manager.is_empty() {
                    let active_window = self.window_manager.get_active_window().unwrap().clone();
                    keyboard.set_focus(self, Some(active_window.into()), SERIAL_COUNTER.next_serial());
                    self.keyboard_key_to_action::<B>(event)
                };
            },

            _ => (),
        }
    }
}

#[allow(dead_code)] // some of these are only read if udev is enabled
#[derive(Debug)]
enum KeyAction {
    /// Quit the compositor
    Quit,
    /// Trigger a vt-switch
    VtSwitch(i32),
    /// run a command
    Run(String),
    /// Switch the current screen
    Screen(usize),
    ScaleUp,
    ScaleDown,
    TogglePreview,
    RotateOutput,
    ToggleTint,
    ToggleDecorations,
    /// Do nothing more
    None,
}