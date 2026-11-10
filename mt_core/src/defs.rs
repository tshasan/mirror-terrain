pub const W: usize = 9;
pub const H: usize = 16;
pub const N: usize = W * H;
/// Tiles per half. Side A's half is tiles `0..HALF` (rows 0-7); side B's is the rest.
pub const HALF: usize = N / 2;

/// Each core sits in the bottom corner of its own half, one tile in from the board edge so its 3x3 ring stays on the
/// board: side A at column 1, row 1 (bottom-left on screen, where mt_game draws side A), side B its point reflection
/// (column 7, row 14, top-right).
pub const CORE: [usize; 2] = [W + 1, N - 1 - (W + 1)];

pub fn xy(i: usize) -> (i32, i32) {
    ((i % W) as i32, (i / W) as i32)
}

pub fn idx(x: i32, y: i32) -> Option<usize> {
    if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
        None
    } else {
        Some(y as usize * W + x as usize)
    }
}

pub fn mirror(i: usize) -> usize {
    N - 1 - i
}

pub fn rel(i: usize, side: usize) -> usize {
    if side == 0 { i } else { N - 1 - i }
}

pub fn cheb(a: usize, b: usize) -> i32 {
    let (ax, ay) = xy(a);
    let (bx, by) = xy(b);
    (ax - bx).abs().max((ay - by).abs())
}

/// Whether `i` is on `side`'s half of the board (rows 0-7 for A, 8-15 for B).
pub fn own_half(i: usize, side: usize) -> bool {
    rel(i, side) < HALF
}

/// Within one tile of either core. Nothing is shaped, scarred or generated here.
pub fn near_core(i: usize) -> bool {
    cheb(i, CORE[0]) <= 1 || cheb(i, CORE[1]) <= 1
}

