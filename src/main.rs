mod character;
mod game;

use std::{
    env,
    error::Error,
    fs::{self, File},
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use pixels::{Pixels, SurfaceTexture};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

use character::{Character, decode_roster};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 200;
const SCALE: u32 = 3;
const TITLE_MUSIC_PATH: &str = "assets/intro.mp3";
const TITLE_PICKUP_DURATION: Duration = Duration::from_micros(219_702);
const TITLE_RING_INTERVAL: Duration = Duration::from_millis(100);
const TITLE_RING_COUNT: u32 = 20;
const SLIDESHOW_INTERVAL: Duration = Duration::from_secs(10);
const FIRST_SCENE: usize = 2;
const LAST_SCENE: usize = 9;
const SAVE_PATH: &str = "savegame.json";

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
const TITLE_EGA_PALETTE: [[u8; 4]; 4] = [
    [0x00, 0x00, 0x00, 0xff],
    [0x00, 0xaa, 0x00, 0xff],
    [0xaa, 0x00, 0x00, 0xff],
    [0xff, 0xff, 0xff, 0xff],
];
const GAME_EGA_PALETTE: [[u8; 4]; 4] = [
    [0x00, 0x00, 0x00, 0xff],
    [0xff, 0xff, 0x55, 0xff],
    [0xaa, 0x55, 0x00, 0xff],
    [0xff, 0xff, 0xff, 0xff],
];

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;

    if args.browse {
        return run_asset_browser();
    }

    if args.reset {
        reset_save_game(Path::new(SAVE_PATH))?;
    }

    if args.headless {
        return run_headless(&args.commands, args.interactive);
    }

    run_windowed()
}

#[derive(Default)]
struct Args {
    headless: bool,
    interactive: bool,
    browse: bool,
    reset: bool,
    commands: Vec<String>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args::default();

    let mut input = env::args().skip(1);
    while let Some(arg) = input.next() {
        match arg.as_str() {
            "--headless" => args.headless = true,
            "--interactive" => args.interactive = true,
            "--browse" => args.browse = true,
            "--reset" => args.reset = true,
            "--commands" => {
                let value = input.next().ok_or("--commands requires a value")?;
                args.commands.extend(
                    value
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(str::to_owned),
                );
            }
            "-h" | "--help" => {
                println!(
                    "Usage: mm1 [--reset] [--headless [--interactive] [--commands LIST] | --browse]\n\n  --reset        Delete the saved game and start over from the beginning\n  --headless     Print the player view as JSON\n  --interactive  Read one command per stdin line and emit views as NDJSON\n  --commands     Repeatable comma-separated session commands\n  --browse       Browse original game assets"
                );
                std::process::exit(0);
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }

    if args.headless && args.browse {
        return Err("--headless and --browse cannot be used together".into());
    }
    if args.reset && args.browse {
        return Err("--reset and --browse cannot be used together".into());
    }
    if !args.headless && !args.commands.is_empty() {
        return Err("--commands requires --headless".into());
    }
    if args.interactive && !args.headless {
        return Err("--interactive requires --headless".into());
    }

    Ok(args)
}

fn reset_save_game(save_path: &Path) -> Result<(), Box<dyn Error>> {
    match fs::remove_file(save_path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!(
            "could not remove save game {}: {error}",
            save_path.display()
        )
        .into()),
    }
}

fn run_headless(commands: &[String], interactive: bool) -> Result<(), Box<dyn Error>> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    run_headless_io(
        commands,
        interactive,
        stdin.lock(),
        stdout.lock(),
        Path::new(SAVE_PATH),
    )
}

fn run_headless_io<R: BufRead, W: Write>(
    commands: &[String],
    interactive: bool,
    input: R,
    mut output: W,
    save_path: &Path,
) -> Result<(), Box<dyn Error>> {
    let mut game = game::Game::load_or_new(save_path)?;
    for command in commands {
        apply_and_save(&mut game, command, save_path)?;
    }

    if !interactive {
        serde_json::to_writer_pretty(&mut output, &game.view())?;
        writeln!(output)?;
        output.flush()?;
        game.save(save_path)?;
        return Ok(());
    }

    write_ndjson_view(&mut output, &game)?;
    for line in input.lines() {
        let line = line?;
        let command = line.trim();
        if command.is_empty() {
            continue;
        }
        apply_and_save(&mut game, command, save_path)?;
        write_ndjson_view(&mut output, &game)?;
    }
    game.save(save_path)?;
    Ok(())
}

fn apply_and_save(game: &mut game::Game, command: &str, save_path: &Path) -> Result<(), String> {
    game.command(command);
    game.save(save_path)
}

fn write_ndjson_view(output: &mut impl Write, game: &game::Game) -> Result<(), Box<dyn Error>> {
    serde_json::to_writer(&mut *output, &game.view())?;
    writeln!(output)?;
    output.flush()?;
    Ok(())
}

fn run_windowed() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    let animation = TitleAnimation::load()?;
    let game = game::Game::load_or_new(Path::new(SAVE_PATH))?;
    let mut app = GameWindow::new(
        animation,
        TitleMusic::start(),
        game,
        PathBuf::from(SAVE_PATH),
    );
    let run_result = event_loop.run_app(&mut app);
    let save_result = app.game.save(&app.save_path);
    run_result?;
    save_result?;
    Ok(())
}

fn run_asset_browser() -> Result<(), Box<dyn Error>> {
    let mut images = Vec::new();
    for index in 0..10 {
        let data = fs::read(format!("dos/SCREEN{index}"))?;
        images.push(decode_screen(&data)?);
    }
    let monster_data = fs::read("dos/MONPIX.DTA")?;
    let monsters = decode_monsters(&monster_data)?;
    let wall_data = fs::read("dos/WALLPIX.DTA")?;
    let walls = decode_wall_sets(&wall_data)?;
    let map_data = fs::read("dos/MAZEDATA.DTA")?;
    let maps = decode_maps(&map_data)?;
    let roster_data = fs::read("dos/ROSTER.DTA")?;
    let characters = decode_roster(&roster_data)?
        .into_iter()
        .filter(|entry| entry.metadata != 0)
        .map(|entry| entry.character)
        .collect();
    let event_loop = EventLoop::new()?;
    let mut app = AssetBrowser::new(images, monsters, walls, characters, maps);
    event_loop.run_app(&mut app)?;
    Ok(())
}

const BROWSER_ITEMS: [&str; 5] = ["MAPS", "MONSTERS", "WALLS", "IMAGES", "ROSTER"];

enum BrowserPage {
    Menu,
    Maps,
    Monsters,
    Walls,
    Images,
    Roster,
}

struct WallSet {
    components: Vec<Vec<u8>>,
}

#[derive(Clone)]
struct Map {
    walls: [u8; 256],
    #[allow(dead_code)] // Retained losslessly for future map-property views.
    properties: [u8; 256],
}

