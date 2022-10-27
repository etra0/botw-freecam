use memory_rs::internal::{
    injections::{Detour, Inject, Injection},
    memory::resolve_module_path,
    process_info::ProcessInfo,
};
use std::ffi::c_void;
use windows_sys::Win32::{
    System::{
        Console::{AllocConsole, FreeConsole},
        LibraryLoader::{FreeLibraryAndExitThread, GetModuleHandleA, GetProcAddress},
    },
    UI::{Input::{
        KeyboardAndMouse::*,
        XboxController::{XInputGetState, XINPUT_STATE},
    }, WindowsAndMessaging::MessageBoxA},
};

use log::*;
use simplelog::*;

mod camera;
mod dolly;
mod globals;
mod utils;

use camera::*;
use dolly::*;
use globals::*;
use utils::{check_key_press, error_message, handle_keyboard, Input, Keys};

use std::io::{self, Write};
use std::mem::MaybeUninit;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

fn write_red(msg: &str) -> io::Result<()> {
    let mut stdout = StandardStream::stdout(ColorChoice::Always);
    stdout.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
    writeln!(&mut stdout, "{}", msg)?;
    stdout.reset()
}

unsafe extern "system" fn wrapper(lib: *mut c_void) -> u32 {
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
}

#[derive(Debug)]
struct CameraOffsets {
    camera: usize,
    rotation_vec1: usize,
}

fn get_camera_function() -> Result<CameraOffsets, Box<dyn std::error::Error>> {
    let function_name = String::from("PPCRecompiler_getJumpTableBase\0");
    let proc_handle = unsafe { GetModuleHandleA(std::ptr::null_mut()) };
    let func = unsafe {
        GetProcAddress(proc_handle, function_name.as_ptr() as _)
            .ok_or("Func return was empty".to_string())?
    };

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
        0x45_u8, 0x0F, 0x38, 0xF1, 0xB4, 0x15, 0x54, 0x06, 0x00, 0x00,
    ];

    // As Exzap said, "It will only compile it once its executed. Before that the table points to a placeholder function"
    // So we'll wait until the game is in the world and the code will be recompiled, then the pointer should be changed to the right function.
    // Once is resolved, we can lookup the rest of the functions since the camera we assume the camera is active
    let dummy_pointer = array[0];
    info!("Waiting for the game to start");
    let camera_offset = loop {
        let function_start = array[0x2C085FC / 4];

        if dummy_pointer != function_start {
            info!("Pointer found");
            break function_start + 0x7E;
        }
        std::thread::sleep(std::time::Duration::from_secs(1))
    };

    let camera_bytes = unsafe { std::slice::from_raw_parts((camera_offset) as *const u8, 10) };
    if camera_bytes != original_bytes {
        return Err(format!(
            "Function signature doesn't match, This can mean two things:\n\n\
            * You're using a cheat that requires a 'master cheat' to be activated (which\
              effectively removes `movbe`)
            * You're using a pre 2016 CPU (your cpu doesn't support `movbe`)\n\
            * You're not using the version described on the README.md\n\
            {:x?} != {:x?}",
            camera_bytes, original_bytes
        )
        .into());
    }

    let rotation_vec1 = array[0x2e57fdc / 4] + 0x57;

    Ok(CameraOffsets {
        camera: camera_offset,
        rotation_vec1,
    })
}

fn block_xinput(proc_inf: &ProcessInfo) -> Result<Detour, Box<dyn std::error::Error>> {
    // Find input blocker for xinput only

    let function_addr = proc_inf
        .region
        .scan_aob(&memory_rs::generate_aob_pattern![
            0x48, 0x8B, 0x40, 0x28, 0x48, 0x8D, 0x55, 0xE7, 0x8B, 0x8F, 0x50, 0x01, 0x00, 0x00
        ])?
        .ok_or("XInput blocker couldn't be found")?;

    // HACK: read interceptor.asm
    let injection = unsafe {
        Detour::new(
            function_addr,
            14,
            &asm_override_xinput_call as *const _ as usize,
            Some(&mut g_xinput_override),
        )
    };

    println!("{:x?}", unsafe {
        &asm_override_xinput_call as *const _ as usize
    });

    Ok(injection)
}