pub fn buildable_row(i: usize, side: usize) -> bool {
    let (_, y) = xy(i);
    if side == 0 { y <= 6 } else { y >= 9 }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Terrain {
    Plain = 0,
    High = 1,
    Trench = 2,
    Wall = 3,
    Water = 4,
    Forest = 5,
    Ore = 6,
}

impl Terrain {
    pub fn buildable(self) -> bool {
        matches!(self, Terrain::Plain | Terrain::High)
    }
    pub fn ground_blocked(self) -> bool {
        matches!(self, Terrain::Wall | Terrain::Water)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Op {
    Raise = 0,
    Lower = 1,
    Wall = 2,
    Flood = 3,
    Grow = 4,
    Clear = 5,
    Expose = 6,
}

pub const ALL_OPS: [Op; 7] = [Op::Raise, Op::Lower, Op::Wall, Op::Flood, Op::Grow, Op::Clear, Op::Expose];

impl Op {
    pub fn cost(self) -> i32 {
        match self {
            Op::Raise | Op::Lower | Op::Grow | Op::Clear => 1,
            Op::Wall | Op::Flood => 2,
            Op::Expose => 3,
        }
    }
    pub fn apply(self, from: Terrain) -> Option<Terrain> {
        use Terrain::*;
        match (self, from) {
            (Op::Raise, Plain) => Some(High),
            (Op::Lower, Plain | High) => Some(Trench),
            (Op::Wall, Plain) => Some(Wall),
            (Op::Flood, Plain | Trench) => Some(Water),
            (Op::Grow, Plain) => Some(Forest),
            (Op::Clear, Forest | Wall | Trench) => Some(Plain),
            (Op::Expose, Plain) => Some(Ore),
            _ => None,
        }
    }
    pub fn blocks_ground(self) -> bool {
        matches!(self, Op::Wall | Op::Flood)
    }
    pub fn name(self) -> &'static str {
        ["raise", "lower", "wall", "flood", "grow", "clear", "expose"][self as usize]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum UnitKind {
    Grunt = 0,
    Rhino = 1,
    Archer = 2,
    Ranger = 3,
    Drone = 4,
    Gunship = 5,
}

pub const ALL_UNITS: [UnitKind; 6] = [
    UnitKind::Grunt,
    UnitKind::Rhino,
    UnitKind::Archer,
    UnitKind::Ranger,
    UnitKind::Drone,
    UnitKind::Gunship,
];

#[derive(Clone, Copy, Debug)]
pub struct UnitStats {
    pub hp: i32,
    pub dmg: i32,
    pub range: i32,
    pub cooldown: u32,
    pub speed: u32,
    pub air: bool,
    pub hits_air: bool,
    pub tier: i32,
    pub cost: i32,
}

impl UnitKind {
    pub fn stats(self) -> UnitStats {
        use UnitKind::*;
        let s = |hp, dmg, range, cooldown, speed, air, hits_air, tier, cost| UnitStats {
            hp,
            dmg,
            range,
            cooldown,
            speed,
            air,
            hits_air,
            tier,
            cost,
        };
        match self {
            Grunt => s(12, 3, 1, 8, 12, false, false, 1, 3),
            Rhino => s(40, 4, 1, 10, 9, false, false, 2, 8),
            Archer => s(8, 3, 3, 10, 10, false, true, 1, 4),
            Ranger => s(10, 3, 2, 9, 12, false, true, 1, 4),
            Drone => s(7, 2, 1, 6, 14, true, true, 1, 6),
            Gunship => s(22, 4, 2, 10, 9, true, true, 2, 8),
        }
    }
    pub fn name(self) -> &'static str {
        ["grunt", "rhino", "archer", "ranger", "drone", "gunship"][self as usize]
    }
    pub fn is_air(self) -> bool {
        self.stats().air
    }
}

pub fn move_cost(kind: UnitKind, t: Terrain, hover: bool) -> Option<u32> {
    if kind.is_air() {
        return Some(100);
    }
    use Terrain::*;
    if kind == UnitKind::Rhino && t == Trench && !hover {
        return None;
    }
    let base = match t {
        Plain | Ore => 100,
        High => 200,
        Trench => {
            if hover {
                100
            } else {
                250
            }
        }
        Forest => 150,
        Wall | Water => return None,
    };
    Some(match (kind, t) {
        (UnitKind::Rhino, Forest) => 250,
        (UnitKind::Ranger, Forest) => 100,
        _ => base,
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum BuildingKind {
    Turret = 0,
    Flak = 1,
    Extractor = 2,
}

#[derive(Clone, Copy, Debug)]
pub struct BuildingStats {
    pub hp: i32,
    pub dmg: i32,
    pub range: i32,
    pub cooldown: u32,
    pub hits_ground: bool,
    pub hits_air: bool,
    pub cost: i32,
}

impl BuildingKind {
    pub fn stats(self) -> BuildingStats {
        match self {
            BuildingKind::Turret => BuildingStats {
                hp: 40,
                dmg: 3,
                range: 3,
                cooldown: 8,
                hits_ground: true,
                hits_air: false,
                cost: 5,
            },
            BuildingKind::Flak => BuildingStats {
                hp: 30,
                dmg: 5,
                range: 3,
                cooldown: 7,
                hits_ground: false,
                hits_air: true,
                cost: 4,
            },
            BuildingKind::Extractor => BuildingStats {
                hp: 20,
                dmg: 0,
                range: 0,
                cooldown: 0,
                hits_ground: false,
                hits_air: false,
                cost: 4,
            },
        }
    }
    pub fn name(self) -> &'static str {
        ["turret", "flak", "extractor"][self as usize]
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tag {
    Ground,
    Air,
    Defense,
    Terrain,
    Economy,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Card {
    Unit(UnitKind),
    Building(BuildingKind),
    Earthshaper,
    Grovekeeper,
    Sapper,
    Prospector,
    HoverTreads,
    Camo,
}

impl Card {
    pub fn tag(self) -> Tag {
        match self {
            Card::Unit(u) if u.is_air() => Tag::Air,
            Card::Unit(_) | Card::HoverTreads => Tag::Ground,
            Card::Building(BuildingKind::Extractor) | Card::Prospector => Tag::Economy,
            Card::Building(_) => Tag::Defense,
            Card::Earthshaper | Card::Grovekeeper | Card::Sapper | Card::Camo => Tag::Terrain,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Card::Unit(u) => u.name(),
            Card::Building(b) => b.name(),
            Card::Earthshaper => "earthshaper",
            Card::Grovekeeper => "grovekeeper",
            Card::Sapper => "sapper",
            Card::Prospector => "prospector",
            Card::HoverTreads => "hover_treads",
            Card::Camo => "camo",
        }
    }
}

pub const CARD_POOL: [Card; 15] = [
    Card::Unit(UnitKind::Rhino),
    Card::Unit(UnitKind::Archer),
    Card::Unit(UnitKind::Ranger),
    Card::Unit(UnitKind::Drone),
    Card::Unit(UnitKind::Gunship),
    Card::Building(BuildingKind::Turret),
    Card::Building(BuildingKind::Flak),
    Card::Building(BuildingKind::Extractor),
    Card::Earthshaper,
    Card::Grovekeeper,
    Card::Sapper,
    Card::Prospector,
    Card::HoverTreads,
    Card::Camo,
    Card::Unit(UnitKind::Drone),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Commander {
    Skywing = 0,
    Ironclad = 1,
    Warden = 2,
    Geomancer = 3,
}

pub const ALL_COMMANDERS: [Commander; 4] = [Commander::Skywing, Commander::Ironclad, Commander::Warden, Commander::Geomancer];

impl Commander {
    pub fn start_card(self) -> Card {
        match self {
            Commander::Skywing => Card::Unit(UnitKind::Drone),
            Commander::Ironclad => Card::Unit(UnitKind::Rhino),
            Commander::Warden => Card::Building(BuildingKind::Turret),
            Commander::Geomancer => Card::Earthshaper,
        }
    }
    pub fn name(self) -> &'static str {
        ["skywing", "ironclad", "warden", "geomancer"][self as usize]
    }
}