const MAP_NAMES: [&str; 55] = [
    "SORPIGAL.OVR",
    "PORTSMIT.OVR",
    "ALGARY.OVR",
    "DUSK.OVR",
    "ERLIQUIN.OVR",
    "CAVE1.OVR",
    "CAVE2.OVR",
    "CAVE3.OVR",
    "CAVE4.OVR",
    "CAVE5.OVR",
    "CAVE6.OVR",
    "CAVE7.OVR",
    "CAVE8.OVR",
    "CAVE9.OVR",
    "AREAA1.OVR",
    "AREAA2.OVR",
    "AREAA3.OVR",
    "AREAA4.OVR",
    "AREAB1.OVR",
    "AREAB2.OVR",
    "AREAB3.OVR",
    "AREAB4.OVR",
    "AREAC1.OVR",
    "AREAC2.OVR",
    "AREAC3.OVR",
    "AREAC4.OVR",
    "AREAD1.OVR",
    "AREAD2.OVR",
    "AREAD3.OVR",
    "AREAD4.OVR",
    "AREAE1.OVR",
    "AREAE2.OVR",
    "AREAE3.OVR",
    "AREAE4.OVR",
    "DOOM.OVR",
    "BLACKRN.OVR",
    "BLACKRS.OVR",
    "QVL1.OVR",
    "QVL2.OVR",
    "RWL1.OVR",
    "RWL2.OVR",
    "ENF1.OVR",
    "ENF2.OVR",
    "WHITEW.OVR",
    "DRAGAD.OVR",
    "UDRAG1.OVR",
    "UDRAG2.OVR",
    "UDRAG3.OVR",
    "DEMON.OVR",
    "ALAMAR.OVR",
    "PP1.OVR",
    "PP2.OVR",
    "PP3.OVR",
    "PP4.OVR",
    "ASTRAL.OVR",
];

struct AssetBrowser {
    window: Option<Arc<Window>>,
    pixels: Option<Pixels<'static>>,
    framebuffer: Vec<u8>,
    images: Vec<Vec<u8>>,
    monsters: Vec<Vec<u8>>,
    walls: Vec<WallSet>,
    characters: Vec<Character>,
    maps: Vec<Map>,
    page: BrowserPage,
    selection: usize,
    image: usize,
    monster: usize,
    wall: usize,
    character: usize,
    map: usize,
    modifiers: ModifiersState,
}

impl AssetBrowser {
    fn new(
        images: Vec<Vec<u8>>,
        monsters: Vec<Vec<u8>>,
        walls: Vec<WallSet>,
        characters: Vec<Character>,
        maps: Vec<Map>,
    ) -> Self {
        let mut browser = Self {
            window: None,
            pixels: None,
            framebuffer: vec![BLACK; (WIDTH * HEIGHT) as usize],
            images,
            monsters,
            walls,
            characters,
            maps,
            page: BrowserPage::Menu,
            selection: 0,
            image: 0,
            monster: 0,
            wall: 0,
            character: 0,
            map: 0,
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
            BrowserPage::Maps => self.draw_map(),
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
            BrowserPage::Images => {
                self.framebuffer.copy_from_slice(&self.images[self.image]);
            }
            BrowserPage::Roster => self.draw_character(),
        }
    }