fn patch(_lib: *mut c_void) -> Result<(), Box<dyn std::error::Error>> {
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
    let mut control_override = false;

    let mut points: Vec<CameraSnapshot> = vec![];

    // This variable will hold the initial position when the freecamera is activated.
    let mut starting_point: Option<CameraSnapshot> = None;

    let camera_struct = get_camera_function()?;
    info!("{:x?}", camera_struct);
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

    let mut nops: Vec<Box<dyn Inject>> = vec![
        // Camera pos and focus writers
        Box::new(Injection::new(camera_struct.camera + 0x17, vec![0x90; 10])),
        Box::new(Injection::new(camera_struct.camera + 0x55, vec![0x90; 10])),
        Box::new(Injection::new(camera_struct.camera + 0xC2, vec![0x90; 10])),
        Box::new(Injection::new(camera_struct.camera + 0xD9, vec![0x90; 10])),
        Box::new(Injection::new(camera_struct.camera + 0x117, vec![0x90; 10])),
        Box::new(Injection::new(camera_struct.camera + 0x12E, vec![0x90; 10])),
        Box::new(Injection::new(camera_struct.camera + 0x15D, vec![0x90; 10])),
        Box::new(Injection::new(camera_struct.camera + 0x174, vec![0x90; 10])),
        Box::new(Injection::new(camera_struct.camera + 0x22A, vec![0x90; 10])),
        Box::new(Injection::new(camera_struct.camera + 0x22A, vec![0x90; 10])),
        // Rotation
        Box::new(Injection::new(camera_struct.rotation_vec1, vec![0x90; 7])),
        Box::new(Injection::new(
            camera_struct.rotation_vec1 + 0x14,
            vec![0x90; 7],
        )),
        Box::new(Injection::new(
            camera_struct.rotation_vec1 + 0x28,
            vec![0x90; 7],
        )),
    ];

    if let Ok(injection) = block_xinput(&proc_inf) {
        control_override = true;
        nops.push(Box::new(injection));
    } else {

        let title = "Error while patching\0";
        let msg = "XInput blocker couldn't be found (this could means you're using a maybe unsupported Cemu version). This means your controller won't be usable as input.\
                   Check the repository to see current Cemu supported version.\0";
        unsafe {
            MessageBoxA(0, msg.as_ptr(), title.as_ptr(), 0x30);
        }
    }

    cam.inject();

    let mut game_camera_pointer = MaybeUninit::uninit();

    let xinput_func = |a: u32, b: &mut XINPUT_STATE| -> u32 { unsafe { XInputGetState(a, b) } };

    loop {
        if control_override {
            utils::handle_controller(&mut input, xinput_func);
        }
        handle_keyboard(&mut input);
        input.sanitize();

        if input.deattach || check_key_press(VK_HOME) {
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
                input.reset();
                nops.iter_mut().inject();
            } else {
                nops.iter_mut().remove_injection();
                starting_point = None;
                input.unlock_character = false;
            }

            input.change_active = false;
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        unsafe {
            // If we don't have the camera struct we need to skip it right away
            if g_camera_struct == 0x0 {
                continue;
            }

            let gc = game_camera_pointer.write(&mut *(g_camera_struct as *mut GameCamera));
            if !active {
                input.fov = gc.fov.into();
                input.delta_rotation = 0.;
                continue;
            }

            if starting_point.is_none() {
                starting_point = Some(CameraSnapshot::new(gc));
            }

            if !points.is_empty() {
                let origin = gc.pos.into();
                if utils::calc_eucl_distance(&origin, &points[0].pos) > 400. {
                    warn!("Sequence cleaned to prevent game crashing");
                    points.clear();
                }
            }

            if check_key_press(VK_F9) {
                let cs = CameraSnapshot::new(gc);
                info!("Point added to interpolation: {:?}", cs);
                points.push(cs);
                std::thread::sleep(std::time::Duration::from_millis(400));
            }

            if check_key_press(VK_F11) {
                info!("Sequence cleaned!");
                points.clear();
                std::thread::sleep(std::time::Duration::from_millis(400));
            }

            if check_key_press(VK_F10) & (points.len() > 1) {
                let dur = std::time::Duration::from_secs_f32(input.dolly_duration);
                points.interpolate(gc, dur, false);
                std::thread::sleep(std::time::Duration::from_millis(500));
            }

            if check_key_press(Keys::L as _) & (points.len() > 1) {
                let dur = std::time::Duration::from_secs_f32(input.dolly_duration);
                points.interpolate(gc, dur, true);
                std::thread::sleep(std::time::Duration::from_millis(500));
            }

            if check_key_press(VK_F7) {
                input.unlock_character = !input.unlock_character;
                if input.unlock_character {
                    nops.last_mut().unwrap().remove_injection();
                } else {
                    nops.last_mut().unwrap().inject();
                }
                info!("Unlock character: {}", input.unlock_character);
                std::thread::sleep(std::time::Duration::from_millis(500));
            }

            if !input.unlock_character {
                gc.consume_input(&input);
            };
        }

        input.reset();

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    Ok(())
}

memory_rs::main_dll!(wrapper);
