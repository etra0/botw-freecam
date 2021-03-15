use memory_rs::internal::{
    injections::{Detour, Inject, Injection},
    memory::{resolve_module_path, scan_aob},
    process_info::ProcessInfo,
};
use nalgebra_glm as glm;
use std::ffi::CString;
use winapi::um::consoleapi::AllocConsole;
use winapi::um::libloaderapi::{FreeLibraryAndExitThread, GetModuleHandleA};
use winapi::um::wincon::FreeConsole;
use winapi::um::winuser;
use winapi::um::xinput;
use winapi::{shared::minwindef::LPVOID, um::libloaderapi::GetProcAddress};

use log::*;
use simplelog::*;

mod camera;
mod dolly;
mod globals;
mod utils;

use camera::*;
use dolly::*;
use globals::*;
use utils::{check_key_press, dummy_xinput, error_message, handle_keyboard, Input, Keys};

use std::io::{self, Write};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

fn write_red(msg: &str) -> io::Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
    writeln!(&mut stdout, "{}", msg)?;
    stdout.reset()
}

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

#[derive(Debug)]
struct CameraOffsets {
    camera: usize,
    rotation_vec1: usize,
    rotation_vec2: usize,
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
            "Jump table was empty, Check you're running the game and using recompiler profile"
                .into(),
        );
    }

    let array = unsafe { std::slice::from_raw_parts(addr as *const usize, 0x8800000 / 0x8) };
    let original_bytes = [
        0x45_u8, 0x0F, 0x38, 0xF1, 0xB4, 0x05, 0xC4, 0x05, 0x00, 0x00,
    ];

    // As Exzap said, "It will only compile it once its executed. Before that the table points to a placeholder function"
    // So we'll wait until the game is in the world and the code will be recompiled, then the pointer should be changed to the right function.
    // Once is resolved, we can lookup the rest of the functions since the camera we assume the camera is active
    let dummy_pointer = array[0];
    info!("Waiting for the game to start");
    let camera_offset = loop {
        let function_start = array[0x2C05484 / 4];

        if dummy_pointer != function_start {
            info!("Pointer found");
            break function_start + 0x6C;
        }
        std::thread::sleep(std::time::Duration::from_secs(1))
    };

    let camera_bytes = unsafe { std::slice::from_raw_parts((camera_offset) as *const u8, 10) };
    if camera_bytes != original_bytes {
        return Err(format!(
            "Function signature doesn't match, This can mean two things:\n\n\
            * You're using a pre 2016 CPU (your cpu doesn't support `movbe`)\n\
            * You're not using the version described on the README.md\n\
            {:x?} != {:x?}",
            camera_bytes, original_bytes
        )
        .into());
    }

    let rotation_vec1 = array[0x2C085FC / 4] + 0x157;
    let rotation_vec2 = array[0x2e57fdc / 4] + 0x7f;

    Ok(CameraOffsets {
        camera: camera_offset,
        rotation_vec1,
        rotation_vec2,
    })
}

fn block_xinput(proc_inf: &ProcessInfo) -> Result<Injection, Box<dyn std::error::Error>> {
    // Find input blocker for xinput only
    let pat = memory_rs::generate_aob_pattern![
        0x41, 0xFF, 0xD0, 0x85, 0xC0, 0x74, 0x16, 0x33, 0xC9, 0xC6, 0x87, 0x58, 0x01, 0x00, 0x00,
        0x00, 0x8B, 0xC1, 0x87, 0x47, 0x14, 0x86, 0x4F, 0x18
    ];

    let function_addr = scan_aob(
        proc_inf.region.start_address,
        proc_inf.region.size,
        pat.1,
        pat.0,
    )?
    .ok_or("XInput blocker couldn't be found")?;

    let rip = function_addr - 10;
    let offset = unsafe { *((rip + 3) as *const u32) as usize + rip + 7 };

    let my_function: [u8; 8] = unsafe { std::mem::transmute(dummy_xinput as *const u8 as usize) };
    let injection = Injection::new(offset, Vec::from(my_function));

    Ok(injection)
}