    fn key_pressed(&mut self, key: &Key) {
        if key == &Key::Named(NamedKey::Escape) {
            if matches!(
                self.page,
                BrowserPage::Monsters
                    | BrowserPage::Maps
                    | BrowserPage::Walls
                    | BrowserPage::Images
                    | BrowserPage::Roster
            ) {
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
                Key::Named(NamedKey::Enter) if self.selection == 3 => {
                    self.page = BrowserPage::Images;
                }
                Key::Named(NamedKey::Enter)
                    if self.selection == 4 && !self.characters.is_empty() =>
                {
                    self.page = BrowserPage::Roster;
                }
                Key::Named(NamedKey::Enter) if self.selection == 0 => {
                    self.page = BrowserPage::Maps;
                }
                _ => return,
            },
            BrowserPage::Maps => match key {
                Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => {
                    self.map = self.map.checked_sub(1).unwrap_or(self.maps.len() - 1);
                }
                Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => {
                    self.map = (self.map + 1) % self.maps.len();
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
            BrowserPage::Images => match key {
                Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => {
                    self.image = self.image.checked_sub(1).unwrap_or(self.images.len() - 1);
                }
                Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => {
                    self.image = (self.image + 1) % self.images.len();
                }
                _ => return,
            },
            BrowserPage::Roster => match key {
                Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowDown) => {
                    self.character = self
                        .character
                        .checked_sub(1)
                        .unwrap_or(self.characters.len() - 1);
                }
                Key::Named(NamedKey::ArrowRight | NamedKey::ArrowUp) => {
                    self.character = (self.character + 1) % self.characters.len();
                }
                _ => return,
            },
        }
        self.redraw_framebuffer();
    }

    fn draw_character(&mut self) {
        let character = &self.characters[self.character];
        let label = |value, names: &[&str]| {
            names
                .get(value as usize)
                .filter(|name| !name.is_empty())
                .map(|name| (*name).to_owned())
                .unwrap_or_else(|| format!("UNKNOWN {value}"))
        };
        let lines = [
            format!(
                "{}   {} / {}",
                character.name,
                self.character + 1,
                self.characters.len()
            ),
            format!(
                "{}  {}  {}  {}",
                label(character.sex, &["", "MALE", "FEMALE"]),
                label(
                    character.current_alignment,
                    &["", "GOOD", "NEUTRAL", "EVIL"]
                ),
                label(
                    character.race,
                    &["", "HUMAN", "ELF", "DWARF", "GNOME", "HALF-ORC"]
                ),
                label(
                    character.class,
                    &[
                        "", "KNIGHT", "PALADIN", "ARCHER", "CLERIC", "SORCERER", "ROBBER"
                    ]
                )
            ),
            format!(
                "LEVEL {:3}   AGE {:3}   CONDITION {}",
                character.level.current, character.age, character.condition
            ),
            format!(
                "INT {:2}  MGT {:2}  PER {:2}  END {:2}",
                character.intellect.current,
                character.might.current,
                character.personality.current,
                character.endurance.current
            ),
            format!(
                "SPD {:2}  ACY {:2}  LCK {:2}  AC {:2}",
                character.speed.current,
                character.accuracy.current,
                character.luck.current,
                character.armor_class.current
            ),
            format!(
                "HP {:3} / {:3}   SP {:3} / {:3}",
                character.current_hp,
                character.effective_max_hp,
                character.current_spell_points,
                character.maximum_spell_points
            ),
            format!("EXPERIENCE {}", character.experience),
            format!(
                "GOLD {}   GEMS {}   FOOD {}",
                character.gold, character.gems, character.food
            ),
        ];
        for (index, line) in lines.iter().enumerate() {
            draw_dos_text(
                &mut self.framebuffer,
                8,
                12 + index as u32 * 20,
                line,
                if index == 0 { WHITE } else { CYAN },
            );
        }
        draw_dos_text(
            &mut self.framebuffer,
            60,
            184,
            "ARROWS: PREVIOUS / NEXT",
            WHITE,
        );
    }

    fn draw_map(&mut self) {
        let map = &self.maps[self.map];
        let title = format!("{}  {:02} / 55", MAP_NAMES[self.map], self.map + 1);
        draw_dos_text(&mut self.framebuffer, 48, 10, &title, WHITE);
        draw_map(&mut self.framebuffer, map, 96, 28);
        draw_dos_text(&mut self.framebuffer, 36, 164, "WALL", WHITE);
        draw_dos_text(&mut self.framebuffer, 92, 164, "DOOR", CYAN);
        draw_dos_text(&mut self.framebuffer, 148, 164, "SPECIAL", MAGENTA);
        draw_dos_text(
            &mut self.framebuffer,
            60,
            184,
            "ARROWS: PREVIOUS / NEXT",
            CYAN,
        );
    }
}

fn decode_maps(data: &[u8]) -> Result<Vec<Map>, String> {
    if data.len() != 55 * 512 {
        return Err(format!(
            "MAZEDATA.DTA must contain 55 512-byte records, found {} bytes",
            data.len()
        ));
    }
    Ok(data
        .chunks_exact(512)
        .map(|record| Map {
            walls: record[..256].try_into().unwrap(),
            properties: record[256..].try_into().unwrap(),
        })
        .collect())
}

fn draw_map(frame: &mut [u8], map: &Map, origin_x: u32, origin_y: u32) {
    const CELL: u32 = 8;
    for display_y in 0..16 {
        let source_y = 15 - display_y;
        for x in 0..16 {
            let value = map.walls[source_y * 16 + x];
            let px = origin_x + x as u32 * CELL;
            let py = origin_y + display_y as u32 * CELL;
            let color = |shift: u32| match (value >> shift) & 3u8 {
                1 => WHITE,
                2 => CYAN,
                3 => MAGENTA,
                _ => BLACK,
            };
            if color(6) != 0 {
                fill_rect(frame, px + 1, py + 1, 6, 1, color(6));
            }
            if color(4) != 0 {
                fill_rect(frame, px + 6, py + 1, 1, 6, color(4));
            }
            if color(2) != 0 {
                fill_rect(frame, px + 1, py + 6, 6, 1, color(2));
            }
            if color(0) != 0 {
                fill_rect(frame, px + 1, py + 1, 1, 6, color(0));
            }
        }
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

fn decode_screen(data: &[u8]) -> Result<Vec<u8>, String> {
    let compressed_size = read_u16(data, 0)? as usize;
    let payload_end = 2usize
        .checked_add(compressed_size)
        .ok_or("SCREEN image size overflow")?;
    if payload_end != data.len() {
        return Err("SCREEN image compressed size does not match its file size".into());
    }
    let packed = decode_rle(&data[2..payload_end], 16_000, "SCREEN image")?;
    Ok(unpack_image(&packed, WIDTH as usize, HEIGHT as usize))
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
    animation: TitleAnimation,
    next_update: Instant,
    _title_music: Option<TitleMusic>,
    game: game::Game,
    game_framebuffer: Vec<u8>,
    walls: Vec<WallSet>,
    monsters: Vec<Vec<u8>>,
    modifiers: ModifiersState,
    save_path: PathBuf,
}

impl GameWindow {
    fn new(
        animation: TitleAnimation,
        title_music: Option<TitleMusic>,
        game: game::Game,
        save_path: PathBuf,
    ) -> Self {
        let walls = decode_wall_sets(&fs::read("dos/WALLPIX.DTA").expect("could not load walls"))
            .expect("could not decode walls");
        let monsters =
            decode_monsters(&fs::read("dos/MONPIX.DTA").expect("could not load monsters"))
                .expect("could not decode monsters");
        let mut window = Self {
            window: None,
            pixels: None,
            animation,
            next_update: Instant::now() + TITLE_RING_INTERVAL,
            _title_music: title_music,
            game,
            game_framebuffer: vec![BLACK; (WIDTH * HEIGHT) as usize],
            walls,
            monsters,
            modifiers: ModifiersState::empty(),
            save_path,
        };
        if window.game.screen != game::Screen::Title {
            window.redraw_game();
        }
        window
    }

    fn apply_command(&mut self, command: &str) {
        if let Err(error) = apply_and_save(&mut self.game, command, &self.save_path) {
            eprintln!("could not autosave game: {error}");
        }
    }

    fn key_pressed(&mut self, key: &Key) -> bool {
        if self.game.screen != game::Screen::Title {
            let command = match key {
                Key::Named(NamedKey::Escape) => Some("escape"),
                Key::Named(NamedKey::Enter | NamedKey::Space) => Some(match self.game.screen {
                    game::Screen::Encounter => "fight",
                    game::Screen::Treasure => "open",
                    _ => "confirm",
                }),
                Key::Named(NamedKey::ArrowUp) => Some("forward"),
                Key::Named(NamedKey::ArrowDown) => Some("back"),
                Key::Named(NamedKey::ArrowLeft) => Some("left"),
                Key::Named(NamedKey::ArrowRight) => Some("right"),
                Key::Character(c) if c.len() == 1 && c.as_bytes()[0].is_ascii_digit() => {
                    let command = match self.game.screen {
                        game::Screen::Inn => "toggle:",
                        game::Screen::CreateCharacter => "choose:",
                        game::Screen::Town => "view:",
                        game::Screen::Blacksmith => self.game.blacksmith_number_action(),
                        game::Screen::Combat => "attack:",
                        _ => "choose:",
                    };
                    self.apply_command(&format!("{command}{c}"));
                    None
                }
                Key::Character(c) => match c.to_ascii_lowercase().as_str() {
                    "c" if self.game.screen == game::Screen::Menu => Some("create"),
                    "v" if self.game.screen == game::Screen::Menu => Some("view"),
                    "m" if self.game.screen == game::Screen::Menu => Some("enter"),
                    letter @ ("a" | "b" | "c" | "d" | "e" | "f")
                        if self.game.screen == game::Screen::Roster =>
                    {
                        let index = letter.as_bytes()[0] - b'a' + 1;
                        self.apply_command(&format!("view-roster:{index}"));
                        None
                    }
                    letter @ ("a" | "b" | "c" | "d" | "e" | "f")
                        if self.game.screen == game::Screen::Inn =>
                    {
                        let index = letter.as_bytes()[0] - b'a' + 1;
                        if self.modifiers.control_key() {
                            self.apply_command(&format!("toggle:{index}"));
                        } else {
                            self.apply_command(&format!("view-inn:{index}"));
                        }
                        None
                    }
                    "k" if self.game.screen == game::Screen::Inn => Some("escape"),
                    "f" if matches!(
                        self.game.screen,
                        game::Screen::Encounter | game::Screen::Combat
                    ) =>
                    {
                        Some("flee")
                    }
                    "f" => Some("food"),
                    "c" if self.game.screen == game::Screen::Combat => Some("cast"),
                    "d" if self.game.screen == game::Screen::Combat => Some("defend"),
                    "a" if self.game.screen == game::Screen::Blacksmith => Some("armor"),
                    "c" if self.game.screen == game::Screen::Blacksmith => Some("choose-member"),
                    "s" if self.game.screen == game::Screen::Blacksmith => Some("sell-item"),
                    "o" if self.game.screen == game::Screen::Treasure => Some("open"),
                    "l" if self.game.screen == game::Screen::Treasure => Some("leave"),
                    "d" => Some("drink"),
                    "t" if self.game.screen == game::Screen::Tavern => Some("tip"),
                    "t" => Some("train"),
                    "u" if self.game.screen == game::Screen::Town => Some("unlock"),
                    "u" => Some("rumor"),
                    "g" => Some("gather"),
                    "o" => Some("donate"),
                    "h" => Some("restore"),
                    "a" if self.game.screen == game::Screen::Temple => Some("realign"),
                    "w" => Some("weapons"),
                    "m" => Some("misc"),
                    "b" => Some("bash"),
                    "y" => Some("yes"),
                    "n" => Some("no"),
                    _ => None,
                },
                _ => None,
            };
            if let Some(command) = command {
                self.apply_command(command);
            }
            self.redraw_game();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return false;
        }
        if key == &Key::Named(NamedKey::Escape)
            || key == &Key::Named(NamedKey::Space)
            || key == &Key::Named(NamedKey::Enter)
        {
            self.apply_command("start");
            self.redraw_game();
            if let Some(window) = &self.window {
                window.request_redraw();
            }
            return false;
        } else if key == &Key::Character("q".into()) {
            return true;
        } else if key == &Key::Named(NamedKey::Tab) {
            if self.animation.in_slideshow() {
                self.animation.advance_slideshow();
            } else {
                self.animation.start_slideshow();
            }
            self.next_update = Instant::now()
                + if self.animation.in_slideshow() {
                    SLIDESHOW_INTERVAL
                } else {
                    TITLE_RING_INTERVAL
                };
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }

        false
    }

    fn redraw_game(&mut self) {
        self.game_framebuffer.fill(BLACK);
        let view = self.game.view();
        if matches!(
            self.game.screen,
            game::Screen::Town
                | game::Screen::Encounter
                | game::Screen::Combat
                | game::Screen::Treasure
        ) {
            draw_exploration(&mut self.game_framebuffer, &self.game, &self.walls[..3]);
            if let Some(combat) = &view.combat {
                if let Some(enemy) = combat.enemies.iter().find(|enemy| enemy.alive)
                    && let Some(image) = self.monsters.get(enemy.image as usize)
                {
                    blit_image(&mut self.game_framebuffer, image, 104, 96, 68, 16);
                }
                draw_combat_panel(&mut self.game_framebuffer, combat, self.game.screen);
            } else {
                draw_command_panel(&mut self.game_framebuffer);
            }
            draw_party(&mut self.game_framebuffer, &view.party);
            draw_message(&mut self.game_framebuffer, view.message);
            return;
        }
        if self.game.screen == game::Screen::Character {
            if let Some(character) = self.game.current_character() {
                draw_character_sheet(&mut self.game_framebuffer, &self.game, character);
            }
            return;
        }

        match self.game.screen {
            game::Screen::Menu => draw_main_menu(&mut self.game_framebuffer),
            game::Screen::CreateCharacter => draw_create_character(&mut self.game_framebuffer),
            game::Screen::Roster => draw_roster(&mut self.game_framebuffer, self.game.roster()),
            game::Screen::RosterCharacter | game::Screen::InnCharacter => {
                if let Some(character) = self.game.current_character() {
                    draw_character_sheet(&mut self.game_framebuffer, &self.game, character);
                }
            }
            game::Screen::Inn => draw_inn(&mut self.game_framebuffer, &view.options),
            _ => {
                draw_generic_screen(&mut self.game_framebuffer, &view, self.game.screen);
            }
        }
    }
}

fn draw_generic_screen(frame: &mut [u8], view: &game::PlayerView<'_>, screen: game::Screen) {
    draw_dos_text(frame, 8, 8, view.title, WHITE);
    let mut y = 24;
    let option_limit = if screen == game::Screen::Blacksmith {
        11
    } else {
        8
    };
    for option in view.options.iter().take(option_limit) {
        draw_dos_text(frame, 8, y, option, CYAN);
        y += 11;
    }
    for member in view.party.iter().take(6) {
        if screen == game::Screen::Blacksmith && !member.is_current {
            continue;
        }
        draw_dos_text(
            frame,
            8,
            y,
            &format!(
                "{}{} L{} HP{} XP{} G{} GM{} C{:02X}",
                if member.is_current { "*" } else { " " },
                member.name,
                member.level,
                member.hp,
                member.experience,
                member.gold,
                member.gems,
                member.condition
            ),
            MAGENTA,
        );
        y += 10;
    }
    for line in view.message.as_bytes().chunks(39) {
        draw_dos_text(frame, 8, y.min(184), &String::from_utf8_lossy(line), WHITE);
        y += 10;
    }
}

fn draw_frame(frame: &mut [u8], title: &str) {
    draw_dos_text(frame, 24, 6, "----------------------------------", WHITE);
    draw_dos_text(frame, 24, 183, "----------------------------------", WHITE);
    // 2x2 solid block glyphs at each corner of the frame.
    fill_rect(frame, 8, 0, 16, 16, WHITE);
    fill_rect(frame, 296, 0, 16, 16, WHITE);
    fill_rect(frame, 8, 176, 16, 16, WHITE);
    fill_rect(frame, 296, 176, 16, 16, WHITE);
    // Vertical borders drawn as '!' character glyphs, matching the original
    // DOS text-mode frame.  Each '!' is an 8x8 cell; 20 cells span the 160
    // pixels between the top and bottom corner blocks.
    for row in 0..20u32 {
        draw_dos_text(frame, 8, 16 + row * 8, "!", WHITE);
        draw_dos_text(frame, 304, 16 + row * 8, "!", WHITE);
    }
    draw_centered_text(frame, 0, title, WHITE);
    draw_centered_text(frame, 192, "'ESC' TO GO BACK", WHITE);
}

fn draw_centered_text(frame: &mut [u8], y: u32, text: &str, color: u8) {
    let width = text.chars().count() as u32 * 8;
    draw_dos_text(frame, (WIDTH.saturating_sub(width)) / 2, y, text, color);
}

fn draw_main_menu(frame: &mut [u8]) {
    draw_frame(frame, "");
    draw_centered_text(frame, 32, "MIGHT AND MAGIC", WHITE);
    draw_centered_text(frame, 48, "SECRET OF THE INNER SANCTUM", WHITE);
    draw_centered_text(frame, 72, "OPTIONS", WHITE);
    draw_dos_text(frame, 132, 80, "-------", WHITE);
    draw_dos_text(frame, 40, 94, "'C'........CREATE NEW CHARACTERS", WHITE);
    draw_dos_text(frame, 40, 110, "'V'........VIEW ALL CHARACTERS", WHITE);
    draw_dos_text(frame, 40, 126, "'M'........GO TO TOWN", WHITE);
    draw_centered_text(frame, 176, "COPR. 1986,1987-JON VAN CANEGHEM", WHITE);
    // Clear the "'ESC' TO GO BACK" line drawn by draw_frame before writing
    // the copyright continuation so the two texts do not overlap.
    fill_rect(frame, 0, 192, WIDTH, 8, BLACK);
    draw_centered_text(frame, 192, "ALL RIGHTS RESERVED", WHITE);
}

fn draw_create_character(frame: &mut [u8]) {
    draw_frame(frame, "CREATE NEW CHARACTERS");
    for (row, text) in [
        "INTELLECT....= 14",
        "MIGHT........= 11",
        "PERSONALITY..= 13",
        "ENDURANCE....= 10",
        "SPEED........= 10",
        "ACCURACY.....= 11",
        "LUCK.........= 12",
    ]
    .iter()
    .enumerate()
    {
        draw_dos_text(frame, 24, 40 + row as u32 * 16, text, WHITE);
    }
    for (row, text) in ["4) CLERIC", "5) SORCERER", "6) ROBBER"].iter().enumerate() {
        draw_dos_text(frame, 184, 64 + row as u32 * 10, text, WHITE);
    }
    draw_dos_text(frame, 176, 136, "SELECT A CLASS", WHITE);
    draw_dos_text(frame, 208, 152, "(1-6)", WHITE);
    draw_dos_text(frame, 168, 168, "'ENT' TO RE-ROLL", WHITE);
}

fn class_name(class: u8) -> &'static str {
    [
        "", "KNIGHT", "PALADIN", "ARCHER", "CLERIC", "SORCERER", "ROBBER",
    ]
    .get(class as usize)
    .copied()
    .unwrap_or("UNKNOWN")
}

fn draw_roster(frame: &mut [u8], roster: &[Character]) {
    draw_frame(frame, "VIEW ALL CHARACTERS");
    for (index, character) in roster.iter().take(6).enumerate() {
        let dots = ".".repeat(16usize.saturating_sub(character.name.len()));
        draw_dos_text(
            frame,
            24,
            24 + index as u32 * 10,
            &format!(
                "{}) {}{}({})L{}  {}",
                (b'A' + index as u8) as char,
                character.name,
                dots,
                character.sex,
                character.level.current,
                class_name(character.class)
            ),
            WHITE,
        );
    }
    draw_centered_text(frame, 176, "'A'-'F' TO VIEW A CHARACTER", WHITE);
}

fn draw_inn(frame: &mut [u8], options: &[String]) {
    draw_frame(frame, "(1) INN OF SORPIGAL");
    draw_centered_text(frame, 24, "AVAILABLE CHARACTERS", WHITE);
    let mut full = false;
    for (index, option) in options.iter().take(6).enumerate() {
        let selected = option.ends_with(" [IN PARTY]");
        full |= selected
            && options
                .iter()
                .take(6)
                .filter(|line| line.ends_with(" [IN PARTY]"))
                .count()
                == 6;
        let name = option
            .split_once(' ')
            .map(|(_, name)| name.trim_end_matches(" [IN PARTY]"))
            .unwrap_or(option);
        draw_dos_text(
            frame,
            24,
            48 + index as u32 * 10,
            &format!(
                "{}{}){}",
                if selected { "@" } else { "" },
                (b'A' + index as u8) as char,
                name
            ),
            WHITE,
        );
    }
    if full {
        draw_centered_text(frame, 128, "*** PARTY IS FULL ***", WHITE);
    }
    draw_centered_text(frame, 152, "'A'-'F' TO VIEW", WHITE);
    draw_centered_text(frame, 160, "(CTRL)-'A'-'F' ADD/REMOVE", WHITE);
    draw_centered_text(frame, 176, "'K' EXIT INN", WHITE);
}

fn draw_exploration(frame: &mut [u8], game: &game::Game, wall_sets: &[WallSet]) {
    const LEFT: [(usize, usize); 4] = [(0, 0), (32, 16), (72, 32), (96, 48)];
    const RIGHT: [(usize, usize); 4] = [(208, 0), (168, 16), (144, 32), (128, 48)];
    const FRONT: [(usize, usize); 4] = [(32, 16), (72, 32), (96, 48), (112, 56)];

    for (depth, cell) in game.perspective().iter().enumerate() {
        if cell.left != 0
            && let Some(wall) = wall_sets.get(cell.left as usize - 1)
        {
            blit_component(frame, wall, depth, LEFT[depth]);
        }
        if cell.right != 0
            && let Some(wall) = wall_sets.get(cell.right as usize - 1)
        {
            blit_component(frame, wall, depth + 4, RIGHT[depth]);
        }
        if cell.front != 0 {
            if let Some(wall) = wall_sets.get(cell.front as usize - 1) {
                blit_component(frame, wall, depth + 8, FRONT[depth]);
            }
            break;
        }
    }
    fill_rect(frame, 240, 0, 1, 128, CYAN);
    fill_rect(frame, 0, 128, WIDTH, 1, CYAN);
}

fn blit_component(frame: &mut [u8], wall: &WallSet, component: usize, position: (usize, usize)) {
    let (width, height) = WALL_COMPONENT_DIMENSIONS[component];
    blit_image(
        frame,
        &wall.components[component],
        width,
        height,
        position.0,
        position.1,
    );
}

fn blit_image(frame: &mut [u8], image: &[u8], width: usize, height: usize, x: usize, y: usize) {
    for row in 0..height.min(HEIGHT as usize - y) {
        let source = &image[row * width..(row + 1) * width];
        let start = (y + row) * WIDTH as usize + x;
        let visible = width.min(WIDTH as usize - x);
        frame[start..start + visible].copy_from_slice(&source[..visible]);
    }
}

fn draw_command_panel(frame: &mut [u8]) {
    draw_dos_text(frame, 248, 2, "COMMANDS", WHITE);
    for &(y, command) in &[
        (18, "^ FORWARD"),
        (31, "V BACK"),
        (44, "< TURN"),
        (54, "  LEFT"),
        (67, "> TURN"),
        (77, "  RIGHT"),
        (90, "U UNLOCK"),
        (103, "B BASH"),
        (116, "1-6 VIEW"),
    ] {
        draw_dos_text(frame, 248, y, command, WHITE);
    }
}

fn draw_character_sheet(frame: &mut [u8], game: &game::Game, character: &Character) {
    let sex = ["", "M", "F"]
        .get(character.sex as usize)
        .copied()
        .unwrap_or("?");
    let alignment = ["", "GOOD", "NEUT", "EVIL"]
        .get(character.current_alignment as usize)
        .copied()
        .unwrap_or("?");
    let race = ["", "HUMAN", "ELF", "DWARF", "GNOME", "HALF-ORC"]
        .get(character.race as usize)
        .copied()
        .unwrap_or("?");
    draw_dos_text(
        frame,
        0,
        0,
        &format!(
            "{}  : {} {} {} {}",
            character.name,
            sex,
            alignment,
            race,
            class_name(character.class)
        ),
        WHITE,
    );
    for (row, line) in [
        format!(
            "INT={:<2}   LEVEL={:<2}  AGE={:<3}  EXP={}",
            character.intellect.current,
            character.level.current,
            character.age,
            character.experience
        ),
        format!("MGT={:<2}", character.might.current),
        format!(
            "PER={:<2}   SP={:<3}   /0    (0) GEMS={}",
            character.personality.current, character.current_spell_points, character.gems
        ),
        format!("END={:<2}", character.endurance.current),
        format!(
            "SPD={:<2}   HP={:<3}   /{:<3}      GOLD={}",
            character.speed.current,
            character.current_hp,
            character.effective_max_hp,
            character.gold
        ),
        format!("ACY={:<2}", character.accuracy.current),
        format!(
            "LCK={:<2}   AC={:<2}             FOOD={}",
            character.luck.current, character.armor_class.current, character.food
        ),
        String::new(),
        format!(
            "COND= {}",
            if character.condition == 0 {
                "GOOD"
            } else {
                "BAD"
            }
        ),
    ]
    .iter()
    .enumerate()
    {
        draw_dos_text(frame, 0, 16 + row as u32 * 10, line, WHITE);
    }
    draw_dos_text(frame, 48, 112, "<EQUIPPED>--------><BACKPACK>", WHITE);
    for slot in 0..6 {
        let equipped = game.item_name(character.equipped_items[slot]);
        let backpack = game.item_name(character.backpack_items[slot]);
        if !equipped.is_empty() {
            draw_dos_text(
                frame,
                0,
                128 + slot as u32 * 10,
                &format!("{}) {}", slot + 1, equipped),
                WHITE,
            );
        } else {
            draw_dos_text(
                frame,
                0,
                128 + slot as u32 * 10,
                &format!("{})", slot + 1),
                WHITE,
            );
        }
        if !backpack.is_empty() {
            draw_dos_text(
                frame,
                176,
                128 + slot as u32 * 10,
                &format!("{}) {}", (b'A' + slot as u8) as char, backpack),
                WHITE,
            );
        } else {
            draw_dos_text(
                frame,
                176,
                128 + slot as u32 * 10,
                &format!("{})", (b'A' + slot as u8) as char),
                WHITE,
            );
        }
    }
    draw_centered_text(frame, 168, "(CTRL)-'N' RE-NAME CHARACTER", WHITE);
    draw_centered_text(frame, 178, "(CTRL)-'D' DELETE CHARACTER", WHITE);
    draw_centered_text(frame, 192, "'ESC' TO GO BACK", WHITE);
}

fn draw_combat_panel(frame: &mut [u8], combat: &game::CombatView<'_>, screen: game::Screen) {
    let heading = if screen == game::Screen::Treasure {
        "TREASURE"
    } else {
        "ENEMIES"
    };
    draw_dos_text(frame, 248, 2, heading, WHITE);
    for (row, enemy) in combat
        .enemies
        .iter()
        .filter(|enemy| enemy.alive)
        .take(8)
        .enumerate()
    {
        draw_dos_text(
            frame,
            248,
            16 + row as u32 * 10,
            &format!("{} {}", enemy.slot, enemy.name),
            WHITE,
        );
    }
    let options: &[&str] = match screen {
        game::Screen::Encounter => &["ENTER", "F FLEE"],
        game::Screen::Combat => &["1-9 ATK", "D DEFEND", "F FLEE"],
        game::Screen::Treasure => &["O OPEN", "L LEAVE"],
        _ => &[],
    };
    for (row, option) in options.iter().enumerate() {
        draw_dos_text(frame, 248, 98 + row as u32 * 10, option, WHITE);
    }
}

fn draw_party(frame: &mut [u8], party: &[game::PartyMember<'_>]) {
    for (index, member) in party.iter().take(6).enumerate() {
        let x = if index % 2 == 0 { 8 } else { 176 };
        let y = 136 + (index / 2) as u32 * 10;
        draw_dos_text(
            frame,
            x,
            y,
            &format!("{}) {}", index + 1, member.name),
            if member.condition == 0 {
                WHITE
            } else {
                MAGENTA
            },
        );
    }
}

fn draw_message(frame: &mut [u8], message: &str) {
    if message.is_empty() {
        return;
    }
    fill_rect(frame, 0, 168, WIDTH, 32, BLACK);
    fill_rect(frame, 0, 168, WIDTH, 1, CYAN);
    for (row, line) in message.as_bytes().chunks(39).take(3).enumerate() {
        draw_dos_text(
            frame,
            4,
            172 + row as u32 * 9,
            &String::from_utf8_lossy(line),
            WHITE,
        );
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

        window.request_redraw();
        self.pixels = Some(pixels);
        self.window = Some(window);
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_update));
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
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if is_quit_shortcut(&event.logical_key, self.modifiers)
                    || self.key_pressed(&event.logical_key)
                {
                    event_loop.exit();
                }
            }
            WindowEvent::Resized(PhysicalSize { width, height }) if width > 0 && height > 0 => {
                if let Err(error) = pixels.resize_surface(width, height) {
                    eprintln!("could not resize the window surface: {error}");
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => {
                let frame = if self.game.screen == game::Screen::Title {
                    self.animation.framebuffer()
                } else {
                    &self.game_framebuffer
                };
                let palette = if self.game.screen == game::Screen::Title {
                    &TITLE_EGA_PALETTE
                } else {
                    &GAME_EGA_PALETTE
                };
                copy_to_rgba_with_palette(frame, pixels.frame_mut(), palette);
                if let Err(error) = pixels.render() {
                    eprintln!("could not render the title screen: {error}");
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if now >= self.next_update {
            if self.animation.in_slideshow() {
                self.animation.advance_slideshow();
            } else {
                self.animation.advance_title();
            }
            self.next_update = now
                + if self.animation.in_slideshow() {
                    SLIDESHOW_INTERVAL
                } else {
                    TITLE_RING_INTERVAL
                };
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_update));
    }
}

struct TitleAnimation {
    images: Vec<Vec<u8>>,
    framebuffer: Vec<u8>,
    image: usize,
    ring: u32,
    scene: Option<usize>,
}

impl TitleAnimation {
    fn load() -> Result<Self, Box<dyn Error>> {
        let mut images = Vec::new();
        for index in 0..=LAST_SCENE {
            images.push(decode_screen(&fs::read(format!("dos/SCREEN{index}"))?)?);
        }
        let mut animation = Self {
            images,
            framebuffer: vec![BLACK; (WIDTH * HEIGHT) as usize],
            image: 0,
            ring: 0,
            scene: None,
        };
        animation.advance_title();
        Ok(animation)
    }

    fn framebuffer(&self) -> &[u8] {
        &self.framebuffer
    }

    fn in_slideshow(&self) -> bool {
        self.scene.is_some()
    }

    fn start_slideshow(&mut self) {
        self.scene = Some(FIRST_SCENE);
        self.framebuffer.copy_from_slice(&self.images[FIRST_SCENE]);
    }

    fn advance_slideshow(&mut self) {
        let Some(scene) = self.scene else {
            return;
        };
        if scene < LAST_SCENE {
            let scene = scene + 1;
            self.scene = Some(scene);
            self.framebuffer.copy_from_slice(&self.images[scene]);
        } else {
            self.scene = None;
            self.image = 0;
            self.ring = 0;
            self.advance_title();
        }
    }

    fn advance_title(&mut self) {
        let inset_x = self.ring * 8;
        let inset_y = self.ring * 5;
        let width = WIDTH - inset_x * 2;
        let side_height = HEIGHT - (inset_y + 5) * 2;
        let image = &self.images[self.image];

        copy_rect(image, &mut self.framebuffer, inset_x, inset_y, width, 5);

        if side_height > 0 {
            copy_rect(
                image,
                &mut self.framebuffer,
                WIDTH - inset_x - 8,
                inset_y + 5,
                8,
                side_height,
            );
        }
        copy_rect(
            image,
            &mut self.framebuffer,
            inset_x,
            HEIGHT - inset_y - 5,
            width,
            5,
        );
        if side_height > 0 {
            copy_rect(
                image,
                &mut self.framebuffer,
                inset_x,
                inset_y + 5,
                8,
                side_height,
            );
        }

        self.ring += 1;
        if self.ring == TITLE_RING_COUNT {
            self.ring = 0;
            self.image = (self.image + 1) % FIRST_SCENE;
        }
    }
}

fn copy_rect(source: &[u8], destination: &mut [u8], x: u32, y: u32, width: u32, height: u32) {
    for row in y..y + height {
        let start = (row * WIDTH + x) as usize;
        let end = start + width as usize;
        destination[start..end].copy_from_slice(&source[start..end]);
    }
}

fn copy_to_rgba(indexed: &[u8], rgba: &mut [u8]) {
    copy_to_rgba_with_palette(indexed, rgba, &PALETTE);
}

fn copy_to_rgba_with_palette(indexed: &[u8], rgba: &mut [u8], palette: &[[u8; 4]; 4]) {
    for (color, pixel) in indexed.iter().zip(rgba.chunks_exact_mut(4)) {
        pixel.copy_from_slice(&palette[*color as usize]);
    }
}

fn fill_rect(frame: &mut [u8], x: u32, y: u32, width: u32, height: u32, color: u8) {
    if x >= WIDTH || y >= HEIGHT {
        return;
    }
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
    let character = character.to_ascii_uppercase();
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
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x30, 0x30, 0x00],
        ',' => [0x00, 0x00, 0x00, 0x00, 0x30, 0x30, 0x60, 0x00],
        '\'' => [0x18, 0x18, 0x18, 0x18, 0x00, 0x00, 0x00, 0x00],
        '(' => [0x0c, 0x18, 0x30, 0x30, 0x30, 0x18, 0x0c, 0x00],
        ')' => [0x60, 0x30, 0x18, 0x18, 0x18, 0x30, 0x60, 0x00],
        '=' => [0x00, 0x00, 0xfc, 0x00, 0xfc, 0x00, 0x00, 0x00],
        '*' => [0x00, 0x66, 0x3c, 0xff, 0x3c, 0x66, 0x00, 0x00],
        '@' => [0x7c, 0xc6, 0xde, 0xde, 0xde, 0xc0, 0x78, 0x00],
        '!' => [0x18, 0x18, 0x18, 0x18, 0x00, 0x18, 0x18, 0x00],
        '?' => [0x78, 0xcc, 0x0c, 0x18, 0x30, 0x00, 0x30, 0x00],
        '-' => [0x00, 0x00, 0x00, 0xfc, 0x00, 0x00, 0x00, 0x00],
        '/' => [0x06, 0x0c, 0x18, 0x30, 0x60, 0xc0, 0x80, 0x00],
        '^' => [0x10, 0x38, 0x6c, 0xc6, 0x00, 0x00, 0x00, 0x00],
        '<' => [0x0c, 0x18, 0x30, 0x60, 0x30, 0x18, 0x0c, 0x00],
        '>' => [0x60, 0x30, 0x18, 0x0c, 0x18, 0x30, 0x60, 0x00],
        _ => [0; 8],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_animation_uses_the_logical_screen_dimensions() {
        let animation = TitleAnimation::load().unwrap();
        assert_eq!(animation.framebuffer().len(), (WIDTH * HEIGHT) as usize);
    }

    #[test]
    fn title_animation_reveals_screen_zero_then_screen_one() {
        let mut animation = TitleAnimation::load().unwrap();

        for _ in 1..TITLE_RING_COUNT {
            animation.advance_title();
        }
        assert_eq!(animation.image, 1);
        assert_eq!(animation.framebuffer, animation.images[0]);

        for _ in 0..TITLE_RING_COUNT {
            animation.advance_title();
        }
        assert_eq!(animation.image, 0);
        assert_eq!(animation.framebuffer, animation.images[1]);
    }

    #[test]
    fn slideshow_shows_screens_two_through_nine_then_returns_to_title() {
        let mut animation = TitleAnimation::load().unwrap();

        animation.start_slideshow();
        for scene in FIRST_SCENE..=LAST_SCENE {
            assert_eq!(animation.scene, Some(scene));
            assert_eq!(animation.framebuffer, animation.images[scene]);
            animation.advance_slideshow();
        }

        assert!(!animation.in_slideshow());
        assert_eq!(animation.image, 0);
    }

    #[test]
    fn title_keys_start_and_advance_the_slideshow_or_exit() {
        let save_path = temporary_save_path("title-keys");
        let mut window = GameWindow::new(
            TitleAnimation::load().unwrap(),
            None,
            game::Game::load().unwrap(),
            save_path.clone(),
        );

        assert!(!window.key_pressed(&Key::Named(NamedKey::Space)));
        assert_eq!(window.game.screen, game::Screen::Menu);
        let mut window = GameWindow::new(
            TitleAnimation::load().unwrap(),
            None,
            game::Game::load().unwrap(),
            save_path.clone(),
        );
        assert!(!window.key_pressed(&Key::Named(NamedKey::Escape)));
        assert_eq!(window.game.screen, game::Screen::Menu);
        fs::remove_file(save_path).unwrap();
    }

    #[test]
    fn title_view_serializes_as_versioned_json() {
        let game = game::Game::load().unwrap();
        let value = serde_json::to_value(game.view()).unwrap();
        assert_eq!(value["schema_version"], 2);
        assert_eq!(value["kind"], "title");
    }

    #[test]
    fn interactive_headless_emits_one_json_view_per_command() {
        let input = b"start\n\ntoggle:1\nconfirm\n";
        let mut output = Vec::new();
        let save_path = temporary_save_path("headless");

        run_headless_io(&[], true, &input[..], &mut output, &save_path).unwrap();

        let views: Vec<serde_json::Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(views.len(), 4);
        assert_eq!(views[0]["kind"], "title");
        assert_eq!(views[1]["kind"], "menu");
        assert_eq!(views[2]["kind"], "inn");
        assert_eq!(views[3]["kind"], "town");
        fs::remove_file(save_path).unwrap();
    }

    #[test]
    fn reset_removes_a_saved_game_so_the_next_load_starts_fresh() {
        let save_path = temporary_save_path("reset");
        let mut game = game::Game::load().unwrap();
        game.command("start");
        game.save(&save_path).unwrap();
        assert!(save_path.exists(), "save game should exist before reset");

        reset_save_game(&save_path).unwrap();
        assert!(!save_path.exists(), "save game should be removed by reset");

        let reloaded = game::Game::load_or_new(&save_path).unwrap();
        assert_eq!(reloaded.view().kind, game::Screen::Title);
    }

    #[test]
    fn reset_is_a_noop_when_no_save_game_exists() {
        let save_path = temporary_save_path("reset-missing");
        assert!(!save_path.exists());
        reset_save_game(&save_path).unwrap();
    }

    fn temporary_save_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mm1-{name}-{}.json", std::process::id()))
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
    fn supplied_screen_files_decode_as_full_screen_images() {
        for index in 0..10 {
            let data = fs::read(format!("dos/SCREEN{index}")).unwrap();
            let image = decode_screen(&data).unwrap();

            assert_eq!(image.len(), (WIDTH * HEIGHT) as usize);
            assert!(image.iter().all(|pixel| *pixel < 4));
        }
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
        let mut browser = AssetBrowser::new(
            blank_images(1),
            vec![vec![BLACK; 104 * 96]; 2],
            vec![blank_wall_set()],
            test_characters(),
            blank_maps(),
        );
        browser.page = BrowserPage::Monsters;

        browser.key_pressed(&Key::Named(NamedKey::ArrowLeft));
        assert_eq!(browser.monster, 1);
        browser.key_pressed(&Key::Named(NamedKey::ArrowRight));
        assert_eq!(browser.monster, 0);
    }

    #[test]
    fn monster_browser_up_is_next_down_is_previous() {
        let mut browser = AssetBrowser::new(
            blank_images(1),
            vec![vec![BLACK; 104 * 96]; 3],
            vec![blank_wall_set()],
            test_characters(),
            blank_maps(),
        );
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
            blank_images(1),
            vec![vec![BLACK; 104 * 96]],
            vec![blank_wall_set(), blank_wall_set()],
            test_characters(),
            blank_maps(),
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
            blank_images(1),
            vec![vec![BLACK; 104 * 96]],
            vec![blank_wall_set(), blank_wall_set(), blank_wall_set()],
            test_characters(),
            blank_maps(),
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

    #[test]
    fn image_browser_opens_and_wraps_in_both_directions() {
        let mut browser = AssetBrowser::new(
            blank_images(2),
            vec![vec![BLACK; 104 * 96]],
            vec![blank_wall_set()],
            test_characters(),
            blank_maps(),
        );
        browser.selection = 3;
        browser.key_pressed(&Key::Named(NamedKey::Enter));
        assert!(matches!(browser.page, BrowserPage::Images));

        browser.key_pressed(&Key::Named(NamedKey::ArrowLeft));
        assert_eq!(browser.image, 1);
        browser.key_pressed(&Key::Named(NamedKey::ArrowRight));
        assert_eq!(browser.image, 0);
    }

    #[test]
    fn roster_opens_wraps_and_escape_returns_to_menu() {
        let mut browser = test_browser();
        browser.selection = 4;
        browser.key_pressed(&Key::Named(NamedKey::Enter));
        assert!(matches!(browser.page, BrowserPage::Roster));

        browser.key_pressed(&Key::Named(NamedKey::ArrowLeft));
        assert_eq!(browser.character, 5);
        browser.key_pressed(&Key::Named(NamedKey::ArrowRight));
        assert_eq!(browser.character, 0);
        browser.key_pressed(&Key::Named(NamedKey::Escape));
        assert!(matches!(browser.page, BrowserPage::Menu));
    }

    fn test_browser() -> AssetBrowser {
        AssetBrowser::new(
            blank_images(1),
            vec![vec![BLACK; 104 * 96]],
            vec![blank_wall_set()],
            test_characters(),
            blank_maps(),
        )
    }

    #[test]
    fn supplied_map_file_decodes_losslessly() {
        let data = fs::read("dos/MAZEDATA.DTA").unwrap();
        let maps = decode_maps(&data).unwrap();

        assert_eq!(maps.len(), 55);
        assert_eq!(&maps[0].walls, &data[..256]);
        assert_eq!(&maps[54].properties, &data[54 * 512 + 256..]);
        assert!(decode_maps(&data[..data.len() - 1]).is_err());
    }

    #[test]
    fn map_renderer_puts_game_north_at_the_top_and_draws_each_edge() {
        let mut map = blank_maps().remove(0);
        map.walls[15 * 16] = 0b01_10_11_01;
        let mut frame = vec![BLACK; (WIDTH * HEIGHT) as usize];
        draw_map(&mut frame, &map, 0, 0);

        assert_eq!(frame[1 * WIDTH as usize + 3], WHITE);
        assert_eq!(frame[3 * WIDTH as usize + 6], CYAN);
        assert_eq!(frame[6 * WIDTH as usize + 3], MAGENTA);
        assert_eq!(frame[3 * WIDTH as usize + 1], WHITE);
        assert_eq!(frame[121 * WIDTH as usize + 3], BLACK);
    }

    #[test]
    fn map_browser_opens_and_wraps_like_other_collections() {
        let mut browser = test_browser();
        browser.key_pressed(&Key::Named(NamedKey::Enter));
        assert!(matches!(browser.page, BrowserPage::Maps));

        browser.key_pressed(&Key::Named(NamedKey::ArrowLeft));
        assert_eq!(browser.map, 54);
        browser.key_pressed(&Key::Named(NamedKey::ArrowUp));
        assert_eq!(browser.map, 0);
        browser.key_pressed(&Key::Named(NamedKey::Escape));
        assert!(matches!(browser.page, BrowserPage::Menu));
    }

    fn test_characters() -> Vec<Character> {
        decode_roster(include_bytes!("../dos/ROSTER.DTA"))
            .unwrap()
            .into_iter()
            .filter(|entry| entry.metadata != 0)
            .map(|entry| entry.character)
            .collect()
    }

    fn blank_images(count: usize) -> Vec<Vec<u8>> {
        vec![vec![BLACK; (WIDTH * HEIGHT) as usize]; count]
    }

    fn blank_wall_set() -> WallSet {
        WallSet {
            components: WALL_COMPONENT_DIMENSIONS
                .iter()
                .map(|&(width, height)| vec![BLACK; width * height])
                .collect(),
        }
    }

    fn blank_maps() -> Vec<Map> {
        vec![
            Map {
                walls: [0; 256],
                properties: [0; 256],
            };
            55
        ]
    }
}
