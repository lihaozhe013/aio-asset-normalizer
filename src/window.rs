use three_d::{
    Context, FrameInput, FrameInputGenerator, FrameOutput, Viewport, WindowError, WindowSettings,
    WindowedContext,
};
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop, EventLoopBuilder},
    window::{Icon, WindowBuilder},
};

const ICON_PNG: &[u8] = include_bytes!("../assets/icon/aio-asset-normalizer-transparent.png");

pub struct Window {
    window: winit::window::Window,
    event_loop: EventLoop<()>,
    gl: WindowedContext,
}

impl Window {
    pub fn create() -> Result<Self, WindowError> {
        let icon = image::load_from_memory(ICON_PNG)
            .expect("failed to decode the application icon")
            .into_rgba8();
        let icon = Icon::from_rgba(icon.into_raw(), 1254, 1254)
            .expect("failed to create the application icon");

        let mut event_loop_builder = EventLoopBuilder::new();
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::EventLoopBuilderExtMacOS;
            // winit installs its default menu at launch, after our menu is built.
            event_loop_builder.with_default_menu(false);
        }
        let event_loop = event_loop_builder.build();
        let window = WindowBuilder::new()
            .with_title("AIO Asset Normalizer")
            .with_min_inner_size(LogicalSize::new(1024_u32, 600_u32))
            .with_inner_size(LogicalSize::new(1200_u32, 750_u32))
            .with_window_icon(Some(icon))
            .build(&event_loop)
            .map_err(WindowError::from)?;

        let mut settings = WindowSettings::default().surface_settings;
        let gl = match WindowedContext::from_winit_window(&window, settings) {
            Ok(context) => context,
            Err(_) => {
                settings.multisamples = 0;
                WindowedContext::from_winit_window(&window, settings)?
            }
        };
        Ok(Self {
            window,
            event_loop,
            gl,
        })
    }

    pub fn gl(&self) -> Context {
        Context::clone(&self.gl)
    }

    pub fn viewport(&self) -> Viewport {
        let size = self.window.inner_size();
        Viewport::new_at_origo(size.width, size.height)
    }

    pub fn render_loop<F>(self, mut callback: F)
    where
        F: 'static + FnMut(FrameInput, bool) -> FrameOutput,
    {
        let Self {
            window,
            event_loop,
            gl,
        } = self;
        let mut frame_input_generator = FrameInputGenerator::from_winit_window(&window);
        let mut close_requested = false;
        event_loop.run(move |event, _, control_flow| match event {
            Event::MainEventsCleared => window.request_redraw(),
            Event::RedrawRequested(_) => {
                let frame_input = frame_input_generator.generate(&gl);
                let frame_output = callback(frame_input, close_requested);
                if frame_output.exit {
                    *control_flow = ControlFlow::Exit;
                } else {
                    if frame_output.swap_buffers {
                        gl.swap_buffers().expect("failed to swap window buffers");
                    }
                    if frame_output.wait_next_event && !close_requested {
                        *control_flow = ControlFlow::Wait;
                    } else {
                        *control_flow = ControlFlow::Poll;
                        window.request_redraw();
                    }
                }
            }
            Event::WindowEvent { event, .. } => {
                frame_input_generator.handle_winit_window_event(&event);
                match event {
                    WindowEvent::Resized(size) => gl.resize(size),
                    WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                        gl.resize(*new_inner_size)
                    }
                    WindowEvent::CloseRequested => {
                        close_requested = true;
                        *control_flow = ControlFlow::Poll;
                        window.request_redraw();
                    }
                    _ => {}
                }
            }
            _ => {}
        });
    }
}