fn patch(_lib: LPVOID) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        "Breath of the Wild freecam by @etra0, v{}",
        utils::get_version()
    );
    write_red("If you close this window the game will close. Use HOME to deattach the freecamera (will close this window as well).")?;
    println!("{}", utils::INSTRUCTIONS);
    write_red("Controller input will only be detected if Xinput is used in the Control settings, otherwise use the keyboard.")?;
    let proc_inf = ProcessInfo::new(None)?;

    let mut input = Input::new();

    let mut active = false;

    let mut points: Vec<CameraSnapshot> = vec![];

    // This variable will hold the initial position when the freecamera is activated.
    let mut starting_point: Option<CameraSnapshot> = None;

    let camera_struct = get_camera_function()?;
    info!("{:x?}", camera_struct);
    let camera_pointer = camera_struct.camera;
    info!("Camera function camera_pointer: {:x}", camera_pointer);

    block_xinput(&proc_inf)?;

    let mut cam = unsafe {
        Detour::new(
            camera_pointer,
            14,
            &asm_get_camera_data as *const u8 as usize,
            Some(&mut g_get_camera_data),
        )
    };

    let mut nops = vec![
        // Camera pos and focus writers
        Injection::new(camera_struct.camera + 0x1C8, vec![0x90; 10]),
        Injection::new(camera_struct.camera + 0x4C, vec![0x90; 10]),
        Injection::new(camera_struct.camera + 0x17, vec![0x90; 10]),
        Injection::new(camera_struct.camera + 0x98, vec![0x90; 10]),
        Injection::new(camera_struct.camera + 0x1DF, vec![0x90; 10]),
        // Fov
        Injection::new(camera_struct.camera + 0xAF, vec![0x90; 10]),
        // Rotation
        Injection::new(camera_struct.rotation_vec1, vec![0x90; 10]),
        Injection::new(camera_struct.rotation_vec1 + 0x3E, vec![0x90; 10]),
        Injection::new(camera_struct.rotation_vec1 + 0x9B, vec![0x90; 10]),
        Injection::new(camera_struct.rotation_vec2, vec![0x90; 7]),
        Injection::new(camera_struct.rotation_vec2 - 0x14, vec![0x90; 7]),
        Injection::new(camera_struct.rotation_vec2 - 0x28, vec![0x90; 7]),
        block_xinput(&proc_inf)?,
    ];

    cam.inject();

    let xinput_func =
        |a: u32, b: &mut xinput::XINPUT_STATE| -> u32 { unsafe { xinput::XInputGetState(a, b) } };

    loop {
        utils::handle_controller(&mut input, xinput_func);
        handle_keyboard(&mut input);
        input.sanitize();

        if input.deattach || check_key_press(winuser::VK_HOME) {
            info!("Exiting");
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
                starting_point = None;
            }

            input.change_active = false;
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        unsafe {
            // If we don't have the camera struct we need to skip it right away
            if g_camera_struct == 0x0 {
                continue;
            }

            let gc = g_camera_struct as *mut GameCamera;
            if !active {
                input.fov = (*gc).fov.to_fbe();
                continue;
            }

            if starting_point.is_none() {
                starting_point = Some(CameraSnapshot::new(&(*gc)));
            }

            if let Some(ref p) = starting_point {
                let cp = (*gc).pos.clone().into();
                let cf: glm::Vec3 = (*gc).focus.clone().into();
                let delta_view = cf - cp;
                let distance = utils::calc_eucl_distance(&p.pos, &cp);
                if distance > 400. {
                    let norm = glm::normalize(&(cp - p.pos));

                    (*gc).pos = (p.pos + norm * 380.).into();
                    (*gc).focus = (p.pos + norm * 380. + delta_view).into();
                }
            }

            // TODO: Maybe remove this
            // if check_key_press(winuser::VK_F7) {
            //     nops.last_mut().unwrap().remove_injection();
            // }

            if !points.is_empty() {
                let origin = (*gc).pos.clone().into();
                if utils::calc_eucl_distance(&origin, &points[0].pos) > 400. {
                    warn!("Sequence cleaned to prevent game crashing");
                    points.clear();
                }
            }

            if check_key_press(winuser::VK_F9) {
                let cs = CameraSnapshot::new(&(*gc));
                info!("Point added to interpolation: {:?}", cs);
                points.push(cs);
                std::thread::sleep(std::time::Duration::from_millis(400));
            }

            if check_key_press(winuser::VK_F11) {
                info!("Sequence cleaned!");
                points.clear();
                std::thread::sleep(std::time::Duration::from_millis(400));
            }

            if check_key_press(winuser::VK_F10) & (points.len() > 1) {
                let dur = std::time::Duration::from_secs_f32(input.dolly_duration);
                points.interpolate(&mut (*gc), dur, false);
                std::thread::sleep(std::time::Duration::from_millis(500));
            }

            if check_key_press(Keys::L as _) & (points.len() > 1) {
                let dur = std::time::Duration::from_secs_f32(input.dolly_duration);
                points.interpolate(&mut (*gc), dur, true);
                std::thread::sleep(std::time::Duration::from_millis(500));
            }

            if check_key_press(winuser::VK_F7) {
                input.unlock_character = !input.unlock_character;
                if input.unlock_character {
                    nops.last_mut().unwrap().remove_injection();
                } else {
                    nops.last_mut().unwrap().inject();
                }
                info!("Unlock character: {}", input.unlock_character);
                std::thread::sleep(std::time::Duration::from_millis(500));
            }

            if input.unlock_character {
                continue;
            };

            (*gc).consume_input(&input);
            // println!("{:?}", *gc);
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

    // Block input:
    // Cemu.exe + 0x3c63e1

    Ok(())
}

memory_rs::main_dll!(wrapper);
