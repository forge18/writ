use writ_vm::{VM, Value};

pub fn register(vm: &mut VM) {
    // ── Keyboard keys ─────────────────────────────────────────────────
    let keys = [
        // Letters
        ("Key_A", 0),
        ("Key_B", 1),
        ("Key_C", 2),
        ("Key_D", 3),
        ("Key_E", 4),
        ("Key_F", 5),
        ("Key_G", 6),
        ("Key_H", 7),
        ("Key_I", 8),
        ("Key_J", 9),
        ("Key_K", 10),
        ("Key_L", 11),
        ("Key_M", 12),
        ("Key_N", 13),
        ("Key_O", 14),
        ("Key_P", 15),
        ("Key_Q", 16),
        ("Key_R", 17),
        ("Key_S", 18),
        ("Key_T", 19),
        ("Key_U", 20),
        ("Key_V", 21),
        ("Key_W", 22),
        ("Key_X", 23),
        ("Key_Y", 24),
        ("Key_Z", 25),
        // Numbers
        ("Key_Num0", 26),
        ("Key_Num1", 27),
        ("Key_Num2", 28),
        ("Key_Num3", 29),
        ("Key_Num4", 30),
        ("Key_Num5", 31),
        ("Key_Num6", 32),
        ("Key_Num7", 33),
        ("Key_Num8", 34),
        ("Key_Num9", 35),
        // Function keys
        ("Key_F1", 36),
        ("Key_F2", 37),
        ("Key_F3", 38),
        ("Key_F4", 39),
        ("Key_F5", 40),
        ("Key_F6", 41),
        ("Key_F7", 42),
        ("Key_F8", 43),
        ("Key_F9", 44),
        ("Key_F10", 45),
        ("Key_F11", 46),
        ("Key_F12", 47),
        // Special keys
        ("Key_Space", 48),
        ("Key_Enter", 49),
        ("Key_Escape", 50),
        ("Key_Tab", 51),
        ("Key_Backspace", 52),
        ("Key_Delete", 53),
        ("Key_Insert", 54),
        ("Key_Home", 55),
        ("Key_End", 56),
        ("Key_PageUp", 57),
        ("Key_PageDown", 58),
        // Arrow keys
        ("Key_Up", 59),
        ("Key_Down", 60),
        ("Key_Left", 61),
        ("Key_Right", 62),
        // Modifiers
        ("Key_LeftShift", 63),
        ("Key_RightShift", 64),
        ("Key_LeftCtrl", 65),
        ("Key_RightCtrl", 66),
        ("Key_LeftAlt", 67),
        ("Key_RightAlt", 68),
    ];

    for (name, value) in &keys {
        vm.register_global(name, Value::I64(*value));
    }

    // ── Mouse buttons ─────────────────────────────────────────────────
    let mouse_buttons = [
        ("MouseButton_Left", 0),
        ("MouseButton_Right", 1),
        ("MouseButton_Middle", 2),
        ("MouseButton_Back", 3),
        ("MouseButton_Forward", 4),
    ];

    for (name, value) in &mouse_buttons {
        vm.register_global(name, Value::I64(*value));
    }

    // ── Controller buttons ────────────────────────────────────────────
    let controller_buttons = [
        ("ControllerButton_A", 0),
        ("ControllerButton_B", 1),
        ("ControllerButton_X", 2),
        ("ControllerButton_Y", 3),
        ("ControllerButton_DPadUp", 4),
        ("ControllerButton_DPadDown", 5),
        ("ControllerButton_DPadLeft", 6),
        ("ControllerButton_DPadRight", 7),
        ("ControllerButton_LeftBumper", 8),
        ("ControllerButton_RightBumper", 9),
        ("ControllerButton_LeftStick", 10),
        ("ControllerButton_RightStick", 11),
        ("ControllerButton_Start", 12),
        ("ControllerButton_Back", 13),
        ("ControllerButton_Guide", 14),
    ];

    for (name, value) in &controller_buttons {
        vm.register_global(name, Value::I64(*value));
    }

    // ── Controller axes ───────────────────────────────────────────────
    let controller_axes = [
        ("ControllerAxis_LeftStickX", 0),
        ("ControllerAxis_LeftStickY", 1),
        ("ControllerAxis_RightStickX", 2),
        ("ControllerAxis_RightStickY", 3),
        ("ControllerAxis_LeftTrigger", 4),
        ("ControllerAxis_RightTrigger", 5),
    ];

    for (name, value) in &controller_axes {
        vm.register_global(name, Value::I64(*value));
    }
}
