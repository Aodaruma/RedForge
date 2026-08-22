use std::{
    collections::HashSet,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use nucleation::{
    UniversalSchematic,
    block_entity::BlockEntity,
    block_position::BlockPosition,
    formats::litematic,
    utils::{NbtMap, NbtValue},
};

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

#[derive(Clone, Debug, PartialEq)]
pub struct CellChange {
    pub pos: BlockPos,
    pub before: String,
    pub after: String,
    before_entity: Option<BlockEntity>,
    after_entity: Option<BlockEntity>,
}

#[derive(Clone, Debug)]
enum Transaction {
    Cells(Vec<CellChange>),
    Inventory {
        pos: BlockPos,
        before: Vec<InventoryItem>,
        after: Vec<InventoryItem>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InventoryItem {
    pub slot: u8,
    pub id: String,
    pub count: u8,
}

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
                let before_entity = self.schematic.get_block_entity_owned(block_position(pos));
                let after_entity = (block_name(&before) == block_name(&after))
                    .then(|| before_entity.clone())
                    .flatten();
                changes.push(CellChange {
                    pos,
                    before,
                    after,
                    before_entity,
                    after_entity,
                });
            }
        }
        if changes.is_empty() {
            return Ok(0);
        }
        self.write_changes(&changes, false)?;
        let count = changes.len();
        self.undo.push(Transaction::Cells(changes));
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
        self.write_transaction(&transaction, true)?;
        self.redo.push(transaction);
        self.changed();
        Ok(true)
    }

    pub fn redo(&mut self) -> Result<bool, String> {
        let Some(transaction) = self.redo.pop() else {
            return Ok(false);
        };
        self.write_transaction(&transaction, false)?;
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

    pub fn inventory_slot_count(&self, pos: BlockPos) -> Option<usize> {
        match block_name(&self.block(pos)) {
            "minecraft:hopper" | "minecraft:brewing_stand" => Some(5),
            "minecraft:dispenser" | "minecraft:dropper" => Some(9),
            "minecraft:barrel" | "minecraft:chest" => Some(27),
            _ => None,
        }
    }

    pub fn inventory(&self, pos: BlockPos) -> Vec<InventoryItem> {
        let Some(entity) = self.schematic.get_block_entity_owned(block_position(pos)) else {
            return Vec::new();
        };
        let Some(NbtValue::List(items)) = entity.nbt.get("Items") else {
            return Vec::new();
        };
        let mut result: Vec<_> = items
            .iter()
            .filter_map(|item| {
                let NbtValue::Compound(item) = item else {
                    return None;
                };
                let id = item
                    .get("id")
                    .or_else(|| item.get("Id"))?
                    .as_string()?
                    .clone();
                let slot = nbt_integer(item.get("Slot").or_else(|| item.get("slot"))?)? as u8;
                let count = nbt_integer(item.get("Count").or_else(|| item.get("count"))?)? as u8;
                Some(InventoryItem { slot, id, count })
            })
            .collect();
        result.sort_by_key(|item| item.slot);
        result
    }

    pub fn set_inventory(
        &mut self,
        pos: BlockPos,
        items: Vec<InventoryItem>,
    ) -> Result<bool, String> {
        let slots = self
            .inventory_slot_count(pos)
            .ok_or_else(|| format!("{} は編集可能なcontainerではありません", self.block(pos)))?;
        let mut seen = HashSet::new();
        for item in &items {
            if usize::from(item.slot) >= slots {
                return Err(format!(
                    "slot {} は範囲外です（0..{}）",
                    item.slot,
                    slots - 1
                ));
            }
            if item.id.trim().is_empty() || item.count == 0 || item.count > 64 {
                return Err(format!("slot {} のitem ID/countが不正です", item.slot));
            }
            if !seen.insert(item.slot) {
                return Err(format!("slot {} が重複しています", item.slot));
            }
        }
        let before = self.inventory(pos);
        let mut after = items;
        for item in &mut after {
            if !item.id.contains(':') {
                item.id = format!("minecraft:{}", item.id);
            }
        }
        after.sort_by_key(|item| item.slot);
        if before == after {
            return Ok(false);
        }
        self.write_inventory(pos, &after)?;
        self.undo
            .push(Transaction::Inventory { pos, before, after });
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.changed();
        Ok(true)
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
            let (state, entity) = if reverse {
                (&change.before, &change.before_entity)
            } else {
                (&change.after, &change.after_entity)
            };
            self.schematic
                .try_set_block_str(change.pos.x, change.pos.y, change.pos.z, state)
                .map_err(|error| format!("BlockState {state} を配置できません: {error}"))?;
            if let Some(entity) = entity {
                self.schematic
                    .set_block_entity(block_position(change.pos), entity.clone());
            } else {
                self.schematic
                    .remove_block_entity((change.pos.x, change.pos.y, change.pos.z));
            }
        }
        Ok(())
    }

    fn write_transaction(
        &mut self,
        transaction: &Transaction,
        reverse: bool,
    ) -> Result<(), String> {
        match transaction {
            Transaction::Cells(changes) => self.write_changes(changes, reverse),
            Transaction::Inventory { pos, before, after } => {
                self.write_inventory(*pos, if reverse { before } else { after })
            }
        }
    }

    fn write_inventory(&mut self, pos: BlockPos, items: &[InventoryItem]) -> Result<(), String> {
        let block = self.block(pos);
        self.inventory_slot_count(pos)
            .ok_or_else(|| format!("{block} はcontainerではありません"))?;
        let mut entity = self
            .schematic
            .get_block_entity_owned(block_position(pos))
            .unwrap_or_else(|| BlockEntity::new(block_name(&block).to_owned(), pos_tuple(pos)));
        let items = items
            .iter()
            .map(|item| {
                let mut nbt = NbtMap::new();
                nbt.insert("id".to_owned(), NbtValue::String(item.id.clone()));
                nbt.insert("Count".to_owned(), NbtValue::Byte(item.count as i8));
                nbt.insert("Slot".to_owned(), NbtValue::Byte(item.slot as i8));
                NbtValue::Compound(nbt)
            })
            .collect();
        entity
            .nbt_mut()
            .insert("Items".to_owned(), NbtValue::List(items));
        self.schematic.set_block_entity(block_position(pos), entity);
        Ok(())
    }

    fn changed(&mut self) {
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
    }
}

