use memory_rs::internal::{injections::{Detour, Inject, Injection}, memory::resolve_module_path};
use std::ffi::CString;
use winapi::um::libloaderapi::{FreeLibraryAndExitThread, GetModuleHandleA};
use winapi::um::wincon::FreeConsole;
use winapi::um::winuser;
use winapi::um::{consoleapi::AllocConsole, winuser::GetAsyncKeyState};
use winapi::{shared::minwindef::LPVOID, um::libloaderapi::GetProcAddress};

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

fn get_camera_function() -> Result<usize, Box<dyn std::error::Error>> {
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
            "Jump table was empty, Check you're running the game and using recompiler".into(),
        );
    }

    let array = unsafe { std::slice::from_raw_parts(addr as *const usize, 0x8800000) };
    let original_bytes = [
        0x45_u8, 0x0F, 0x38, 0xF1, 0xB4, 0x05, 0xC4, 0x05, 0x00, 0x00,
    ];

    let camera_offset = loop {
        let function_start = array[0x2C053DC / 4];
        let camera_offset =
            unsafe { std::slice::from_raw_parts((function_start + 0x2E0) as *const u8, 10) };

        if &original_bytes == camera_offset {
            info!("Camera function found");
            break function_start + 0x2E0;
        }
        info!("Waiting for the game to start");
        std::thread::sleep(std::time::Duration::from_secs(1))
    };

    Ok(camera_offset)
}

fn patch(_lib: LPVOID) -> Result<(), Box<dyn std::error::Error>> {
    let base_addr = get_base_offset()?;

    println!("{:x}", base_addr);

    let mut input = Input::new();

    let mut active = false;

    let pointer = get_camera_function()?;
    println!("Camera function pointer: {:x}", pointer);

    let mut cam = unsafe {
        Detour::new(
            pointer,
            14,
            &asm_get_camera_data as *const u8 as usize,
            Some(&mut g_get_camera_data),
        )
    };

    let mut nops = vec![
        Injection::new(pointer + 0x1C8, vec![0x90; 10]),
        Injection::new(pointer + 0x4C, vec![0x90; 10]),
        Injection::new(pointer + 0x17, vec![0x90; 10]),
        Injection::new(pointer + 0x98, vec![0x90; 10]),
        Injection::new(pointer + 0x1Df, vec![0x90; 10]),
    ];

    cam.inject();

    loop {
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

    Ok(())
}

memory_rs::main_dll!(wrapper);
