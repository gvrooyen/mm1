use std::fs;

use serde::Serialize;

use crate::character::{Character, decode_roster};

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Facing {
    North,
    East,
    South,
    West,
}

impl Facing {
    fn mask(self) -> u8 {
        match self {
            Self::North => 0xc0,
            Self::East => 0x30,
            Self::South => 0x0c,
            Self::West => 0x03,
        }
    }
    fn left(self) -> Self {
        match self {
            Self::North => Self::West,
            Self::West => Self::South,
            Self::South => Self::East,
            Self::East => Self::North,
        }
    }
    fn right(self) -> Self {
        self.left().left().left()
    }
    fn opposite(self) -> Self {
        self.left().left()
    }
    fn shift(self, x: u8, y: u8) -> Option<(u8, u8)> {
        match self {
            Self::North if y < 15 => Some((x, y + 1)),
            Self::East if x < 15 => Some((x + 1, y)),
            Self::South if y > 0 => Some((x, y - 1)),
            Self::West if x > 0 => Some((x - 1, y)),
            _ => None,
        }
    }
    fn bits(self) -> u8 {
        match self {
            Self::North => 6,
            Self::East => 4,
            Self::South => 2,
            Self::West => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Screen {
    Title,
    Inn,
    Town,
    Food,
    Tavern,
    Temple,
    Training,
    Blacksmith,
    Leprechaun,
    Statue,
    Passage,
    Encounter,
    Combat,
    Treasure,
    Character,
}

#[derive(Serialize)]
pub struct PartyMember<'a> {
    pub slot: usize,
    pub is_current: bool,
    pub name: &'a str,
    pub level: u8,
    pub experience: u32,
    pub hp: u16,
    pub max_hp: u16,
    pub condition: u8,
    pub gold: u32,
    pub gems: u16,
    pub food: u8,
    pub backpack: Vec<InventoryItemView<'a>>,
}

#[derive(Serialize)]
pub struct InventoryItemView<'a> {
    pub slot: usize,
    pub name: &'a str,
    pub charges: u8,
}

#[derive(Serialize)]
pub struct ExitView {
    pub direction: Facing,
    pub wall_type: u8,
    pub passable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PerspectiveCell {
    pub left: u8,
    pub front: u8,
    pub right: u8,
}

#[derive(Serialize)]
pub struct PlayerView<'a> {
    pub schema_version: u32,
    pub kind: Screen,
    pub title: &'a str,
    pub options: Vec<String>,
    pub message: &'a str,
    pub party: Vec<PartyMember<'a>>,
    pub position: Option<(u8, u8)>,
    pub facing: Option<Facing>,
    pub exits: Vec<ExitView>,
    pub combat: Option<CombatView<'a>>,
}

#[derive(Serialize)]
pub struct EnemyView<'a> {
    pub slot: usize,
    pub name: &'a str,
    pub image: u8,
    pub hp: u16,
    pub max_hp: u16,
    pub alive: bool,
}

#[derive(Serialize)]
pub struct CombatView<'a> {
    pub round: u16,
    pub active_party_member: Option<usize>,
    pub enemies: Vec<EnemyView<'a>>,
    pub treasure_gold: u32,
}

#[derive(Clone, Debug)]
struct ItemDef {
    id: u8,
    name: String,
    max_charges: u8,
    cost: u16,
}

#[derive(Clone, Debug)]
struct MonsterDef {
    name: String,
    image: u8,
    hp: u8,
    armor_class: u8,
    max_damage: u8,
    attacks: u8,
    speed: u8,
    experience: u16,
    loot: u8,
}

#[derive(Clone, Debug)]
struct Enemy {
    definition: usize,
    hp: u16,
    max_hp: u16,
}

#[derive(Clone, Debug)]
struct CombatState {
    enemies: Vec<Enemy>,
    round: u16,
    active: usize,
    treasure_gold: u32,
}

pub struct Game {
    pub screen: Screen,
    roster: Vec<Character>,
    selected: Vec<usize>,
    pub party: Vec<Character>,
    pub x: u8,
    pub y: u8,
    pub facing: Facing,
    message: String,
    current: usize,
    maze: [u8; 512],
    overlay_data: Vec<u8>,
    specials: Vec<Special>,
    items: Vec<ItemDef>,
    monsters: Vec<MonsterDef>,
    combat: Option<CombatState>,
    encounter_min_level: u8,
    encounter_max_level: u8,
    shop_stock: Option<[u8; 6]>,
    properties: [u8; 256],
    cleared_specials: [bool; 256],
    drinks: Vec<u8>,
    rumor_heard: bool,
    rng: u32,
}