fn block_position(pos: BlockPos) -> BlockPosition {
    BlockPosition::new(pos.x, pos.y, pos.z)
}

fn pos_tuple(pos: BlockPos) -> (i32, i32, i32) {
    (pos.x, pos.y, pos.z)
}

fn block_name(state: &str) -> &str {
    state.split('[').next().unwrap_or(state)
}

fn nbt_integer(value: &NbtValue) -> Option<i32> {
    match value {
        NbtValue::Byte(value) => Some(i32::from(*value)),
        NbtValue::Short(value) => Some(i32::from(*value)),
        NbtValue::Int(value) => Some(*value),
        _ => None,
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

    #[test]
    fn container_inventory_undo_and_round_trip() {
        let mut document = Document::new("inventory");
        let pos = BlockPos::new(2, 3, 4);
        document
            .apply_cells([(pos, "minecraft:hopper[enabled=true,facing=down]")])
            .unwrap();
        let items = vec![InventoryItem {
            slot: 0,
            id: "minecraft:redstone".to_owned(),
            count: 3,
        }];
        assert!(document.set_inventory(pos, items.clone()).unwrap());
        assert_eq!(document.inventory(pos), items);
        assert!(document.undo().unwrap());
        assert!(document.inventory(pos).is_empty());
        assert!(document.redo().unwrap());
        assert_eq!(document.inventory(pos), items);

        let path = std::env::temp_dir().join(format!(
            "redforge-inventory-{}-{}.litematic",
            std::process::id(),
            document.revision
        ));
        document.save(&path).unwrap();
        let loaded = Document::open(&path).unwrap();
        assert_eq!(loaded.inventory(pos), items);
        fs::remove_file(path).unwrap();
    }
}
