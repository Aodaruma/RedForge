use std::{
    collections::HashSet,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use nucleation::{UniversalSchematic, formats::litematic};

use crate::DATA_VERSION;

pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_VOLUME: i64 = 16_777_216;
const HISTORY_LIMIT: usize = 100;
const AIR: &str = "minecraft:air";

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPos {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CellChange {
    pub pos: BlockPos,
    pub before: String,
    pub after: String,
}

#[derive(Clone, Debug, Default)]
struct Transaction(Vec<CellChange>);

pub struct Document {
    schematic: UniversalSchematic,
    undo: Vec<Transaction>,
    redo: Vec<Transaction>,
    path: Option<PathBuf>,
    pub dirty: bool,
    pub revision: u64,
}

impl Default for Document {
    fn default() -> Self {
        Self::new("Untitled")
    }
}

impl Document {
    pub fn new(name: &str) -> Self {
        let mut schematic = UniversalSchematic::new(name.to_owned());
        schematic.metadata.mc_version = Some(DATA_VERSION);
        schematic.metadata.source_data_version = Some(DATA_VERSION);
        Self {
            schematic,
            undo: Vec::new(),
            redo: Vec::new(),
            path: None,
            dirty: false,
            revision: 0,
        }
    }

    pub fn gate_zero_fixture() -> Result<Self, String> {
        let mut document = Self::new("Gate 0 - Button Dust Piston");
        let mut cells = Vec::new();
        for x in -1..=4 {
            cells.push((BlockPos::new(x, 0, 0), "minecraft:stone"));
        }
        cells.extend([
            (
                BlockPos::new(0, 1, 0),
                "minecraft:stone_button[face=floor,facing=east,powered=false]",
            ),
            (
                BlockPos::new(1, 1, 0),
                "minecraft:redstone_wire[east=side,north=none,power=0,south=none,west=side]",
            ),
            (
                BlockPos::new(2, 1, 0),
                "minecraft:sticky_piston[extended=false,facing=east]",
            ),
        ]);
        document.apply_cells(cells)?;
        document.undo.clear();
        document.dirty = false;
        Ok(document)
    }

    pub fn block(&self, pos: BlockPos) -> String {
        self.schematic
            .get_block(pos.x, pos.y, pos.z)
            .map(ToString::to_string)
            .unwrap_or_else(|| AIR.to_owned())
    }

    pub fn blocks(&self) -> Vec<(BlockPos, String)> {
        self.schematic
            .iter_blocks()
            .filter(|(_, block)| block.name.as_str() != AIR)
            .map(|(pos, block)| (BlockPos::new(pos.x, pos.y, pos.z), block.to_string()))
            .collect()
    }

    pub fn block_states(&self) -> Vec<String> {
        let mut states: Vec<_> = self
            .schematic
            .iter_blocks()
            .map(|(_, state)| state.to_string())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        states.push(AIR.to_owned());
        states.sort();
        states.dedup();
        states
    }

    pub fn apply_cells<I, S>(&mut self, cells: I) -> Result<usize, String>
    where
        I: IntoIterator<Item = (BlockPos, S)>,
        S: AsRef<str>,
    {
        let mut changes = Vec::new();
        for (pos, state) in cells {
            let after = state.as_ref().to_owned();
            let before = self.block(pos);
            if before != after {
                changes.push(CellChange { pos, before, after });
            }
        }
        if changes.is_empty() {
            return Ok(0);
        }
        self.write_changes(&changes, false)?;
        let count = changes.len();
        self.undo.push(Transaction(changes));
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.changed();
        Ok(count)
    }

    pub fn undo(&mut self) -> Result<bool, String> {
        let Some(transaction) = self.undo.pop() else {
            return Ok(false);
        };
        self.write_changes(&transaction.0, true)?;
        self.redo.push(transaction);
        self.changed();
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, String> {
        let Some(transaction) = self.redo.pop() else {
            return Ok(false);
        };
        self.write_changes(&transaction.0, false)?;
        self.undo.push(transaction);
        self.changed();
        Ok(true)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn open(path: &Path) -> Result<Self, String> {
        let metadata = fs::metadata(path)
            .map_err(|error| format!("{} を確認できません: {error}", path.display()))?;
        if metadata.len() > MAX_FILE_BYTES {
            return Err(format!(
                "ファイルが大きすぎます: {} bytes（上限 {} bytes）",
                metadata.len(),
                MAX_FILE_BYTES
            ));
        }
        let bytes =
            fs::read(path).map_err(|error| format!("{} を読めません: {error}", path.display()))?;
        let schematic = litematic::from_litematic(&bytes)
            .map_err(|error| format!("Litematicを解析できません: {error}"))?;
        if schematic.get_region_names().len() != 1 {
            return Err("複数regionのLitematicは現在読み取り専用です".to_owned());
        }
        let (x, y, z) = schematic.get_dimensions();
        let volume = i64::from(x) * i64::from(y) * i64::from(z);
        if volume > MAX_VOLUME {
            return Err(format!(
                "schematic volume {volume} が上限 {MAX_VOLUME} を超えています"
            ));
        }
        Ok(Self {
            schematic,
            undo: Vec::new(),
            redo: Vec::new(),
            path: Some(path.to_owned()),
            dirty: false,
            revision: 1,
        })
    }

    pub fn save(&mut self, path: &Path) -> Result<(), String> {
        self.write_copy(path)?;
        self.path = Some(path.to_owned());
        self.dirty = false;
        Ok(())
    }

    pub fn write_copy(&self, path: &Path) -> Result<(), String> {
        let bytes = litematic::to_litematic(&self.schematic)
            .map_err(|error| format!("Litematicを生成できません: {error}"))?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| format!("保存先を作成できません: {error}"))?;
        let temp = path.with_extension("litematic.tmp");
        let mut file = File::create(&temp)
            .map_err(|error| format!("一時ファイルを作成できません: {error}"))?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("一時ファイルを書き込めません: {error}"))?;
        fs::rename(&temp, path)
            .map_err(|error| format!("保存ファイルを置き換えられません: {error}"))?;
        Ok(())
    }

    pub fn selected_blocks(&self, a: BlockPos, b: BlockPos) -> Vec<(BlockPos, String)> {
        let min = BlockPos::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z));
        let max = BlockPos::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z));
        self.blocks()
            .into_iter()
            .filter(|(pos, _)| {
                (min.x..=max.x).contains(&pos.x)
                    && (min.y..=max.y).contains(&pos.y)
                    && (min.z..=max.z).contains(&pos.z)
            })
            .collect()
    }

    pub(crate) fn schematic(&self) -> &UniversalSchematic {
        &self.schematic
    }

    pub(crate) fn minimum(&self) -> BlockPos {
        let min = self.schematic.get_bounding_box().min;
        BlockPos::new(min.0, min.1, min.2)
    }

    fn write_changes(&mut self, changes: &[CellChange], reverse: bool) -> Result<(), String> {
        for change in changes {
            let state = if reverse {
                &change.before
            } else {
                &change.after
            };
            self.schematic
                .try_set_block_str(change.pos.x, change.pos.y, change.pos.z, state)
                .map_err(|error| format!("BlockState {state} を配置できません: {error}"))?;
        }
        Ok(())
    }

    fn changed(&mut self) {
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
    }
}

