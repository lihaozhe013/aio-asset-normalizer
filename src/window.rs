use three_d::{Window, WindowError, WindowSettings};
use winit::{
    dpi::LogicalSize,
    event_loop::EventLoop,
    window::{Icon, WindowBuilder},
};

const ICON_PNG: &[u8] =
    include_bytes!("../assets/icon/aio-asset-normalizer-transparent.png");

pub fn create() -> Result<Window, WindowError> {
    let icon = image::load_from_memory(ICON_PNG)
        .expect("failed to decode the application icon")
        .into_rgba8();
    let icon = Icon::from_rgba(icon.into_raw(), 1254, 1254)
        .expect("failed to create the application icon");

    let event_loop = EventLoop::new();
    let winit_window = WindowBuilder::new()
        .with_title("AIO Asset Normalizer")
        .with_min_inner_size(LogicalSize::new(1024_u32, 600_u32))
        .with_inner_size(LogicalSize::new(1200_u32, 750_u32))
        .with_window_icon(Some(icon))
        .build(&event_loop)
        .expect("failed to create the application window");

    Window::from_winit_window(
        winit_window,
        event_loop,
        WindowSettings::default().surface_settings,
        false,
    )
}
