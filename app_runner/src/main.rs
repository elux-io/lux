use winapi::{
    shared::minwindef::HMODULE,
    um::{
        fileapi::{
            FindCloseChangeNotification, FindFirstChangeNotificationA, FindNextChangeNotification,
        },
        libloaderapi::{FreeLibrary, GetProcAddress, LoadLibraryA},
        synchapi::WaitForSingleObjectEx,
        winbase::WAIT_OBJECT_0,
        winnt::FILE_NOTIFY_CHANGE_LAST_WRITE,
    },
};
use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    platform::run_return::EventLoopExtRunReturn,
    window::{Window, WindowBuilder},
};

use std::ffi::c_void;

struct AppCode {
    dll: HMODULE,
    app_new: unsafe fn(&Window) -> *mut c_void,
    app_drop: unsafe fn(*mut c_void),
    app_update: unsafe fn(*mut c_void),
    app_on_resize: unsafe fn(*mut c_void, u32, u32),
}

impl AppCode {
    unsafe fn new(loaded_dll_path: &[u8]) -> Self {
        let dll = LoadLibraryA(loaded_dll_path.as_ptr().cast());

        // Je vérifie pas les erreurs parce que je m'en fous
        Self {
            dll,
            app_new: std::mem::transmute(GetProcAddress(dll, b"lux_app_new\0".as_ptr().cast())),
            app_drop: std::mem::transmute(GetProcAddress(dll, b"lux_app_drop\0".as_ptr().cast())),
            app_update: std::mem::transmute(GetProcAddress(dll, b"lux_app_update\0".as_ptr().cast())),
            app_on_resize: std::mem::transmute(GetProcAddress(dll, b"lux_app_on_resize\0".as_ptr().cast())),
        }
    }
}

unsafe fn load_app_code() -> AppCode {
    std::fs::copy("target/debug/app.dll", "target/debug/loaded_app.dll").unwrap();
    AppCode::new(b"target/debug/loaded_app.dll\0")
}

fn main() {
    env_logger::init();
    let mut event_loop = EventLoop::new();
    let window = WindowBuilder::new().build(&event_loop).unwrap();

    let dll_watch_handle = unsafe {
        FindFirstChangeNotificationA(
            b"target\0".as_ptr().cast(),
            0,
            FILE_NOTIFY_CHANGE_LAST_WRITE,
        )
    };

    let mut app_code = unsafe { load_app_code() };
    let app = unsafe { (app_code.app_new)(&window) };

    event_loop.run_return(|event, _, control_flow| {
        control_flow.set_poll();

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => *control_flow = ControlFlow::Exit,

                WindowEvent::Resized(physical_size) => unsafe {
                    (app_code.app_on_resize)(app, physical_size.width, physical_size.height);
                },

                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => unsafe {
                    (app_code.app_on_resize)(app, new_inner_size.width, new_inner_size.height);
                },

                _ => {}
            },

            Event::MainEventsCleared => unsafe {
                let wait_status = WaitForSingleObjectEx(dll_watch_handle, 0, 0);

                if wait_status == WAIT_OBJECT_0 {
                    FreeLibrary(app_code.dll);
                    app_code = load_app_code();
                }

                FindNextChangeNotification(dll_watch_handle);

                (app_code.app_update)(app);
            },
            _ => (),
        }
    });

    unsafe {
        (app_code.app_drop)(app);
        FreeLibrary(app_code.dll);
        FindCloseChangeNotification(dll_watch_handle);
    }
}
