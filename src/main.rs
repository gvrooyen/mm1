use std::{env, error::Error, sync::Arc};

use pixels::{Pixels, SurfaceTexture};
use serde::Serialize;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 200;
const SCALE: u32 = 3;

const BLACK: u8 = 0;
const CYAN: u8 = 1;
const MAGENTA: u8 = 2;
const WHITE: u8 = 3;

const PALETTE: [[u8; 4]; 4] = [
    [0x00, 0x00, 0x00, 0xff],
    [0x55, 0xff, 0xff, 0xff],
    [0xff, 0x55, 0xff, 0xff],
    [0xff, 0xff, 0xff, 0xff],
];

#[derive(Serialize)]
struct PlayerView<'a> {
    schema_version: u32,
    view: View<'a>,
}

#[derive(Serialize)]
struct View<'a> {
    kind: &'a str,
    width: u32,
    height: u32,
    title: &'a str,
    subtitle: &'a str,
    prompt: &'a str,
}

impl PlayerView<'static> {
    fn title_screen() -> Self {
        Self {
            schema_version: 1,
            view: View {
                kind: "title",
                width: WIDTH,
                height: HEIGHT,
                title: "Might and Magic",
                subtitle: "Book One: Secret of the Inner Sanctum",
                prompt: "Press any key",
            },
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let headless = parse_args()?;
    let view = PlayerView::title_screen();

    if headless {
        println!("{}", serde_json::to_string_pretty(&view)?);
        return Ok(());
    }

    run_windowed()
}

fn parse_args() -> Result<bool, String> {
    let mut headless = false;

    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--headless" => headless = true,
            "-h" | "--help" => {
                println!(
                    "Usage: mm1 [--headless]\n\n  --headless  Print the current player view as JSON and exit"
                );
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    Ok(headless)
}

fn run_windowed() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = GameWindow::new();
    event_loop.run_app(&mut app)?;
    Ok(())
}

struct GameWindow {
    window: Option<Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    framebuffer: Vec<u8>,
}

impl GameWindow {
    fn new() -> Self {
        Self {
            window: None,
            pixels: None,
            framebuffer: title_framebuffer(),
        }
    }
}

impl ApplicationHandler for GameWindow {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Might and Magic Book One")
                        .with_inner_size(LogicalSize::new(WIDTH * SCALE, HEIGHT * SCALE))
                        .with_min_inner_size(LogicalSize::new(WIDTH, HEIGHT)),
                )
                .expect("could not create the game window"),
        );
        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, window.clone());
        let pixels = Pixels::new(WIDTH, HEIGHT, surface)
            .expect("could not create the game rendering surface");

        self.pixels = Some(pixels);
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(pixels) = self.pixels.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(PhysicalSize { width, height }) if width > 0 && height > 0 => {
                if let Err(error) = pixels.resize_surface(width, height) {
                    eprintln!("could not resize the window surface: {error}");
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {
                copy_to_rgba(&self.framebuffer, pixels.frame_mut());
                if let Err(error) = pixels.render() {
                    eprintln!("could not render the title screen: {error}");
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

fn title_framebuffer() -> Vec<u8> {
    let mut frame = vec![BLACK; (WIDTH * HEIGHT) as usize];

    fill_rect(&mut frame, 5, 5, 310, 2, CYAN);
    fill_rect(&mut frame, 5, 193, 310, 2, MAGENTA);
    fill_rect(&mut frame, 5, 7, 2, 186, CYAN);
    fill_rect(&mut frame, 313, 7, 2, 186, MAGENTA);

    draw_text(&mut frame, 25, 52, "MIGHT AND MAGIC", 3, WHITE);
    draw_text(&mut frame, 108, 88, "BOOK ONE", 2, CYAN);
    draw_text(
        &mut frame,
        58,
        110,
        "SECRET OF THE INNER SANCTUM",
        1,
        MAGENTA,
    );
    draw_text(&mut frame, 116, 166, "PRESS ANY KEY", 1, WHITE);

    frame
}

fn copy_to_rgba(indexed: &[u8], rgba: &mut [u8]) {
    for (color, pixel) in indexed.iter().zip(rgba.chunks_exact_mut(4)) {
        pixel.copy_from_slice(&PALETTE[*color as usize]);
    }
}

fn fill_rect(frame: &mut [u8], x: u32, y: u32, width: u32, height: u32, color: u8) {
    for row in y..(y + height).min(HEIGHT) {
        let start = (row * WIDTH + x) as usize;
        let end = (row * WIDTH + (x + width).min(WIDTH)) as usize;
        frame[start..end].fill(color);
    }
}

fn draw_text(frame: &mut [u8], x: u32, y: u32, text: &str, scale: u32, color: u8) {
    let mut cursor = x;
    for character in text.chars() {
        let glyph = glyph(character);
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) != 0 {
                    fill_rect(
                        frame,
                        cursor + column * scale,
                        y + row as u32 * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
        cursor += 6 * scale;
    }
}

fn glyph(character: char) -> [u8; 7] {
    match character {
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        'C' => [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e],
        'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'G' => [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'I' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1f],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_framebuffer_has_the_logical_screen_dimensions() {
        assert_eq!(title_framebuffer().len(), (WIDTH * HEIGHT) as usize);
    }

    #[test]
    fn title_view_serializes_as_versioned_json() {
        let value = serde_json::to_value(PlayerView::title_screen()).unwrap();
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["view"]["kind"], "title");
        assert_eq!(value["view"]["width"], 320);
        assert_eq!(value["view"]["height"], 200);
    }
}
