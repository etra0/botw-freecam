use memory_rs::internal::{injections::{Detour, Inject, Injection}, memory::resolve_module_path};
use std::ffi::CString;
use winapi::um::libloaderapi::{FreeLibraryAndExitThread, GetModuleHandleA};
use winapi::um::wincon::FreeConsole;
use winapi::um::winuser;
use winapi::um::{consoleapi::AllocConsole, winuser::GetAsyncKeyState};
use winapi::{shared::minwindef::LPVOID, um::libloaderapi::GetProcAddress};
use winapi::um::xinput;

use log::*;
use simplelog::*;

mod camera;
mod globals;
mod utils;

use camera::*;
use globals::*;
use utils::{error_message, handle_keyboard, Input};

unsafe extern "system" fn wrapper(lib: LPVOID) -> u32 {
    AllocConsole();
    {
        let mut path = resolve_module_path(lib).unwrap();
        path.push("botw.log");
        CombinedLogger::init(vec![
            TermLogger::new(
                log::LevelFilter::Info,
                Config::default(),
                TerminalMode::Mixed,
            ),
            WriteLogger::new(
                log::LevelFilter::Info,
                Config::default(),
                std::fs::File::create(path).unwrap(),
            ),
        ])
        .unwrap();

        match patch(lib) {
            Ok(_) => (),
            Err(e) => {
                let msg = format!("Something went wrong:\n{}", e);
                error!("{}", msg);
                error_message(&msg);
            }
        }
    }

    FreeConsole();
    FreeLibraryAndExitThread(lib as _, 0);
    0
}

fn get_base_offset() -> Result<usize, Box<dyn std::error::Error>> {
    let function_name = CString::new("memory_getBase")?;

    // Get the handle of the process.
    let proc_handle = unsafe { GetModuleHandleA(std::ptr::null_mut()) };

    let func = unsafe { GetProcAddress(proc_handle, function_name.as_ptr()) };
    let func: extern "C" fn() -> usize = unsafe { std::mem::transmute(func) };
    let addr = (func)();

    Ok(addr)
}

#[derive(Debug)]
struct CameraOffsets {
    camera: usize,
    rotation_vec1: usize,
    rotation_vec2: usize
}

fn get_camera_function() -> Result<CameraOffsets, Box<dyn std::error::Error>> {
    let function_name = CString::new("PPCRecompiler_getJumpTableBase").unwrap();
    let proc_handle = unsafe { GetModuleHandleA(std::ptr::null_mut()) };
    let func = unsafe { GetProcAddress(proc_handle, function_name.as_ptr()) };

    if (func as usize) == 0x0 {
        return Err("Func returned was empty".into());
    }
    let func: extern "C" fn() -> usize = unsafe { std::mem::transmute(func) };

    let addr = (func)();

    if addr == 0x0 {
        return Err(
            "Jump table was empty, Check you're running the game and using recompiler profile".into(),
        );
    }

    let array = unsafe { std::slice::from_raw_parts(addr as *const usize, 0x8800000/0x8) };
    let original_bytes = [
        0x45_u8, 0x0F, 0x38, 0xF1, 0xB4, 0x05, 0xC4, 0x05, 0x00, 0x00,
    ];

    info!("Waiting for the game to start");
    let camera_offset = loop {
        let function_start = array[0x2C053DC / 4];
        let camera_offset =
            unsafe { std::slice::from_raw_parts((function_start + 0x2E0) as *const u8, 10) };

        if &original_bytes == camera_offset {
            info!("Camera function found");
            break function_start + 0x2E0;
        }
        std::thread::sleep(std::time::Duration::from_secs(1))
    };

    let rotation_vec1 = array[0x2c085f0 / 4] + 0x192;
    let rotation_vec2 = array[0x2e57fdc / 4] + 0x7f;

    Ok(CameraOffsets {camera: camera_offset, rotation_vec1, rotation_vec2})
}

fn patch(_lib: LPVOID) -> Result<(), Box<dyn std::error::Error>> {
    let base_addr = get_base_offset()?;

    println!("{:x}", base_addr);

    let mut input = Input::new();

    let mut active = false;

    let camera_struct = get_camera_function()?;
    println!("{:x?}", camera_struct);
    let camera_pointer = camera_struct.camera;
    info!("Camera function camera_pointer: {:x}", camera_pointer);

    let mut cam = unsafe {
        Detour::new(
            camera_pointer,
            14,
            &asm_get_camera_data as *const u8 as usize,
            Some(&mut g_get_camera_data),
        )
    };

    let mut nops = vec![
        Injection::new(camera_struct.camera + 0x1C8, vec![0x90; 10]),
        Injection::new(camera_struct.camera + 0x4C, vec![0x90; 10]),
        Injection::new(camera_struct.camera + 0x17, vec![0x90; 10]),
        Injection::new(camera_struct.camera + 0x98, vec![0x90; 10]),
        Injection::new(camera_struct.camera + 0x1Df, vec![0x90; 10]),

        // Fov
        Injection::new(camera_struct.camera + 0xAF, vec![0x90; 10]),

        // Rotation
        Injection::new(camera_struct.rotation_vec1, vec![0x90; 10]),
        Injection::new(camera_struct.rotation_vec1 + 0x3E, vec![0x90; 10]),
        Injection::new(camera_struct.rotation_vec1 + 0x9B, vec![0x90; 10]),

        Injection::new(camera_struct.rotation_vec2, vec![0x90; 7]),
        Injection::new(camera_struct.rotation_vec2 - 0x14, vec![0x90; 7]),
        Injection::new(camera_struct.rotation_vec2 - 0x28, vec![0x90; 7]),
    ];

    cam.inject();

    let xinput_func = |a: u32, b: &mut xinput::XINPUT_STATE| -> u32 {
        unsafe { xinput::XInputGetState(a, b) }
    };

    loop {
        utils::handle_controller(&mut input, xinput_func);
        handle_keyboard(&mut input);
        input.sanitize();

        if input.deattach || (unsafe { GetAsyncKeyState(winuser::VK_HOME) } as u32 & 0x8000) != 0 {
            println!("Exiting");
            break;
        }

        input.is_active = active;
        if input.change_active {
            active = !active;

            unsafe {
                g_camera_active = active as u8;
            }
            info!("Camera is {}", active);

            if active {
                nops.inject();
            } else {
                nops.remove_injection();
            }

            input.change_active = false;
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        unsafe {
            if !active || g_camera_struct == 0x0 {
                continue;
            }

            // let gc = (base_addr + 0x44E58260) as *mut GameCamera;
            let gc = g_camera_struct as *mut GameCamera;
            (*gc).consume_input(&input);
            println!("{:?}", *gc);
        }

        input.reset();

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // base camera: 0x44E581F0
    // base camera 2: 0x44E58260
    // distance 1: 0xd3690e8
    // camera writer
    // code: 0x30A66C4
    // 0x2c054fc (+0xc054fc)
    // offset to function start: 0x120
    // offset in RPX: 0x02aed470
    // Possible offset: 2E0

    // up vector 1:
    // 0x02c08648

    // up vector 2:
    // 0x02e57ff4

    Ok(())
}

memory_rs::main_dll!(wrapper);
