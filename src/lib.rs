use winit::window::Window;

#[allow(unused)]
pub trait App {
    fn new(window: &Window) -> Self;
    fn update(&mut self) {}
    fn on_resize(&mut self, width: u32, height: u32) {}
}
