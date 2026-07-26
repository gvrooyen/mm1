use std::{
    env,
    error::Error,
    fs::{self, File},
    io::{self, IsTerminal, Read, Write},
    sync::Arc,
    time::Duration,
};

use crossterm::{event, terminal};
use pixels::{Pixels, SurfaceTexture};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use serde::Serialize;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 200;
const SCALE: u32 = 3;
const TITLE_MUSIC_PATH: &str = "assets/intro.mp3";
const TITLE_PICKUP_DURATION: Duration = Duration::from_micros(219_702);

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
    let args = parse_args()?;
    let view = PlayerView::title_screen();

    if args.headless {
        println!("{}", serde_json::to_string_pretty(&view)?);
        io::stdout().flush()?;
        wait_for_headless_keypress()?;
        return Ok(());
    }

    if args.browse {
        return run_asset_browser();
    }

    run_windowed()
}

#[derive(Default)]
struct Args {
    headless: bool,
    browse: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();

    for arg in env::args().skip(1) {
        match arg.as_str() {
            "--headless" => args.headless = true,
            "--browse" => args.browse = true,
            "-h" | "--help" => {
                println!(
                    "Usage: mm1 [--headless | --browse]\n\n  --headless  Print the current player view as JSON, then wait for a keypress\n  --browse    Browse original game assets"
                );
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    if args.headless && args.browse {
        return Err("--headless and --browse cannot be used together".into());
    }

    Ok(args)
}

fn wait_for_headless_keypress() -> io::Result<()> {
    if !io::stdin().is_terminal() {
        io::stdin().read_exact(&mut [0])?;
        return Ok(());
    }

    terminal::enable_raw_mode()?;
    let _raw_mode = RawModeGuard;
    loop {
        if let event::Event::Key(key) = event::read()?
            && key.is_press()
        {
            return Ok(());
        }
    }
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

fn run_windowed() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = GameWindow::new(TitleMusic::start());
    event_loop.run_app(&mut app)?;
    Ok(())
}

fn run_asset_browser() -> Result<(), Box<dyn Error>> {
    let monster_data = fs::read("dos/MONPIX.DTA")?;
    let monsters = decode_monsters(&monster_data)?;
    let wall_data = fs::read("dos/WALLPIX.DTA")?;
    let walls = decode_wall_sets(&wall_data)?;
    let event_loop = EventLoop::new()?;
    let mut app = AssetBrowser::new(monsters, walls);
    event_loop.run_app(&mut app)?;
    Ok(())
}

const BROWSER_ITEMS: [&str; 4] = ["MAPS", "MONSTERS", "WALLS", "ROSTER"];

enum BrowserPage {
    Menu,
    Monsters,
    Walls,
}

struct WallSet {
    components: Vec<Vec<u8>>,
}

struct AssetBrowser {
    window: Option<Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    framebuffer: Vec<u8>,
    monsters: Vec<Vec<u8>>,
    walls: Vec<WallSet>,
    page: BrowserPage,
    selection: usize,
    monster: usize,
    wall: usize,
    modifiers: ModifiersState,
}

impl AssetBrowser {
    fn new(monsters: Vec<Vec<u8>>, walls: Vec<WallSet>) -> Self {
        let mut browser = Self {
            window: None,
            pixels: None,
            framebuffer: vec![BLACK; (WIDTH * HEIGHT) as usize],
            monsters,
            walls,
            page: BrowserPage::Menu,
            selection: 0,
            monster: 0,
            wall: 0,
            modifiers: ModifiersState::empty(),
        };
        browser.redraw_framebuffer();
        browser
    }

    fn redraw_framebuffer(&mut self) {
        self.framebuffer.fill(BLACK);
        match self.page {
            BrowserPage::Menu => {
                draw_dos_text(&mut self.framebuffer, 104, 28, "ASSET BROWSER", WHITE);
                for (index, item) in BROWSER_ITEMS.iter().enumerate() {
                    let y = 70 + index as u32 * 24;
                    if index == self.selection {
                        fill_rect(&mut self.framebuffer, 112, y - 4, 96, 16, WHITE);
                        draw_dos_text(&mut self.framebuffer, 120, y, item, BLACK);
                    } else {
                        draw_dos_text(&mut self.framebuffer, 120, y, item, WHITE);
                    }
                }
            }
            BrowserPage::Monsters => {
                let image = &self.monsters[self.monster];
                for y in 0..96usize {
                    let source = &image[y * 104..(y + 1) * 104];
                    let destination = (32 + y) * WIDTH as usize + 108;
                    self.framebuffer[destination..destination + 104].copy_from_slice(source);
                }
                draw_dos_text(&mut self.framebuffer, 120, 16, "MONSTERS", WHITE);
                let position = format!("{:02} / {:02}", self.monster + 1, self.monsters.len());
                draw_dos_text(&mut self.framebuffer, 124, 144, &position, WHITE);
                draw_dos_text(
                    &mut self.framebuffer,
                    60,
                    176,
                    "ARROWS: PREVIOUS / NEXT",
                    CYAN,
                );
            }
            BrowserPage::Walls => {
                draw_wall_preview(&mut self.framebuffer, &self.walls[self.wall], 40, 32);
                draw_dos_text(&mut self.framebuffer, 140, 16, "WALLS", WHITE);
                let position = format!("{:02} / {:02}", self.wall + 1, self.walls.len());
                draw_dos_text(&mut self.framebuffer, 124, 164, &position, WHITE);
                draw_dos_text(
                    &mut self.framebuffer,
                    60,
                    184,
                    "ARROWS: PREVIOUS / NEXT",
                    CYAN,
                );
            }
        }
    }

    fn key_pressed(&mut self, key: &Key) {
        if key == &Key::Named(NamedKey::Escape) {
            if matches!(self.page, BrowserPage::Monsters | BrowserPage::Walls) {
                self.page = BrowserPage::Menu;
                self.redraw_framebuffer();
            }
            return;
        }

        match self.page {
            BrowserPage::Menu => match key {
                Key::Named(NamedKey::ArrowUp) => {
                    self.selection = self
                        .selection
                        .checked_sub(1)
                        .unwrap_or(BROWSER_ITEMS.len() - 1);
                }
                Key::Named(NamedKey::ArrowDown) => {
                    self.selection = (self.selection + 1) % BROWSER_ITEMS.len();
                }
                Key::Named(NamedKey::Enter) if self.selection == 1 => {
                    self.page = BrowserPage::Monsters;
                }
                Key::Named(NamedKey::Enter) if self.selection == 2 => {
                    self.page = BrowserPage::Walls;
                }
                _ => return,
            },
            BrowserPage::Monsters => match key {
                Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => {
                    self.monster = self
                        .monster
                        .checked_sub(1)
                        .unwrap_or(self.monsters.len() - 1);
                }
                Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => {
                    self.monster = (self.monster + 1) % self.monsters.len();
                }
                _ => return,
            },
            BrowserPage::Walls => match key {
                Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => {
                    self.wall = self.wall.checked_sub(1).unwrap_or(self.walls.len() - 1);
                }
                Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => {
                    self.wall = (self.wall + 1) % self.walls.len();
                }
                _ => return,
            },
        }
        self.redraw_framebuffer();
    }
}

fn draw_wall_preview(frame: &mut [u8], wall: &WallSet, x: usize, y: usize) {
    const POSITIONS: [(usize, usize); 9] = [
        (0, 0),
        (32, 16),
        (72, 32),
        (96, 48),
        (208, 0),
        (168, 16),
        (144, 32),
        (128, 48),
        (112, 56),
    ];
    const COMPONENTS: [usize; 9] = [0, 1, 2, 3, 4, 5, 6, 7, 11];

    for (&component, &(component_x, component_y)) in COMPONENTS.iter().zip(&POSITIONS) {
        let (width, height) = WALL_COMPONENT_DIMENSIONS[component];
        let pixels = &wall.components[component];
        for row in 0..height {
            let source = &pixels[row * width..(row + 1) * width];
            let destination = (y + component_y + row) * WIDTH as usize + x + component_x;
            frame[destination..destination + width].copy_from_slice(source);
        }
    }
}

fn is_quit_shortcut(key: &Key, modifiers: ModifiersState) -> bool {
    modifiers.control_key()
        && matches!(key, Key::Character(character) if
            character.eq_ignore_ascii_case("c") || character.eq_ignore_ascii_case("q"))
}

impl ApplicationHandler for AssetBrowser {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Might and Magic Asset Browser")
                        .with_inner_size(LogicalSize::new(WIDTH * SCALE, HEIGHT * SCALE))
                        .with_min_inner_size(LogicalSize::new(WIDTH, HEIGHT)),
                )
                .expect("could not create the asset browser window"),
        );
        let size = window.inner_size();
        let surface = SurfaceTexture::new(size.width, size.height, window.clone());
        self.pixels = Some(
            Pixels::new(WIDTH, HEIGHT, surface)
                .expect("could not create the asset browser rendering surface"),
        );
        self.window = Some(window);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if is_quit_shortcut(&event.logical_key, self.modifiers) {
                    event_loop.exit();
                } else {
                    self.key_pressed(&event.logical_key);
                }
            }
            WindowEvent::Resized(PhysicalSize { width, height }) if width > 0 && height > 0 => {
                if let Some(pixels) = self.pixels.as_mut()
                    && let Err(error) = pixels.resize_surface(width, height)
                {
                    eprintln!("could not resize the window surface: {error}");
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(pixels) = self.pixels.as_mut() {
                    copy_to_rgba(&self.framebuffer, pixels.frame_mut());
                    if let Err(error) = pixels.render() {
                        eprintln!("could not render the asset browser: {error}");
                        event_loop.exit();
                    }
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

fn decode_monsters(data: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    let index_bytes = read_u16(data, 0)? as usize;
    if index_bytes == 0 || !index_bytes.is_multiple_of(4) {
        return Err("MONPIX.DTA has an invalid index size".into());
    }
    let object_data_start = 2 + index_bytes;
    if object_data_start > data.len() {
        return Err("MONPIX.DTA index extends past the end of the file".into());
    }

    (0..index_bytes / 4)
        .map(|index| {
            let offset_position = 2 + index * 4;
            let offset = read_u32(data, offset_position)? as usize;
            let record_start = object_data_start
                .checked_add(offset)
                .ok_or("MONPIX.DTA record offset overflow")?;
            let compressed_size = read_u16(data, record_start)? as usize;
            let payload_start = record_start + 2;
            let payload_end = payload_start
                .checked_add(compressed_size)
                .ok_or("MONPIX.DTA record size overflow")?;
            let payload = data
                .get(payload_start..payload_end)
                .ok_or("MONPIX.DTA record extends past the end of the file")?;
            decode_monster(payload)
        })
        .collect()
}

fn decode_monster(compressed: &[u8]) -> Result<Vec<u8>, String> {
    let packed = decode_rle(compressed, 2496, "MONPIX.DTA image")?;
    Ok(unpack_image(&packed, 104, 96))
}

const WALL_COMPONENT_DIMENSIONS: [(usize, usize); 12] = [
    (32, 128),
    (40, 96),
    (24, 64),
    (16, 32),
    (32, 128),
    (40, 96),
    (24, 64),
    (16, 32),
    (176, 96),
    (96, 64),
    (48, 32),
    (16, 16),
];

fn decode_wall_sets(data: &[u8]) -> Result<Vec<WallSet>, String> {
    let index_bytes = read_u16(data, 0)? as usize;
    if index_bytes == 0 || !index_bytes.is_multiple_of(4) {
        return Err("WALLPIX.DTA has an invalid index size".into());
    }
    let object_data_start = 2 + index_bytes;
    if object_data_start > data.len() {
        return Err("WALLPIX.DTA index extends past the end of the file".into());
    }

    (0..index_bytes / 4)
        .map(|index| {
            let offset = read_u32(data, 2 + index * 4)? as usize;
            let record_start = object_data_start
                .checked_add(offset)
                .ok_or("WALLPIX.DTA record offset overflow")?;
            let compressed_size = read_u16(data, record_start)? as usize;
            let payload_start = record_start + 2;
            let payload_end = payload_start
                .checked_add(compressed_size)
                .ok_or("WALLPIX.DTA record size overflow")?;
            let payload = data
                .get(payload_start..payload_end)
                .ok_or("WALLPIX.DTA record extends past the end of the file")?;
            decode_wall_set(payload)
        })
        .collect()
}

fn decode_wall_set(compressed: &[u8]) -> Result<WallSet, String> {
    let packed = decode_rle(compressed, 11_200, "WALLPIX.DTA wall set")?;
    let mut offset = 0;
    let components = WALL_COMPONENT_DIMENSIONS
        .iter()
        .map(|&(width, height)| {
            let size = width * height / 4;
            let component = unpack_image(&packed[offset..offset + size], width, height);
            offset += size;
            component
        })
        .collect();
    Ok(WallSet { components })
}

fn decode_rle(
    compressed: &[u8],
    expected_size: usize,
    description: &str,
) -> Result<Vec<u8>, String> {
    let mut packed = Vec::with_capacity(expected_size);
    let mut position = 0;
    while position < compressed.len() {
        let value = compressed[position];
        position += 1;
        if value == 0x7b {
            let run = compressed
                .get(position..position + 2)
                .ok_or_else(|| format!("truncated {description} RLE run"))?;
            packed.extend(std::iter::repeat_n(run[1], run[0] as usize + 1));
            position += 2;
        } else {
            packed.push(value);
        }
    }
    if packed.len() != expected_size {
        return Err(format!(
            "{description} decoded to {} bytes instead of {expected_size}",
            packed.len(),
        ));
    }
    Ok(packed)
}

fn unpack_image(packed: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut pixels = vec![BLACK; width * height];
    for (stream_index, value) in packed.iter().copied().enumerate() {
        let byte_x = stream_index / height;
        let y = stream_index % height;
        for pixel in 0..4 {
            pixels[y * width + byte_x * 4 + pixel] = (value >> (6 - pixel * 2)) & 3;
        }
    }
    pixels
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, String> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or("unexpected end of graphics data")?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, String> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or("unexpected end of graphics data")?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

struct TitleMusic {
    _device: MixerDeviceSink,
    _player: Player,
}

impl TitleMusic {
    fn start() -> Option<Self> {
        match Self::try_start() {
            Ok(music) => Some(music),
            Err(error) => {
                eprintln!("could not play title music: {error}");
                None
            }
        }
    }

    fn try_start() -> Result<Self, Box<dyn Error>> {
        let mut device = DeviceSinkBuilder::open_default_sink()?;
        device.log_on_drop(false);
        let player = Player::connect_new(device.mixer());
        player.append(Decoder::try_from(File::open(TITLE_MUSIC_PATH)?)?);
        player.append(
            Decoder::try_from(File::open(TITLE_MUSIC_PATH)?)?
                .skip_duration(TITLE_PICKUP_DURATION)
                .repeat_infinite(),
        );
        Ok(Self {
            _device: device,
            _player: player,
        })
    }
}

struct GameWindow {
    window: Option<Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    framebuffer: Vec<u8>,
    _title_music: Option<TitleMusic>,
}

impl GameWindow {
    fn new(title_music: Option<TitleMusic>) -> Self {
        Self {
            window: None,
            pixels: None,
            framebuffer: title_framebuffer(),
            _title_music: title_music,
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
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                event_loop.exit();
            }
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

fn draw_dos_text(frame: &mut [u8], x: u32, y: u32, text: &str, color: u8) {
    for (index, character) in text.chars().enumerate() {
        for (row, bits) in dos_glyph(character).iter().enumerate() {
            for column in 0..8 {
                if bits & (0x80 >> column) != 0 {
                    fill_rect(
                        frame,
                        x + index as u32 * 8 + column,
                        y + row as u32,
                        1,
                        1,
                        color,
                    );
                }
            }
        }
    }
}

// The original DOS presentation uses the familiar 8x8 IBM PC character-cell style.
fn dos_glyph(character: char) -> [u8; 8] {
    match character {
        'A' => [0x30, 0x78, 0xcc, 0xcc, 0xfc, 0xcc, 0xcc, 0x00],
        'B' => [0xfc, 0x66, 0x66, 0x7c, 0x66, 0x66, 0xfc, 0x00],
        'C' => [0x3c, 0x66, 0xc0, 0xc0, 0xc0, 0x66, 0x3c, 0x00],
        'D' => [0xf8, 0x6c, 0x66, 0x66, 0x66, 0x6c, 0xf8, 0x00],
        'E' => [0xfe, 0x62, 0x68, 0x78, 0x68, 0x62, 0xfe, 0x00],
        'F' => [0xfe, 0x62, 0x68, 0x78, 0x68, 0x60, 0xf0, 0x00],
        'G' => [0x3c, 0x66, 0xc0, 0xc0, 0xce, 0x66, 0x3e, 0x00],
        'H' => [0xcc, 0xcc, 0xcc, 0xfc, 0xcc, 0xcc, 0xcc, 0x00],
        'I' => [0x78, 0x30, 0x30, 0x30, 0x30, 0x30, 0x78, 0x00],
        'J' => [0x1e, 0x0c, 0x0c, 0x0c, 0xcc, 0xcc, 0x78, 0x00],
        'K' => [0xe6, 0x66, 0x6c, 0x78, 0x6c, 0x66, 0xe6, 0x00],
        'L' => [0xf0, 0x60, 0x60, 0x60, 0x62, 0x66, 0xfe, 0x00],
        'M' => [0xc6, 0xee, 0xfe, 0xfe, 0xd6, 0xc6, 0xc6, 0x00],
        'N' => [0xc6, 0xe6, 0xf6, 0xde, 0xce, 0xc6, 0xc6, 0x00],
        'O' => [0x38, 0x6c, 0xc6, 0xc6, 0xc6, 0x6c, 0x38, 0x00],
        'P' => [0xfc, 0x66, 0x66, 0x7c, 0x60, 0x60, 0xf0, 0x00],
        'Q' => [0x78, 0xcc, 0xcc, 0xcc, 0xdc, 0x78, 0x1c, 0x00],
        'R' => [0xfc, 0x66, 0x66, 0x7c, 0x6c, 0x66, 0xe6, 0x00],
        'S' => [0x78, 0xcc, 0xe0, 0x70, 0x1c, 0xcc, 0x78, 0x00],
        'T' => [0xfc, 0xb4, 0x30, 0x30, 0x30, 0x30, 0x78, 0x00],
        'U' => [0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xfc, 0x00],
        'V' => [0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0x78, 0x30, 0x00],
        'W' => [0xc6, 0xc6, 0xc6, 0xd6, 0xfe, 0xee, 0xc6, 0x00],
        'X' => [0xc6, 0xc6, 0x6c, 0x38, 0x6c, 0xc6, 0xc6, 0x00],
        'Y' => [0xcc, 0xcc, 0xcc, 0x78, 0x30, 0x30, 0x78, 0x00],
        'Z' => [0xfe, 0xc6, 0x8c, 0x18, 0x32, 0x66, 0xfe, 0x00],
        '0' => [0x7c, 0xc6, 0xce, 0xde, 0xf6, 0xe6, 0x7c, 0x00],
        '1' => [0x30, 0x70, 0x30, 0x30, 0x30, 0x30, 0xfc, 0x00],
        '2' => [0x78, 0xcc, 0x0c, 0x38, 0x60, 0xcc, 0xfc, 0x00],
        '3' => [0x78, 0xcc, 0x0c, 0x38, 0x0c, 0xcc, 0x78, 0x00],
        '4' => [0x1c, 0x3c, 0x6c, 0xcc, 0xfe, 0x0c, 0x1e, 0x00],
        '5' => [0xfc, 0xc0, 0xf8, 0x0c, 0x0c, 0xcc, 0x78, 0x00],
        '6' => [0x38, 0x60, 0xc0, 0xf8, 0xcc, 0xcc, 0x78, 0x00],
        '7' => [0xfc, 0xcc, 0x0c, 0x18, 0x30, 0x30, 0x30, 0x00],
        '8' => [0x78, 0xcc, 0xcc, 0x78, 0xcc, 0xcc, 0x78, 0x00],
        '9' => [0x78, 0xcc, 0xcc, 0x7c, 0x0c, 0x18, 0x70, 0x00],
        ':' => [0x00, 0x30, 0x30, 0x00, 0x00, 0x30, 0x30, 0x00],
        '/' => [0x06, 0x0c, 0x18, 0x30, 0x60, 0xc0, 0x80, 0x00],
        _ => [0; 8],
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

    #[test]
    fn title_music_is_a_decodable_mp3() {
        let decoder = Decoder::try_from(File::open(TITLE_MUSIC_PATH).unwrap()).unwrap();
        assert!(decoder.take(1).next().is_some());
    }

    #[test]
    fn supplied_monster_file_decodes_all_images() {
        let data = fs::read("dos/MONPIX.DTA").unwrap();
        let monsters = decode_monsters(&data).unwrap();

        assert_eq!(monsters.len(), 76);
        assert!(monsters.iter().all(|monster| monster.len() == 104 * 96));
        assert!(monsters.iter().flatten().all(|pixel| *pixel < 4));
    }

    #[test]
    fn supplied_wall_file_decodes_all_component_sets() {
        let data = fs::read("dos/WALLPIX.DTA").unwrap();
        let walls = decode_wall_sets(&data).unwrap();

        assert_eq!(walls.len(), 18);
        for wall in walls {
            assert_eq!(wall.components.len(), WALL_COMPONENT_DIMENSIONS.len());
            for (component, &(width, height)) in
                wall.components.iter().zip(&WALL_COMPONENT_DIMENSIONS)
            {
                assert_eq!(component.len(), width * height);
                assert!(component.iter().all(|pixel| *pixel < 4));
            }
        }
    }

    #[test]
    fn malformed_monster_rle_is_rejected() {
        assert!(decode_monster(&[0x7b]).is_err());
    }

    #[test]
    fn browser_menu_opens_monsters() {
        let mut browser = test_browser();

        browser.key_pressed(&Key::Named(NamedKey::Enter));
        assert!(matches!(browser.page, BrowserPage::Menu));

        browser.key_pressed(&Key::Named(NamedKey::ArrowDown));
        browser.key_pressed(&Key::Named(NamedKey::Enter));
        assert!(matches!(browser.page, BrowserPage::Monsters));
    }

    #[test]
    fn escape_returns_to_the_previous_browser_menu() {
        let mut browser = test_browser();
        browser.page = BrowserPage::Monsters;

        browser.key_pressed(&Key::Named(NamedKey::Escape));
        assert!(matches!(browser.page, BrowserPage::Menu));

        browser.key_pressed(&Key::Named(NamedKey::Escape));
        assert!(matches!(browser.page, BrowserPage::Menu));
    }

    #[test]
    fn ctrl_c_and_ctrl_q_are_quit_shortcuts() {
        let control = ModifiersState::CONTROL;

        assert!(is_quit_shortcut(&Key::Character("c".into()), control));
        assert!(is_quit_shortcut(&Key::Character("Q".into()), control));
        assert!(!is_quit_shortcut(
            &Key::Character("q".into()),
            ModifiersState::empty()
        ));
        assert!(!is_quit_shortcut(&Key::Named(NamedKey::Escape), control));
    }

    #[test]
    fn monster_browser_wraps_in_both_directions() {
        let mut browser = AssetBrowser::new(vec![vec![BLACK; 104 * 96]; 2], vec![blank_wall_set()]);
        browser.page = BrowserPage::Monsters;

        browser.key_pressed(&Key::Named(NamedKey::ArrowLeft));
        assert_eq!(browser.monster, 1);
        browser.key_pressed(&Key::Named(NamedKey::ArrowRight));
        assert_eq!(browser.monster, 0);
    }

    #[test]
    fn monster_browser_up_is_next_down_is_previous() {
        let mut browser = AssetBrowser::new(vec![vec![BLACK; 104 * 96]; 3], vec![blank_wall_set()]);
        browser.page = BrowserPage::Monsters;

        browser.key_pressed(&Key::Named(NamedKey::ArrowUp));
        assert_eq!(browser.monster, 1, "UP moves to the next image");
        browser.key_pressed(&Key::Named(NamedKey::ArrowUp));
        assert_eq!(browser.monster, 2);
        browser.key_pressed(&Key::Named(NamedKey::ArrowDown));
        assert_eq!(browser.monster, 1, "DOWN moves to the previous image");
    }

    #[test]
    fn wall_browser_opens_and_wraps_in_both_directions() {
        let mut browser = AssetBrowser::new(
            vec![vec![BLACK; 104 * 96]],
            vec![blank_wall_set(), blank_wall_set()],
        );
        browser.selection = 2;
        browser.key_pressed(&Key::Named(NamedKey::Enter));
        assert!(matches!(browser.page, BrowserPage::Walls));

        browser.key_pressed(&Key::Named(NamedKey::ArrowLeft));
        assert_eq!(browser.wall, 1);
        browser.key_pressed(&Key::Named(NamedKey::ArrowRight));
        assert_eq!(browser.wall, 0);
    }

    #[test]
    fn wall_browser_up_is_next_down_is_previous() {
        let mut browser = AssetBrowser::new(
            vec![vec![BLACK; 104 * 96]],
            vec![blank_wall_set(), blank_wall_set(), blank_wall_set()],
        );
        browser.selection = 2;
        browser.key_pressed(&Key::Named(NamedKey::Enter));
        assert!(matches!(browser.page, BrowserPage::Walls));

        browser.key_pressed(&Key::Named(NamedKey::ArrowUp));
        assert_eq!(browser.wall, 1, "UP moves to the next image");
        browser.key_pressed(&Key::Named(NamedKey::ArrowUp));
        assert_eq!(browser.wall, 2);
        browser.key_pressed(&Key::Named(NamedKey::ArrowDown));
        assert_eq!(browser.wall, 1, "DOWN moves to the previous image");
    }

    fn test_browser() -> AssetBrowser {
        AssetBrowser::new(vec![vec![BLACK; 104 * 96]], vec![blank_wall_set()])
    }

    fn blank_wall_set() -> WallSet {
        WallSet {
            components: WALL_COMPONENT_DIMENSIONS
                .iter()
                .map(|&(width, height)| vec![BLACK; width * height])
                .collect(),
        }
    }
}
