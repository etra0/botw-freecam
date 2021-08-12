use memory_rs::internal::{
    injections::{Detour, Inject, Injection},
    memory::{resolve_module_path, scan_aob},
    process_info::ProcessInfo,
};
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

use crate::utils::calc_eucl_distance;

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
struct MainGameInfo {
    camera: usize,
    rotation_vec1: usize,
    player_position: usize,
}

/// Returns an extern function from a name.
/// # Safety
/// This function is highly unsafe to allow its transmutability. It assumes caller responsability
/// if the types are correct or not.
fn get_extern_function<T>(name: &str) -> Result<T, Box<dyn std::error::Error>> {
    let func_name = format!("{}\x00", name);
    let proc_handle = unsafe { GetModuleHandleA(std::ptr::null_mut()) };
    let func: usize = unsafe { GetProcAddress(proc_handle, func_name.as_ptr() as _) } as _;

    if func == 0x00 { 
        return Err(format!("GetProcHandle couldn't find the function {}", name).into());
    }

    return Ok(unsafe { std::mem::transmute_copy(&func) });
}

type ExternFunction = extern "C" fn() -> usize;
fn get_main_game_info() -> Result<MainGameInfo, Box<dyn std::error::Error>> {
    let func: ExternFunction = get_extern_function("PPCRecompiler_getJumpTableBase")?;

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
            * You're using a pre 2016 CPU (your cpu doesn't support `movbe`)\n\
            * You're not using the version described on the README.md\n\
            {:x?} != {:x?}",
            camera_bytes, original_bytes
        )
        .into());
    }

    let rotation_vec1 = array[0x2e57fdc / 4] + 0x57;
    // Now we get the player position which we assume it's on this specific offset.
    let player_position_offset: usize = 0x113444F0;
    let get_base_addr_func: ExternFunction = get_extern_function("memory_getBase")?;
    let player_position = (get_base_addr_func)() + player_position_offset + 0x50;

    Ok(MainGameInfo {
        camera: camera_offset,
        rotation_vec1,
        player_position
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

    let main_game_info = get_main_game_info()?;
    info!("{:x?}", main_game_info);
    let camera_pointer = main_game_info.camera;
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
        Injection::new(main_game_info.camera + 0x17, vec![0x90; 10]),
        Injection::new(main_game_info.camera + 0x55, vec![0x90; 10]),
        Injection::new(main_game_info.camera + 0xC2, vec![0x90; 10]),
        Injection::new(main_game_info.camera + 0xD9, vec![0x90; 10]),
        Injection::new(main_game_info.camera + 0x117, vec![0x90; 10]),
        Injection::new(main_game_info.camera + 0x12E, vec![0x90; 10]),
        Injection::new(main_game_info.camera + 0x15D, vec![0x90; 10]),
        Injection::new(main_game_info.camera + 0x174, vec![0x90; 10]),
        Injection::new(main_game_info.camera + 0x22A, vec![0x90; 10]),
        Injection::new(main_game_info.camera + 0x22A, vec![0x90; 10]),

        // Rotation
        Injection::new(main_game_info.rotation_vec1, vec![0x90; 7]),
        Injection::new(main_game_info.rotation_vec1 + 0x14, vec![0x90; 7]),
        Injection::new(main_game_info.rotation_vec1 + 0x28, vec![0x90; 7]),
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
                input.reset();
                nops.inject();
            } else {
                nops.remove_injection();
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

            let gc = g_camera_struct as *mut GameCamera;
            let player_position = main_game_info.player_position as *mut PlayerPosition;

            if !active {
                input.fov = (*gc).fov.into();
                input.delta_rotation = 0.;
                continue;
            }

            // If we have a starting point and link is attached, we don't need to clamp the
            // distance, but we need to update the starting_point.
            // If link is not clipped, to avoid softlock we clamp the distance to 400 units.
            // If we don't have starting_point, we need to assign one ASAP.
            match (&starting_point, input.clip_link) {
                (Some(_), true) => {
                    starting_point = Some(CameraSnapshot::new(&(*gc)));
                },
                (Some(ref sp), false) => {
                    (*gc).clamp_distance(&sp.pos.into());
                }
                (None, _) => {
                    starting_point = Some(CameraSnapshot::new(&(*gc)));
                }
            };

            if !points.is_empty() {
                let origin = (*gc).pos.into();
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

            if check_key_press(Keys::K as _)  {
                input.clip_link = !input.clip_link;
                std::thread::sleep(std::time::Duration::from_millis(500));
            }

            if !input.unlock_character {
                let pp = if input.clip_link {
                    Some(&mut *player_position)
                } else {
                    None
                };

                (*gc).consume_input(&input, pp);
            };
        }

        input.reset();

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    Ok(())
}

memory_rs::main_dll!(wrapper);
