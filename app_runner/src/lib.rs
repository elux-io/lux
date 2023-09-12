#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
use winit::{
    dpi::PhysicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

#[cfg(target_arch = "wasm32")]
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn wasm_main() {
    use app::{lux_app_drop, lux_app_new, lux_app_on_resize, lux_app_update};
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    const WIDTH: u32 = 1280;
    const HEIGHT: u32 = 720;

    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_inner_size(PhysicalSize::new(WIDTH, HEIGHT))
        .build(&event_loop)
        .unwrap();

    use winit::platform::web::WindowExtWebSys;
    web_sys::window()
        .and_then(|win| win.document())
        .and_then(|doc| {
            let container = doc.get_element_by_id("lux-canvas")?;
            let canvas = window.canvas();
            canvas
                .style()
                .set_css_text(&format!("width: 100%; max-width: {}px;", WIDTH));
            container
                .append_child(&web_sys::Element::from(canvas))
                .ok()?;
            Some(())
        })
        .expect("failed to append canvas");

    let app = lux_app_new(&window);

    event_loop.run(move |event, _, control_flow| {
        control_flow.set_poll();

        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    *control_flow = ControlFlow::Exit;
                    unsafe {
                        lux_app_drop(app);
                    }
                }

                WindowEvent::Resized(physical_size) => unsafe {
                    lux_app_on_resize(app, physical_size.width, physical_size.height);
                },

                WindowEvent::ScaleFactorChanged { new_inner_size, .. } => unsafe {
                    lux_app_on_resize(app, new_inner_size.width, new_inner_size.height);
                },

                _ => {}
            },

            Event::MainEventsCleared => unsafe {
                lux_app_update(app);
            },

            _ => (),
        }
    });
}