pub fn rotate_state_y(state: &str) -> Result<String, String> {
    let mut block = nucleation::BlockState::from_block_string(state)?;
    if let Some(value) = block.get_property("facing").cloned() {
        let rotated = match value.as_str() {
            "north" => "east",
            "east" => "south",
            "south" => "west",
            "west" => "north",
            _ => value.as_str(),
        };
        block.set_property("facing", rotated);
    }
    let cardinal = ["north", "east", "south", "west"];
    let values: Vec<_> = cardinal
        .iter()
        .map(|key| block.get_property(key).cloned())
        .collect();
    for (index, value) in values.into_iter().enumerate() {
        if let Some(value) = value {
            block.set_property(cardinal[(index + 1) % 4], value);
        }
    }
    Ok(block.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_drag_is_one_undo_transaction() {
        let mut document = Document::default();
        let cells = (0..20).map(|x| (BlockPos::new(x, 0, 0), "minecraft:redstone_wire"));
        assert_eq!(document.apply_cells(cells).unwrap(), 20);
        assert!(document.undo().unwrap());
        assert!(document.blocks().is_empty());
        assert!(document.redo().unwrap());
        assert_eq!(document.blocks().len(), 20);
    }

    #[test]
    fn litematic_round_trip_keeps_block_state() {
        let mut document = Document::default();
        document
            .apply_cells([(
                BlockPos::new(3, 2, -4),
                "minecraft:repeater[delay=3,facing=east,locked=false,powered=false]",
            )])
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "redforge-roundtrip-{}-{}.litematic",
            std::process::id(),
            document.revision
        ));
        document.save(&path).unwrap();
        document
            .apply_cells([(BlockPos::new(0, 0, 0), "minecraft:stone")])
            .unwrap();
        document.save(&path).unwrap();
        let loaded = Document::open(&path).unwrap();
        assert_eq!(
            loaded.block(BlockPos::new(3, 2, -4)),
            document.block(BlockPos::new(3, 2, -4))
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rotates_facing_and_wire_sides() {
        assert_eq!(
            rotate_state_y("minecraft:observer[facing=north,powered=false]").unwrap(),
            "minecraft:observer[facing=east,powered=false]"
        );
        let wire = rotate_state_y(
            "minecraft:redstone_wire[east=side,north=up,power=0,south=none,west=side]",
        )
        .unwrap();
        assert!(wire.contains("east=up"));
        assert!(wire.contains("south=side"));
    }
}