impl Game {
    pub fn load() -> Result<Self, String> {
        let maze_data = fs::read("dos/MAZEDATA.DTA").map_err(|error| error.to_string())?;
        if maze_data.len() < 512 {
            return Err("MAZEDATA.DTA does not contain Sorpigal's 512-byte map".into());
        }
        let maze: [u8; 512] = maze_data[..512].try_into().unwrap();
        let overlay = fs::read("dos/SORPIGAL.OVR").map_err(|error| error.to_string())?;
        let overlay_data = decode_overlay_data(&overlay)?.to_vec();
        let specials = decode_specials(&overlay_data)?;
        let roster_data = fs::read("dos/ROSTER.DTA").map_err(|error| error.to_string())?;
        let roster = decode_roster(&roster_data)?
            .into_iter()
            .filter(|r| r.metadata != 0)
            .map(|r| r.character)
            .collect();
        let executable = fs::read("dos/MM.EXE").map_err(|error| error.to_string())?;
        let items = decode_items(&executable)?;
        let monsters = decode_monsters(&executable)?;
        let encounter_min_level = overlay_data[47];
        let encounter_max_level = overlay_data[33];
        Ok(Self {
            screen: Screen::Title,
            roster,
            selected: vec![],
            party: vec![],
            x: 8,
            y: 3,
            facing: Facing::North,
            message: String::new(),
            current: 0,
            properties: maze[256..512].try_into().unwrap(),
            maze,
            overlay_data,
            specials,
            items,
            monsters,
            combat: None,
            encounter_min_level,
            encounter_max_level,
            shop_stock: None,
            cleared_specials: [false; 256],
            drinks: vec![],
            rumor_heard: false,
            rng: 0x4d4d_3101,
        })
    }
    pub fn view(&self) -> PlayerView<'_> {
        let title = match self.screen {
            Screen::Title => "Might and Magic",
            Screen::Inn => "Sorpigal Inn",
            Screen::Town => "Sorpigal",
            Screen::Food => "Food Store",
            Screen::Tavern => "Tavern",
            Screen::Temple => "Temple",
            Screen::Training => "Training Grounds",
            Screen::Blacksmith => "Blacksmith",
            Screen::Leprechaun => "Leprechaun",
            Screen::Statue => "Statue",
            Screen::Passage => "Passage",
            Screen::Encounter => "Encounter",
            Screen::Combat => "Combat",
            Screen::Treasure => "Treasure",
            Screen::Character => "Character",
        };
        let options = match self.screen {
            Screen::Title => vec!["start".into()],
            Screen::Inn => self
                .roster
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    format!(
                        "toggle:{} {}{}",
                        i + 1,
                        c.name,
                        if self.selected.contains(&i) {
                            " [IN PARTY]"
                        } else {
                            ""
                        }
                    )
                })
                .chain(["confirm".into()])
                .collect(),
            Screen::Town => vec![
                "forward".into(),
                "back".into(),
                "left".into(),
                "right".into(),
                "unlock".into(),
                "bash".into(),
            ],
            Screen::Food => vec!["food".into(), "escape".into()],
            Screen::Tavern => vec![
                "drink".into(),
                "tip".into(),
                "rumor".into(),
                "gather".into(),
                "choose:N".into(),
                "escape".into(),
            ],
            Screen::Temple => vec![
                "restore".into(),
                "realign".into(),
                "donate".into(),
                "choose:N".into(),
                "escape".into(),
            ],
            Screen::Training => vec!["train".into(), "choose:N".into(), "escape".into()],
            Screen::Blacksmith => {
                let mut options = vec![
                    "weapons".into(),
                    "armor".into(),
                    "misc".into(),
                    "choose:N".into(),
                ];
                if let Some(stock) = self.shop_stock {
                    options.extend(stock.into_iter().enumerate().map(|(index, id)| {
                        let item = self.item(id);
                        format!("buy:{} {} ({} gold)", index + 1, item.name, item.cost)
                    }));
                }
                options.extend(["sell:N".into(), "escape".into()]);
                options
            }
            Screen::Leprechaun => vec![
                "choose:1".into(),
                "choose:2".into(),
                "choose:3".into(),
                "choose:4".into(),
                "choose:5".into(),
                "no".into(),
            ],
            Screen::Passage => vec!["yes".into(), "no".into()],
            Screen::Encounter => vec!["fight".into(), "flee".into()],
            Screen::Combat => vec![
                "attack:N".into(),
                "defend".into(),
                "cast".into(),
                "flee".into(),
            ],
            Screen::Treasure => vec!["open".into(), "leave".into()],
            Screen::Character => vec!["escape".into()],
            _ => vec!["escape".into()],
        };
        PlayerView {
            schema_version: 2,
            kind: self.screen,
            title,
            options,
            message: &self.message,
            party: self
                .party
                .iter()
                .enumerate()
                .map(|(index, c)| PartyMember {
                    slot: index + 1,
                    is_current: index == self.current,
                    name: &c.name,
                    level: c.level.current,
                    experience: c.experience,
                    hp: c.current_hp,
                    max_hp: c.effective_max_hp,
                    condition: c.condition,
                    gold: c.gold,
                    gems: c.gems,
                    food: c.food,
                    backpack: c
                        .backpack_items
                        .iter()
                        .zip(c.backpack_charges)
                        .enumerate()
                        .filter_map(|(slot, (&id, charges))| {
                            (id != 0).then(|| InventoryItemView {
                                slot: slot + 1,
                                name: &self.item(id).name,
                                charges,
                            })
                        })
                        .collect(),
                })
                .collect(),
            position: (self.screen != Screen::Title && self.screen != Screen::Inn)
                .then_some((self.x, self.y)),
            facing: (self.screen != Screen::Title && self.screen != Screen::Inn)
                .then_some(self.facing),
            exits: if self.screen == Screen::Town {
                [Facing::North, Facing::East, Facing::South, Facing::West]
                    .into_iter()
                    .map(|d| {
                        let w = self.wall(d);
                        ExitView {
                            direction: d,
                            wall_type: w,
                            passable: self.can_move(d),
                        }
                    })
                    .collect()
            } else {
                vec![]
            },
            combat: self.combat.as_ref().map(|combat| CombatView {
                round: combat.round,
                active_party_member: self.active_party_member().map(|index| index + 1),
                enemies: combat
                    .enemies
                    .iter()
                    .enumerate()
                    .map(|(index, enemy)| EnemyView {
                        slot: index + 1,
                        name: &self.monsters[enemy.definition].name,
                        image: self.monsters[enemy.definition].image,
                        hp: enemy.hp,
                        max_hp: enemy.max_hp,
                        alive: enemy.hp != 0,
                    })
                    .collect(),
                treasure_gold: combat.treasure_gold,
            }),
        }
    }

    /// Describes the visible Sorpigal corridor without exposing windowing or
    /// graphics-asset types to game state.
    pub fn perspective(&self) -> Vec<PerspectiveCell> {
        let mut cells = Vec::with_capacity(4);
        let (mut x, mut y) = (self.x, self.y);
        for _ in 0..4 {
            let wall = |direction: Facing| {
                (self.maze[y as usize * 16 + x as usize] >> direction.bits()) & 3
            };
            let front = wall(self.facing);
            cells.push(PerspectiveCell {
                left: wall(self.facing.left()),
                front,
                right: wall(self.facing.right()),
            });
            if front != 0 {
                break;
            }
            let Some(next) = self.facing.shift(x, y) else {
                break;
            };
            (x, y) = next;
        }
        cells
    }
    pub fn command(&mut self, raw: &str) {
        let cmd = raw.trim().to_ascii_lowercase();
        self.message.clear();
        if self.screen == Screen::Title {
            if cmd == "start" || cmd == "confirm" {
                self.screen = Screen::Inn;
                self.message = "Select one to six adventurers, then confirm.".into();
            } else {
                self.message = "Use start to enter the Inn of Sorpigal.".into();
            }
            return;
        }
        if self.screen == Screen::Inn {
            if let Some(n) = cmd
                .strip_prefix("toggle:")
                .and_then(|s| s.parse::<usize>().ok())
            {
                if n > 0 && n <= self.roster.len() {
                    let i = n - 1;
                    if let Some(p) = self.selected.iter().position(|x| *x == i) {
                        self.selected.remove(p);
                    } else if self.selected.len() < 6 {
                        self.selected.push(i)
                    }
                }
                return;
            }
            if cmd == "confirm" {
                if self.selected.is_empty() {
                    self.message = "Choose at least one character.".into()
                } else {
                    self.party = self
                        .selected
                        .iter()
                        .map(|i| self.roster[*i].clone())
                        .collect();
                    self.current = 0;
                    self.drinks = vec![0; self.party.len()];
                    self.x = 8;
                    self.y = 3;
                    self.facing = Facing::North;
                    self.screen = Screen::Town;
                    self.message = "You leave the inn and enter Sorpigal.".into()
                }
            } else if cmd == "escape" && self.party.is_empty() {
                self.screen = Screen::Title;
            } else if cmd == "escape" {
                self.x = 8;
                self.y = 3;
                self.facing = Facing::North;
                self.screen = Screen::Town;
                self.message = "You leave the inn without changing the party.".into();
            }
            return;
        }
        if matches!(
            self.screen,
            Screen::Encounter | Screen::Combat | Screen::Treasure
        ) {
            self.combat_command(&cmd);
            return;
        }
        if let Some(n) = cmd
            .strip_prefix("view:")
            .and_then(|s| s.parse::<usize>().ok())
            && self.screen == Screen::Town
            && n > 0
            && n <= self.party.len()
        {
            self.current = n - 1;
            self.screen = Screen::Character;
            return;
        }
        if self.screen == Screen::Character {
            if cmd == "escape" {
                self.screen = Screen::Town;
            }
            return;
        }
        if let Some(n) = cmd
            .strip_prefix("choose:")
            .and_then(|s| s.parse::<usize>().ok())
        {
            if !self.party.is_empty() {
                self.current = (n.saturating_sub(1)).min(self.party.len() - 1)
            };
            if self.screen == Screen::Leprechaun {
                self.leprechaun(n);
            }
            return;
        }
        if cmd == "escape" || cmd == "no" {
            self.leave_service("You decline.");
            return;
        }
        match self.screen {
            Screen::Town => match cmd.as_str() {
                "left" => {
                    self.facing = self.facing.left();
                    self.update_current_cell();
                }
                "right" => {
                    self.facing = self.facing.right();
                    self.update_current_cell();
                }
                "back" => self.walk(self.facing.opposite(), true),
                "forward" => self.walk(self.facing, false),
                "unlock" => self.unlock(),
                "bash" => self.bash(),
                _ => self.message = "Unknown command.".into(),
            },
            Screen::Food if cmd == "food" => {
                let mut bought = false;
                for c in &mut self.party {
                    if c.gold >= 5 {
                        c.gold -= 5;
                        c.food = 40;
                        bought = true;
                    }
                }
                self.message = if bought {
                    "Thank you, come again! Each member who paid 5 gold now has 40 food."
                } else {
                    "No gold, no food!"
                }
                .into()
            }
            Screen::Tavern => self.tavern(&cmd),
            Screen::Temple => self.temple(&cmd),
            Screen::Training if cmd == "train" => self.train(),
            Screen::Blacksmith => self.blacksmith(&cmd),
            Screen::Passage if cmd == "yes" => {
                self.leave_service(
                    "That destination is outside the implemented Sorpigal slice; you remain in town.",
                );
            }
            _ => self.message = "That option is not available here.".into(),
        }
    }
    fn wall(&self, d: Facing) -> u8 {
        (self.maze[self.y as usize * 16 + self.x as usize] >> d.bits()) & 3
    }

    pub fn current_character(&self) -> Option<&Character> {
        self.party.get(self.current)
    }
    fn walk(&mut self, d: Facing, backwards: bool) {
        if !self.can_move(d) || (backwards && self.wall(d) != 0) {
            self.message = if self.wall(d) == 2 {
                "The door is locked from this side."
            } else {
                "A wall blocks the way."
            }
            .into();
            return;
        }
        if let Some((x, y)) = d.shift(self.x, self.y) {
            self.x = x;
            self.y = y;
            let offset = self.y as usize * 16 + self.x as usize;
            let special = self.properties[offset] & 0x80 != 0 && !self.cleared_specials[offset];
            self.update_current_cell();
            if !special
                && self.screen == Screen::Town
                && Self::next_random(&mut self.rng, self.overlay_data[29]) == self.overlay_data[29]
            {
                self.start_encounter();
            }
        } else {
            self.message = "The edge of this game slice blocks the way.".into()
        }
    }
    fn can_move(&self, d: Facing) -> bool {
        let restriction = 1 << d.bits();
        d.shift(self.x, self.y).is_some()
            && self.properties[self.y as usize * 16 + self.x as usize] & restriction == 0
    }
    fn update_current_cell(&mut self) {
        let offset = self.y as usize * 16 + self.x as usize;
        if self.properties[offset] & 0x80 == 0 || self.cleared_specials[offset] {
            return;
        }
        let mut coordinate_is_known = false;
        for (i, s) in self.specials.iter().enumerate() {
            if s.x == self.x && s.y == self.y {
                coordinate_is_known = true;
            }
            if s.x == self.x && s.y == self.y && s.mask & self.facing.mask() != 0 {
                self.screen = match i {
                    0 => Screen::Inn,
                    2 => Screen::Blacksmith,
                    3 => Screen::Food,
                    5 => Screen::Tavern,
                    6 => Screen::Temple,
                    7 => Screen::Training,
                    8 => Screen::Leprechaun,
                    10..=17 => Screen::Statue,
                    4 | 9 | 23 => Screen::Passage,
                    22 => Screen::Town,
                    _ => Screen::Town,
                };
                self.message = match i {
                    0 => "The innkeeper asks, \"Would you like to sign in?\"",
                    1 if self.facing == Facing::East => "Eulard's Fine Foods",
                    1 if self.facing == Facing::West => "B and B Blacksmithing",
                    1 => "The Inn of Sorpigal",
                    2 => "A smith in a leather apron asks if he can help.",
                    3 => "An overweight dwarf offers 40 food for 5 gold per party member.",
                    4 => "A passage leads outside. Take it?",
                    5 => "Step up to the bar.",
                    6 => "Clerics of Temple Moonshadow ask if you seek their help.",
                    7 => "Worg the guildmaster asks if you require training.",
                    8 => "A leprechaun offers passage to towns 1-5 for one gem.",
                    9 => "Stairs go down to the caverns. Take them?",
                    10..=17 => STATUES[i - 10],
                    18 => "Temple Moonshadow",
                    19 => "Jail. Keep Out!",
                    20 => "Ye Olde Hogge Tavern",
                    21 => "Otto's Training",
                    22 => "The character of future encounters changes here.",
                    23 => "Trap door! The caverns below are not implemented.",
                    _ => "",
                }
                .into();
                if i == 0 {
                    self.sync_party_to_roster();
                    self.selected = self
                        .party
                        .iter()
                        .filter_map(|party_member| {
                            self.roster.iter().position(|roster_member| {
                                roster_member.roster_index == party_member.roster_index
                            })
                        })
                        .collect();
                }
                if i == 22 {
                    self.encounter_min_level = 3;
                    self.encounter_max_level = 6;
                    self.cleared_specials[offset] = true;
                }
                return;
            }
        }
        if !coordinate_is_known {
            self.cleared_specials[offset] = true;
            self.start_encounter();
        }
    }

    fn start_encounter(&mut self) {
        let eligible_levels: u16 = self
            .party
            .iter()
            .filter(|member| can_act(member))
            .map(|member| member.level.current as u16)
            .sum();
        let count = (eligible_levels / 2)
            .clamp(1, self.overlay_data[34] as u16)
            .min(10) as usize;
        let mut enemies = Vec::with_capacity(count);
        for _ in 0..count {
            let level_range = self
                .encounter_max_level
                .saturating_sub(self.encounter_min_level)
                .saturating_add(1);
            let level = self.encounter_min_level
                + Self::next_random(&mut self.rng, level_range).saturating_sub(1);
            let definition = (level.saturating_sub(1) as usize * 16
                + (Self::next_random(&mut self.rng, 16) - 1) as usize)
                .min(self.monsters.len() - 1);
            let hp = self.monsters[definition].hp.max(1) as u16;
            enemies.push(Enemy {
                definition,
                hp,
                max_hp: hp,
            });
        }
        self.combat = Some(CombatState {
            enemies,
            round: 1,
            active: 0,
            treasure_gold: 0,
        });
        self.screen = Screen::Encounter;
        self.message = "Monsters approach. Fight or flee?".into();
    }

    fn active_party_member(&self) -> Option<usize> {
        let combat = self.combat.as_ref()?;
        (0..self.party.len())
            .map(|offset| (combat.active + offset) % self.party.len())
            .find(|&index| can_act(&self.party[index]))
    }

    fn combat_command(&mut self, command: &str) {
        if self.screen == Screen::Treasure {
            if command == "open" {
                let gold = self
                    .combat
                    .as_ref()
                    .map_or(0, |combat| combat.treasure_gold);
                if let Some(member) = self
                    .party
                    .iter_mut()
                    .find(|member| can_receive_rewards(member))
                {
                    member.gold = member.gold.saturating_add(gold);
                }
                self.finish_combat(format!("Treasure opened: {gold} gold."));
            } else if command == "leave" || command == "escape" {
                self.finish_combat("You leave the treasure untouched.".into());
            } else {
                self.message = "Open or leave the treasure.".into();
            }
            return;
        }
        if command == "flee" || command == "escape" {
            let success = Self::next_random(&mut self.rng, 100) <= 60;
            if success {
                self.finish_combat("The party escapes.".into());
            } else {
                self.message = "The party fails to escape.".into();
                self.enemy_turn();
            }
            return;
        }
        if self.screen == Screen::Encounter {
            if command == "fight" {
                self.screen = Screen::Combat;
                self.message = "Combat begins.".into();
            } else {
                self.message = "Choose fight or flee.".into();
            }
            return;
        }
        if command == "cast" {
            self.message = "Combat spell casting is not implemented.".into();
            return;
        }
        let Some(actor) = self.active_party_member() else {
            self.finish_combat("The party is defeated.".into());
            return;
        };
        if command == "defend" {
            self.message = format!("{} defends.", self.party[actor].name);
        } else if let Some(target) = command
            .strip_prefix("attack:")
            .and_then(|s| s.parse::<usize>().ok())
        {
            let target = target.wrapping_sub(1);
            let Some(enemy) = self.combat.as_ref().and_then(|c| c.enemies.get(target)) else {
                self.message = "Choose an enemy slot.".into();
                return;
            };
            if enemy.hp == 0 {
                self.message = "That enemy is already defeated.".into();
                return;
            }
            let ac = self.monsters[enemy.definition].armor_class;
            let hit = Self::next_random(&mut self.rng, 20) as u16
                + self.party[actor].accuracy.current as u16 / 3
                + self.party[actor].level.current as u16
                >= 10 + ac as u16;
            if hit {
                let damage = (Self::next_random(&mut self.rng, 6) as u16
                    + self.party[actor].might.current.saturating_sub(10) as u16 / 3)
                    .max(1);
                let enemy = &mut self.combat.as_mut().unwrap().enemies[target];
                enemy.hp = enemy.hp.saturating_sub(damage);
                self.message = format!(
                    "{} hits {} for {damage}.",
                    self.party[actor].name, self.monsters[enemy.definition].name
                );
            } else {
                self.message = format!("{} misses.", self.party[actor].name);
            }
        } else {
            self.message = "Attack an enemy, defend, cast, or flee.".into();
            return;
        }
        if self
            .combat
            .as_ref()
            .unwrap()
            .enemies
            .iter()
            .all(|enemy| enemy.hp == 0)
        {
            self.victory();
            return;
        }
        let next = (1..=self.party.len())
            .map(|offset| (actor + offset) % self.party.len())
            .find(|&index| can_act(&self.party[index]));
        if next.is_none_or(|index| index <= actor) {
            self.enemy_turn();
        } else {
            self.combat.as_mut().unwrap().active = next.unwrap();
        }
    }

    fn enemy_turn(&mut self) {
        let living: Vec<usize> = self
            .party
            .iter()
            .enumerate()
            .filter(|(_, member)| can_be_targeted(member))
            .map(|(i, _)| i)
            .collect();
        if living.is_empty() {
            self.finish_combat("The party is defeated.".into());
            return;
        }
        let enemies = self.combat.as_ref().unwrap().enemies.clone();
        for enemy in enemies.into_iter().filter(|enemy| enemy.hp != 0) {
            let targets: Vec<usize> = self
                .party
                .iter()
                .enumerate()
                .filter(|(_, member)| can_be_targeted(member))
                .map(|(i, _)| i)
                .collect();
            if targets.is_empty() {
                break;
            }
            let target =
                targets[(Self::next_random(&mut self.rng, targets.len() as u8) - 1) as usize];
            let def = &self.monsters[enemy.definition];
            for _ in 0..def.attacks.max(1) {
                if Self::next_random(&mut self.rng, 20) as u16 + def.speed as u16 / 3
                    >= 10 + self.party[target].armor_class.current as u16
                {
                    let damage = Self::next_random(&mut self.rng, def.max_damage.max(1)) as u16;
                    self.party[target].current_hp =
                        self.party[target].current_hp.saturating_sub(damage);
                    if self.party[target].current_hp == 0 {
                        self.party[target].condition |= 0x40;
                        break;
                    }
                }
            }
        }
        if self.active_party_member().is_none() {
            self.finish_combat("The party is defeated.".into());
        } else if let Some(combat) = self.combat.as_mut() {
            combat.round = combat.round.saturating_add(1);
            combat.active = 0;
        }
    }

    fn victory(&mut self) {
        let definitions: Vec<usize> = self
            .combat
            .as_ref()
            .unwrap()
            .enemies
            .iter()
            .map(|enemy| enemy.definition)
            .collect();
        let mut experience = 0u32;
        let mut gold = 0u32;
        for definition in definitions {
            let def = &self.monsters[definition];
            experience += def.experience as u32;
            gold += match def.loot & 6 {
                2 => Self::next_random(&mut self.rng, 10) as u32,
                4 => Self::next_random(&mut self.rng, 100) as u32,
                6 => Self::next_random(&mut self.rng, 4) as u32 * 256,
                _ => 0,
            };
        }
        let active: Vec<usize> = self
            .party
            .iter()
            .enumerate()
            .filter(|(_, member)| can_receive_rewards(member))
            .map(|(i, _)| i)
            .collect();
        let share = experience / active.len().max(1) as u32;
        for index in active {
            self.party[index].experience = self.party[index].experience.saturating_add(share);
        }
        self.combat.as_mut().unwrap().treasure_gold = gold;
        if gold != 0 {
            self.screen = Screen::Treasure;
            self.message =
                format!("Victory! Each active adventurer earns {share} XP. Treasure remains.");
        } else {
            self.finish_combat(format!("Victory! Each active adventurer earns {share} XP."));
        }
    }

    fn finish_combat(&mut self, message: String) {
        self.combat = None;
        self.screen = Screen::Town;
        self.message = message;
    }

    fn leave_service(&mut self, message: &str) {
        if matches!(
            self.screen,
            Screen::Food
                | Screen::Tavern
                | Screen::Temple
                | Screen::Training
                | Screen::Blacksmith
                | Screen::Leprechaun
                | Screen::Passage
        ) {
            self.facing = self.facing.opposite();
        }
        self.screen = Screen::Town;
        self.message = message.into();
    }

    fn unlock(&mut self) {
        let offset = self.y as usize * 16 + self.x as usize;
        let bit = 1 << self.facing.bits();
        if self.wall(self.facing) != 2 || self.properties[offset] & bit == 0 {
            self.message = "There is no locked door ahead.".into();
            return;
        }
        let skill_roll =
            self.overlay_data[49] as u16 * 4 + Self::next_random(&mut self.rng, 100) as u16;
        if skill_roll < self.party[self.current].trap_counter as u16 {
            self.properties[offset] &= !bit;
            self.message = "The lock yields.".into();
        } else if Self::next_random(&mut self.rng, 100) < self.overlay_data[48] {
            self.message = "The lock resists the attempt.".into();
        } else {
            self.properties[offset] &= !bit;
            self.message =
                "The door opens, but its trap is triggered; trap damage is not yet implemented."
                    .into();
        }
    }

    fn bash(&mut self) {
        let offset = self.y as usize * 16 + self.x as usize;
        let bit = 1 << self.facing.bits();
        if self.wall(self.facing) != 2 || self.properties[offset] & bit == 0 {
            self.message = "There is no locked door ahead.".into();
            return;
        }
        let might: u16 = self
            .party
            .iter()
            .map(|member| member.might.current as u16)
            .sum();
        let roll = Self::next_random(&mut self.rng, 100) as u16 + might;
        if self.overlay_data[45] != 0 && roll >= self.overlay_data[45] as u16 {
            self.properties[offset] &= !bit;
            self.message = "The party bashes the door open.".into();
        } else {
            self.message = "The door withstands the blow.".into();
        }
    }

    fn sync_party_to_roster(&mut self) {
        for party_member in &self.party {
            if let Some(roster_member) = self
                .roster
                .iter_mut()
                .find(|member| member.roster_index == party_member.roster_index)
            {
                *roster_member = party_member.clone();
            }
        }
    }

    fn item(&self, id: u8) -> &ItemDef {
        &self.items[id as usize - 1]
    }

    fn blacksmith(&mut self, command: &str) {
        const WEAPONS: [u8; 6] = [2, 3, 5, 61, 62, 86];
        const ARMOR: [u8; 6] = [156, 121, 122, 123, 124, 125];
        const MISC: [u8; 6] = [172, 171, 175, 178, 185, 192];
        match command {
            "weapons" => self.shop_stock = Some(WEAPONS),
            "armor" => self.shop_stock = Some(ARMOR),
            "misc" => self.shop_stock = Some(MISC),
            _ => {
                if let Some(index) = command
                    .strip_prefix("buy:")
                    .and_then(|value| value.parse::<usize>().ok())
                {
                    let Some(id) = self
                        .shop_stock
                        .and_then(|stock| stock.get(index.wrapping_sub(1)).copied())
                    else {
                        self.message =
                            "Choose stock 1 through 6 after selecting a category.".into();
                        return;
                    };
                    let item = self.item(id).clone();
                    let member = &mut self.party[self.current];
                    let Some(slot) = member.backpack_items.iter().position(|item| *item == 0)
                    else {
                        self.message = "The backpack is full.".into();
                        return;
                    };
                    if member.gold < item.cost as u32 {
                        self.message = format!("{} costs {} gold.", item.name, item.cost);
                        return;
                    }
                    member.gold -= item.cost as u32;
                    member.backpack_items[slot] = item.id;
                    member.backpack_charges[slot] = item.max_charges;
                    self.message = format!("{} purchased for {} gold.", item.name, item.cost);
                    return;
                }
                if let Some(index) = command
                    .strip_prefix("sell:")
                    .and_then(|value| value.parse::<usize>().ok())
                {
                    let slot = index.wrapping_sub(1);
                    let Some(&id) = self.party[self.current].backpack_items.get(slot) else {
                        self.message = "Choose backpack slot 1 through 6.".into();
                        return;
                    };
                    if id == 0 {
                        self.message = "That backpack slot is empty.".into();
                        return;
                    }
                    let item = self.item(id).clone();
                    let mut value = item.cost as u32;
                    if item.max_charges != 0 {
                        value /= 2;
                    }
                    value /= 2;
                    let member = &mut self.party[self.current];
                    member.gold = member.gold.saturating_add(value);
                    member.backpack_items[slot] = 0;
                    member.backpack_charges[slot] = 0;
                    self.message = format!("{} sold for {value} gold.", item.name);
                    return;
                }
                self.message = "Select weapons, armor, or misc; then buy:N or sell:N.".into();
                return;
            }
        }
        let stock = self.shop_stock.unwrap();
        self.message = stock
            .into_iter()
            .enumerate()
            .map(|(index, id)| {
                let item = self.item(id);
                format!("{}: {} ({} gold)", index + 1, item.name, item.cost)
            })
            .collect::<Vec<_>>()
            .join("; ");
    }

    fn tavern(&mut self, command: &str) {
        match command {
            "gather" => {
                let gold = self.party.iter().map(|member| member.gold).sum();
                for member in &mut self.party {
                    member.gold = 0;
                }
                self.party[self.current].gold = gold;
                self.message = format!("The party's {gold} gold is gathered.");
            }
            "rumor" if self.rumor_heard => self.message = "No rumors today.".into(),
            "rumor" => {
                self.rumor_heard = true;
                self.message = "Rumor: Sorpigal has 8 statues.".into();
            }
            "drink" | "tip" => {
                let member = &mut self.party[self.current];
                if member.condition != 0 {
                    self.message =
                        "The bartender refuses to serve someone in that condition.".into();
                    return;
                }
                if member.gold == 0 {
                    self.message = "Not enough gold.".into();
                    return;
                }
                member.gold -= 1;
                if command == "tip" {
                    let drinks = self.drinks[self.current];
                    self.message = if drinks == 0 {
                        "Have a drink, then we'll talk."
                    } else if Self::next_random(&mut self.rng, 3) != 3 {
                        "Thanks a lot, have another round!"
                    } else {
                        match drinks {
                            1 => "Tip: See man in cave below (1,2).",
                            2 => "Tip: Check walls near (12,3).",
                            3 => "Tip: Statue at (2,4) is your first job.",
                            _ => {
                                "Tip: Similar pieces of a puzzle may not belong to the same puzzle."
                            }
                        }
                    }
                    .into();
                    return;
                }
                self.drinks[self.current] = self.drinks[self.current].saturating_add(1);
                let drinks = self.drinks[self.current];
                let endurance = member.endurance.current;
                let roll = Self::next_random(&mut self.rng, 10);
                if drinks >= 3 && roll >= endurance {
                    member.condition |= 0x10;
                    self.message = "The drink leaves the adventurer poisoned.".into();
                } else {
                    self.message = "The drink goes down smoothly.".into();
                }
            }
            _ => self.message = "That tavern option is not available.".into(),
        }
    }

    fn temple(&mut self, command: &str) {
        let member = &mut self.party[self.current];
        match command {
            "restore" => {
                let cost = if member.condition == 0xff {
                    2000
                } else if member.condition & 0x80 != 0 {
                    200
                } else {
                    25
                };
                if member.condition == 0 && member.current_hp == member.base_max_hp {
                    self.message = "No restoration is needed.".into();
                } else if member.gold < cost {
                    self.message = format!("Restoration costs {cost} gold.");
                } else {
                    member.gold -= cost;
                    if member.condition == 0xff {
                        member.age = member.age.saturating_add(10);
                        member.endurance.base = member.endurance.base.saturating_sub(1);
                        member.endurance.current = member.endurance.current.saturating_sub(1);
                    }
                    member.condition = 0;
                    member.current_hp = member.base_max_hp;
                    member.effective_max_hp = member.base_max_hp;
                    self.message = format!("The clerics restore the adventurer for {cost} gold.");
                }
            }
            "realign" => {
                if member.current_alignment == member.initial_alignment {
                    self.message = "The adventurer already follows the original alignment.".into();
                } else if member.gold < 250 {
                    self.message = "Restoring alignment costs 250 gold.".into();
                } else {
                    member.gold -= 250;
                    member.current_alignment = member.initial_alignment;
                    member.alignment_counter = match member.current_alignment {
                        1 => 8,
                        2 => 16,
                        _ => 24,
                    };
                    self.message = "The original alignment is restored.".into();
                }
            }
            "donate" => {
                if member.gold < 100 {
                    self.message = "A donation costs 100 gold.".into();
                } else {
                    member.gold -= 100;
                    member.worthiness |= 1;
                    self.message = "The clerics accept the donation.".into();
                }
            }
            _ => self.message = "That temple option is not available.".into(),
        }
    }

    fn train(&mut self) {
        let member = &mut self.party[self.current];
        let easy_class = matches!(member.class, 1 | 4 | 6);
        let level = member.level.base;
        let cost_table = if easy_class {
            [25, 50, 100, 200, 400, 800, 1500]
        } else {
            [40, 75, 150, 300, 600, 1200, 2500]
        };
        let cost = if level >= 8 {
            if easy_class { 3000 } else { 4000 }
        } else {
            cost_table[level.saturating_sub(1) as usize]
        };
        let mut needed = if easy_class { 1500u32 } else { 2000u32 };
        for _ in 0..level.saturating_sub(1).min(7) {
            needed = needed.saturating_mul(16);
        }
        if level > 8 {
            needed = needed
                .saturating_add((level as u32 - 8) * if easy_class { 150_000 } else { 200_000 });
        }
        if level >= 200 {
            self.message = "No further training is possible.".into();
        } else if member.condition != 0 {
            self.message = "Training is impossible in this condition.".into();
        } else if member.experience < needed {
            self.message = format!(
                "{} more experience is required.",
                needed - member.experience
            );
        } else if member.gold < cost {
            self.message = format!("Training costs {cost} gold.");
        } else {
            member.gold -= cost;
            member.level.base = member.level.base.saturating_add(1);
            member.level.current = member.level.base;
            member.age = member.age.saturating_add(1).min(220);
            member.trap_counter = member.trap_counter.saturating_add(2);
            let die = [12, 10, 10, 8, 6, 8]
                .get(member.class.saturating_sub(1) as usize)
                .copied()
                .unwrap_or(8);
            let mut hp = Self::next_random(&mut self.rng, die) as i16;
            hp += endurance_modifier(member.endurance.base);
            let hp = hp.max(1) as u16;
            member.current_hp = member.current_hp.saturating_add(hp);
            member.base_max_hp = member.current_hp;
            member.effective_max_hp = member.current_hp;
            self.message = format!("Training complete: level {}, +{hp} HP.", member.level.base);
        }
    }

    fn next_random(state: &mut u32, maximum: u8) -> u8 {
        *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        ((*state >> 16) % maximum as u32) as u8 + 1
    }

    fn leprechaun(&mut self, n: usize) {
        if !(1..=5).contains(&n) {
            self.message = "Choose a town from 1 through 5.".into();
            return;
        }
        if n != 1 {
            self.message = "That town is outside this implemented slice; no gem was taken.".into();
            return;
        }
        if let Some(payer) = self.party.iter_mut().find(|member| member.gems > 0) {
            payer.gems -= 1;
            self.screen = Screen::Town;
            self.x = 8;
            self.y = 5;
            self.facing = Facing::North;
            self.message = "The leprechaun returns you to Sorpigal.".into()
        } else {
            self.screen = Screen::Town;
            self.x = 8;
            self.y = 5;
            self.message = "No one has a gem; the leprechaun leaves you in Sorpigal.".into()
        }
    }
}

