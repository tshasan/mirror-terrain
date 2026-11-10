use mt_core::*;

const BUILDINGS: [BuildingKind; 3] = [BuildingKind::Turret, BuildingKind::Flak, BuildingKind::Extractor];

/// Fuzzer bytes read as game inputs. Running out of bytes yields zeros, so every input is a whole game.
pub struct Bytes<'a>(&'a [u8]);

impl<'a> Bytes<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Bytes(data)
    }
    pub fn u8(&mut self) -> u8 {
        match self.0.split_first() {
            Some((&b, rest)) => {
                self.0 = rest;
                b
            }
            None => 0,
        }
    }
    pub fn u64(&mut self) -> u64 {
        (0..8).fold(0, |acc, i| acc | (self.u8() as u64) << (8 * i))
    }
    /// A tile or card index; 255 stands for an absurd one, so out-of-range inputs are covered.
    pub fn index(&mut self) -> usize {
        match self.u8() {
            255 => usize::MAX,
            b => b as usize,
        }
    }
    pub fn order(&mut self) -> [usize; 3] {
        [self.index(), self.index(), self.index()]
    }
    pub fn plan(&mut self) -> Vec<(usize, Op)> {
        let len = self.u8() % 8;
        (0..len).map(|_| (self.index(), ALL_OPS[self.u8() as usize % ALL_OPS.len()])).collect()
    }
    pub fn build(&mut self) -> BuildCmd {
        let nb = self.u8() % 5;
        let buildings = (0..nb).map(|_| (BUILDINGS[self.u8() as usize % 3], self.index())).collect();
        let nu = self.u8() % 12;
        let units = (0..nu).map(|_| ALL_UNITS[self.u8() as usize % ALL_UNITS.len()]).collect();
        BuildCmd { buildings, units }
    }
}
