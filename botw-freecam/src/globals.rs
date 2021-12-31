memory_rs::scoped_no_mangle! {
    g_camera_struct: usize = 0;
    g_camera_active: u8 = 0x0;

    g_get_camera_data: usize = 0x0;
    g_xinput_override: usize = 0x0;
}

extern "C" {
    pub static asm_get_camera_data: u8;
    pub static asm_override_xinput_call: u8;
}