fn can_act(member: &Character) -> bool {
    member.current_hp != 0 && member.condition & 0xe0 == 0
}

fn can_be_targeted(member: &Character) -> bool {
    member.current_hp != 0 && member.condition & 0xe0 == 0
}

fn can_receive_rewards(member: &Character) -> bool {
    member.current_hp != 0 && member.condition & 0xe0 == 0
}

const STATUES: [&str; 8] = [
    "A human knight's plaque says that services rendered make secrets unfold; travel the five towns.",
    "An elven wizard's plaque says to seek Ranalou and the six castles before judgement day.",
    "A gnome robber's plaque says: one by water, one by land, one by air, and one by sand.",
    "A dwarf paladin painted in black and white checks gives a clue about dungeons and Og's idol.",
    "A half-orc archer's plaque honors Corak and the rediscovery of Dusk.",
    "A human cleric's plaque honors Gala and the Volcanic Isles.",
    "A blue dragon recalls the era before the underground towns.",
    "A gray minotaur points toward the Enchanted Forest fortress.",
];

fn endurance_modifier(endurance: u8) -> i16 {
    match endurance {
        40.. => 10,
        35..=39 => 9,
        30..=34 => 8,
        27..=29 => 7,
        24..=26 => 6,
        21..=23 => 5,
        19..=20 => 4,
        17..=18 => 3,
        15..=16 => 2,
        13..=14 => 1,
        9..=12 => 0,
        7..=8 => -1,
        5..=6 => -2,
        _ => -3,
    }
}

