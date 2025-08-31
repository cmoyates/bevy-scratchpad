use bevy::prelude::*;

/// Native-only quit: press Esc or Q to exit the app.
/// (No-op on wasm32.)
pub fn exit_on_esc_or_q_if_native(keys: Res<ButtonInput<KeyCode>>, mut exit: EventWriter<AppExit>) {
    if cfg!(not(target_arch = "wasm32")) {
        if keys.any_just_pressed([KeyCode::Escape, KeyCode::KeyQ]) {
            exit.write(AppExit::Success);
        }
    }
}