fn decode_items(executable: &[u8]) -> Result<Vec<ItemDef>, String> {
    const START: usize = 0x19b2a;
    const RECORD_SIZE: usize = 24;
    let end = START + 255 * RECORD_SIZE;
    let table = executable
        .get(START..end)
        .ok_or_else(|| "MM.EXE is truncated before its item table".to_owned())?;
    table
        .chunks_exact(RECORD_SIZE)
        .enumerate()
        .map(|(index, record)| {
            if !record[..14].is_ascii() {
                return Err(format!("MM.EXE item {} has a non-ASCII name", index + 1));
            }
            Ok(ItemDef {
                id: (index + 1) as u8,
                name: String::from_utf8(record[..14].to_vec())
                    .unwrap()
                    .trim_end()
                    .to_owned(),
                max_charges: record[0x13],
                cost: u16::from_be_bytes([record[0x14], record[0x15]]),
            })
        })
        .collect()
}

fn decode_monsters(executable: &[u8]) -> Result<Vec<MonsterDef>, String> {
    const START: usize = 0x1b3f2;
    const RECORD_SIZE: usize = 32;
    let table = executable
        .get(START..START + 196 * RECORD_SIZE)
        .ok_or_else(|| "MM.EXE is truncated before its monster table".to_owned())?;
    table
        .chunks_exact(RECORD_SIZE)
        .enumerate()
        .map(|(index, record)| {
            if !record[..15].is_ascii() {
                return Err(format!("MM.EXE monster {} has a non-ASCII name", index + 1));
            }
            Ok(MonsterDef {
                name: String::from_utf8(record[..15].to_vec())
                    .unwrap()
                    .trim_end()
                    .to_owned(),
                image: record[30],
                hp: record[17],
                armor_class: record[18],
                max_damage: record[19],
                attacks: record[20],
                speed: record[21],
                experience: u16::from_le_bytes([record[22], record[23]]),
                loot: record[24],
            })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Special {
    pub x: u8,
    pub y: u8,
    pub mask: u8,
}
#[cfg(test)]
fn specials() -> Result<Vec<Special>, String> {
    let overlay = fs::read("dos/SORPIGAL.OVR").map_err(|error| error.to_string())?;
    decode_specials(decode_overlay_data(&overlay)?)
}

fn decode_specials(data: &[u8]) -> Result<Vec<Special>, String> {
    if data.len() < 99 {
        return Err("SORPIGAL.OVR data is too short for its special table".into());
    }
    Ok((0..24)
        .map(|i| Special {
            x: data[51 + i] % 16,
            y: data[51 + i] / 16,
            mask: data[75 + i],
        })
        .collect())
}

fn decode_overlay_data(overlay: &[u8]) -> Result<&[u8], String> {
    if overlay.len() < 14 {
        return Err("SORPIGAL.OVR is shorter than its header".into());
    }
    let code_size = u16::from_le_bytes([overlay[4], overlay[5]]) as usize;
    let data_size = u16::from_le_bytes([overlay[8], overlay[9]]) as usize;
    let start = 14 + code_size;
    let end = start + data_size;
    overlay
        .get(start..end)
        .ok_or_else(|| "SORPIGAL.OVR payload is truncated".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_specials() {
        let s = specials().unwrap();
        assert_eq!(
            s[0],
            Special {
                x: 8,
                y: 3,
                mask: 0x0c
            }
        );
        assert_eq!(
            s[22],
            Special {
                x: 5,
                y: 15,
                mask: 0xff
            }
        );
        assert_eq!(
            s[23],
            Special {
                x: 5,
                y: 8,
                mask: 0x0f
            }
        )
    }
    #[test]
    fn party_and_services() {
        let mut g = Game::load().unwrap();
        g.command("start");
        g.command("toggle:1");
        g.command("confirm");
        assert_eq!((g.x, g.y, g.facing), (8, 3, Facing::North));
        g.screen = Screen::Food;
        let gold = g.party[0].gold;
        g.command("food");
        assert_eq!(g.party[0].gold, gold - 5);
        assert_eq!(g.party[0].food, 40)
    }

    #[test]
    fn movement_uses_directional_property_restrictions() {
        let mut g = Game::load().unwrap();
        g.x = 3;
        g.y = 7;
        assert_eq!(g.wall(Facing::South), 2, "the map draws a door here");
        assert!(!g.can_move(Facing::South), "the property plane locks it");
        g.walk(Facing::South, false);
        assert_eq!((g.x, g.y), (3, 7));
        assert!(g.message.contains("locked"));
    }

    #[test]
    fn turning_on_a_special_cell_dispatches_for_the_new_facing() {
        let mut g = Game::load().unwrap();
        g.command("start");
        g.command("toggle:1");
        g.command("confirm");
        g.command("right");
        assert_eq!(g.screen, Screen::Town);
        g.command("right");
        assert_eq!(g.screen, Screen::Inn);
    }

    #[test]
    fn perspective_follows_the_maze_until_the_first_front_wall() {
        let mut g = Game::load().unwrap();
        g.x = 8;
        g.y = 3;
        g.facing = Facing::North;

        let perspective = g.perspective();

        assert!(!perspective.is_empty());
        assert!(perspective.len() <= 4);
        assert!(perspective.last().is_some_and(|cell| cell.front != 0));
    }

    #[test]
    fn party_members_can_be_opened_from_town_and_closed() {
        let mut g = Game::load().unwrap();
        g.command("start");
        g.command("toggle:1");
        g.command("confirm");

        g.command("view:1");
        assert_eq!(g.screen, Screen::Character);
        assert_eq!(g.current_character().unwrap().name, "CRAG THE HACK");
        g.command("escape");
        assert_eq!(g.screen, Screen::Town);
    }

    #[test]
    fn temple_donation_costs_one_hundred_and_does_not_heal() {
        let mut g = Game::load().unwrap();
        g.command("start");
        g.command("toggle:1");
        g.command("confirm");
        g.screen = Screen::Temple;
        g.party[0].current_hp -= 1;
        let hp = g.party[0].current_hp;
        let gold = g.party[0].gold;
        g.command("donate");
        assert_eq!(g.party[0].gold, gold - 100);
        assert_eq!(g.party[0].current_hp, hp);
        assert_eq!(g.party[0].worthiness & 1, 1);
    }

    #[test]
    fn leprechaun_uses_the_first_party_member_who_has_a_gem() {
        let mut g = Game::load().unwrap();
        g.command("start");
        g.command("toggle:1");
        g.command("toggle:2");
        g.command("confirm");
        g.party[0].gems = 0;
        g.party[1].gems = 2;
        g.screen = Screen::Leprechaun;
        g.command("choose:1");
        assert_eq!(g.party[1].gems, 1);
        assert_eq!((g.x, g.y), (8, 5));
    }

    #[test]
    fn executable_item_table_drives_blacksmith_transactions() {
        let mut g = Game::load().unwrap();
        assert_eq!(g.item(2).name, "DAGGER");
        assert_eq!(g.item(2).cost, 5);
        assert_eq!(g.item(172).name, "TORCH");
        assert_eq!(g.item(172).max_charges, 1);

        g.command("start");
        g.command("toggle:1");
        g.command("confirm");
        g.screen = Screen::Blacksmith;
        let gold = g.party[0].gold;
        g.command("weapons");
        g.command("buy:1");
        assert_eq!(g.party[0].gold, gold - 5);
        assert!(g.party[0].backpack_items.contains(&2));
        let slot = g.party[0]
            .backpack_items
            .iter()
            .position(|id| *id == 2)
            .unwrap();
        g.command(&format!("sell:{}", slot + 1));
        assert_eq!(g.party[0].gold, gold - 3);
        assert_eq!(g.party[0].backpack_items[slot], 0);
    }

    #[test]
    fn unknown_special_cells_surface_an_encounter_once() {
        let mut g = Game::load().unwrap();
        g.x = 5;
        g.y = 3;
        g.update_current_cell();
        assert_eq!(g.screen, Screen::Encounter);
        assert!(g.cleared_specials[3 * 16 + 5]);
        assert!(!g.view().combat.unwrap().enemies.is_empty());
    }

    #[test]
    fn executable_monster_table_decodes_stats_and_rewards() {
        let monsters = decode_monsters(&fs::read("dos/MM.EXE").unwrap()).unwrap();
        assert_eq!(monsters.len(), 196);
        assert_eq!(monsters[0].name, "KOBOLD");
        assert_eq!((monsters[0].hp, monsters[0].experience), (1, 50));
    }

    #[test]
    fn victory_awards_decoded_experience_and_preserves_location() {
        let mut g = Game::load().unwrap();
        g.command("start");
        g.command("toggle:1");
        g.command("confirm");
        let origin = (g.x, g.y, g.facing);
        let before = g.party[0].experience;
        g.combat = Some(CombatState {
            enemies: vec![Enemy {
                definition: 0,
                hp: 0,
                max_hp: 1,
            }],
            round: 1,
            active: 0,
            treasure_gold: 0,
        });
        g.screen = Screen::Combat;
        g.victory();
        assert_eq!(g.party[0].experience, before + 50);
        assert_eq!((g.x, g.y, g.facing), origin);
        assert_eq!(g.screen, Screen::Treasure);
        g.command("open");
        assert_eq!(g.screen, Screen::Town);
    }

    #[test]
    fn flee_and_defeat_are_safe() {
        let mut g = Game::load().unwrap();
        g.command("start");
        g.command("toggle:1");
        g.command("confirm");
        g.start_encounter();
        g.party[0].current_hp = 0;
        g.party[0].condition = 0x40;
        g.screen = Screen::Combat;
        g.command("defend");
        assert_eq!(g.screen, Screen::Town);
        assert!(g.message.contains("defeated"));
    }

    #[test]
    fn poisoned_members_can_act_and_incapacitated_tail_does_not_skip_enemy_turn() {
        let mut g = Game::load().unwrap();
        g.command("start");
        g.command("toggle:1");
        g.command("toggle:2");
        g.command("confirm");
        g.start_encounter();
        g.screen = Screen::Combat;
        g.party[0].current_hp = 100;
        g.party[0].condition = 0x10;
        g.party[1].current_hp = 0;
        g.party[1].condition = 0x40;
        assert_eq!(g.active_party_member(), Some(0));
        g.command("defend");
        assert_eq!(g.combat.as_ref().unwrap().round, 2);
    }

    #[test]
    fn every_screen_serializes() {
        let mut g = Game::load().unwrap();
        for s in [
            Screen::Title,
            Screen::Inn,
            Screen::Town,
            Screen::Food,
            Screen::Tavern,
            Screen::Temple,
            Screen::Training,
            Screen::Blacksmith,
            Screen::Leprechaun,
            Screen::Statue,
            Screen::Passage,
            Screen::Encounter,
            Screen::Combat,
            Screen::Treasure,
            Screen::Character,
        ] {
            g.screen = s;
            serde_json::to_string(&g.view()).unwrap();
        }
    }
}
