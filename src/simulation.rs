use diplomat_runtime::rust_interop::RustWriteVec;
use nucleation::{
    bridge::{
        mc_tick::ffi::{TickSettleMode, TickSimulation},
        schematic::ffi::Schematic,
    },
    formats::litematic,
};
use serde::Deserialize;

use crate::{
    DATA_VERSION, MINECRAFT_VERSION, NUCLEATION_REVISION,
    document::{BlockPos, Document},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SettleMode {
    #[default]
    InWorld,
    Placement,
    Quiet,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct SimChange {
    pub tick: u32,
    pub pos: [i32; 3],
    pub from: String,
    pub to: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticLevel {
    Limited,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub level: DiagnosticLevel,
    pub pos: BlockPos,
    pub message: String,
}

pub struct RedstoneSimulation {
    inner: Box<TickSimulation>,
    minimum: BlockPos,
    pub mode: SettleMode,
    pub seed: i64,
}

impl RedstoneSimulation {
    pub fn start(document: &Document, mode: SettleMode, seed: i64) -> Result<Self, String> {
        let simulation_schematic = document.simulation_schematic();
        let bytes = litematic::to_litematic(&simulation_schematic)
            .map_err(|error| format!("simulation snapshotを作れません: {error}"))?;
        let schematic = Schematic::from_litematic(&bytes)
            .map_err(|error| format!("simulation snapshotを読めません: {error:?}"))?;
        let extra_states = document.block_states().join(";");
        let minimum = document.minimum();
        let settle = match mode {
            SettleMode::InWorld => TickSettleMode::InWorld,
            SettleMode::Placement => TickSettleMode::Placement,
            SettleMode::Quiet => TickSettleMode::Quiet,
        };
        let mut inner = TickSimulation::from_schematic(
            &schematic,
            settle,
            minimum.x,
            minimum.y,
            minimum.z,
            extra_states.as_bytes(),
        )
        .map_err(|error| {
            format!(
                "tick engineを開始できません: {error:?}: {}",
                last_error_detail()
            )
        })?;
        inner.set_rng_seed(seed);
        Ok(Self {
            inner,
            minimum,
            mode,
            seed,
        })
    }

    pub fn use_block(&mut self, pos: BlockPos) {
        let pos = self.local(pos);
        self.inner.use_block(pos.x, pos.y, pos.z);
    }

    pub fn place_block(&mut self, pos: BlockPos, state: &str) -> Result<(), String> {
        let local = self.local(pos);
        self.inner
            .place_block(local.x, local.y, local.z, state.as_bytes())
            .map_err(|error| format!("{pos:?} に {state} を配置できません: {error:?}"))?;
        let actual = self.block(pos);
        let expected_name = state.split('[').next().unwrap_or(state);
        if !actual.starts_with(expected_name) {
            return Err(format!(
                "配置後のstateが一致しません: requested={state}, actual={actual}"
            ));
        }
        Ok(())
    }

    pub fn step(&mut self) {
        self.inner.step();
    }

    pub fn run(&mut self, ticks: u32) {
        self.inner.run(ticks);
    }

    pub fn tick(&self) -> u32 {
        self.inner.tick_count()
    }

    pub fn is_quiescent(&self) -> bool {
        self.inner.is_quiescent()
    }

    pub fn block(&self, pos: BlockPos) -> String {
        let pos = self.local(pos);
        capture(96, |out| self.inner.get_block(pos.x, pos.y, pos.z, out))
    }

    pub fn changes(&self) -> Result<Vec<SimChange>, String> {
        let json = capture(1024, |out| self.inner.changes_json(out));
        let mut changes: Vec<SimChange> = serde_json::from_str(&json)
            .map_err(|error| format!("block changeを読めません: {error}"))?;
        for change in &mut changes {
            change.pos[0] += self.minimum.x;
            change.pos[1] += self.minimum.y;
            change.pos[2] += self.minimum.z;
        }
        Ok(changes)
    }

    pub fn change_count(&self) -> u32 {
        self.inner.changes_count()
    }

    pub fn summary(&self) -> String {
        format!(
            "Minecraft {MINECRAFT_VERSION} / DataVersion {DATA_VERSION} / Nucleation {} / {:?} / seed {} / origin {},{},{}",
            &NUCLEATION_REVISION[..12],
            self.mode,
            self.seed,
            self.minimum.x,
            self.minimum.y,
            self.minimum.z,
        )
    }

    fn local(&self, pos: BlockPos) -> BlockPos {
        BlockPos::new(
            pos.x - self.minimum.x,
            pos.y - self.minimum.y,
            pos.z - self.minimum.z,
        )
    }
}

pub fn diagnostics(document: &Document) -> Vec<Diagnostic> {
    document
        .blocks()
        .into_iter()
        .filter_map(|(pos, state)| {
            let name = state.split('[').next().unwrap_or(&state);
            let (level, message) = if name == "minecraft:brewing_stand" {
                (
                    DiagnosticLevel::Limited,
                    "Nucleationでは不活性。RedForgeの限定醸造simulationを使用します",
                )
            } else if name == "minecraft:water" || state.contains("waterlogged=true") {
                (
                    DiagnosticLevel::Limited,
                    "water flowとwaterlogged形状は既知の近似があります",
                )
            } else if is_unsupported_mechanic(name) {
                (
                    DiagnosticLevel::Unsupported,
                    "このmechanicは表示・保存のみで、tick simulation対象外です",
                )
            } else {
                return None;
            };
            Some(Diagnostic {
                level,
                pos,
                message: format!("{name}: {message}"),
            })
        })
        .collect()
}

fn is_unsupported_mechanic(name: &str) -> bool {
    name.contains("wheat")
        || name.contains("crop")
        || name.contains("sapling")
        || name.contains("farmland")
        || name.contains("spawner")
}

fn last_error_detail() -> String {
    capture(256, TickSimulation::last_error_detail)
}

fn capture(
    initial_capacity: usize,
    write: impl FnOnce(&mut diplomat_runtime::DiplomatWrite),
) -> String {
    let mut buffer = RustWriteVec::with_capacity(initial_capacity);
    // SAFETY: `buffer` is the only owner and the borrowed writer is not swapped.
    write(unsafe { buffer.borrow_mut() });
    String::from_utf8_lossy(buffer.borrow().as_bytes()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::InventoryItem;

    #[test]
    fn gate_zero_fixture_reaches_tick_engine() {
        let document = Document::gate_zero_fixture().unwrap();
        let mut simulation = RedstoneSimulation::start(&document, SettleMode::InWorld, 0).unwrap();
        simulation.use_block(BlockPos::new(0, 1, 0));
        simulation.step();
        assert_eq!(simulation.tick(), 1);
        assert!(
            simulation
                .block(BlockPos::new(0, 1, 0))
                .contains("powered=true")
        );
        assert!(!simulation.changes().unwrap().is_empty());
    }

    #[test]
    fn lever_repeater_lights_lamp() {
        let mut document = Document::new("lever-lamp");
        let mut cells = Vec::new();
        for x in 0..=4 {
            cells.push((BlockPos::new(x, 0, 0), "minecraft:stone"));
        }
        cells.extend([
            (
                BlockPos::new(0, 1, 0),
                "minecraft:lever[face=floor,facing=east,powered=false]",
            ),
            (
                BlockPos::new(1, 1, 0),
                "minecraft:redstone_wire[east=side,north=none,power=0,south=none,west=side]",
            ),
            (
                BlockPos::new(2, 1, 0),
                "minecraft:repeater[delay=1,facing=west,locked=false,powered=false]",
            ),
            (BlockPos::new(3, 1, 0), "minecraft:redstone_lamp[lit=false]"),
        ]);
        document.apply_cells(cells).unwrap();
        let mut simulation = RedstoneSimulation::start(&document, SettleMode::InWorld, 0).unwrap();
        simulation.use_block(BlockPos::new(0, 1, 0));
        simulation.run(4);
        let lamp = simulation.block(BlockPos::new(3, 1, 0));
        assert!(
            lamp.contains("lit=true"),
            "lamp={lamp}; changes={:?}",
            simulation.changes().unwrap()
        );
    }

    #[test]
    fn placement_is_verified_after_write() {
        let mut document = Document::new("placement");
        document
            .apply_cells([(BlockPos::new(5, 2, -1), "minecraft:stone")])
            .unwrap();
        let mut simulation = RedstoneSimulation::start(&document, SettleMode::InWorld, 0).unwrap();
        simulation
            .place_block(BlockPos::new(5, 2, -1), "minecraft:air")
            .unwrap();
        assert_eq!(simulation.block(BlockPos::new(5, 2, -1)), "minecraft:air");
    }

    #[test]
    fn observer_change_reaches_sticky_piston() {
        let mut document = Document::new("observer-piston");
        document
            .apply_cells([
                (BlockPos::new(0, 0, 0), "minecraft:stone"),
                (BlockPos::new(1, 0, 0), "minecraft:stone"),
                (BlockPos::new(2, 0, 0), "minecraft:stone"),
                (BlockPos::new(0, 1, 0), "minecraft:stone"),
                (
                    BlockPos::new(1, 1, 0),
                    "minecraft:observer[facing=west,powered=false]",
                ),
                (
                    BlockPos::new(2, 1, 0),
                    "minecraft:sticky_piston[extended=false,facing=east]",
                ),
            ])
            .unwrap();
        let mut simulation = RedstoneSimulation::start(&document, SettleMode::InWorld, 0).unwrap();
        simulation
            .place_block(BlockPos::new(0, 1, 0), "minecraft:air")
            .unwrap();
        simulation.run(4);
        let changes = simulation.changes().unwrap();
        assert!(
            changes.iter().any(|change| {
                change.pos == [2, 1, 0]
                    && change.to.starts_with("minecraft:sticky_piston")
                    && change.to.contains("extended=true")
            }),
            "changes={changes:?}"
        );
    }

    #[test]
    fn container_inventory_drives_comparator() {
        let mut document = Document::new("container-comparator");
        document
            .apply_cells([
                (BlockPos::new(0, 0, 0), "minecraft:stone"),
                (BlockPos::new(1, 0, 0), "minecraft:stone"),
                (BlockPos::new(2, 0, 0), "minecraft:stone"),
                (
                    BlockPos::new(0, 1, 0),
                    "minecraft:hopper[enabled=true,facing=down]",
                ),
                (
                    BlockPos::new(1, 1, 0),
                    "minecraft:comparator[facing=west,mode=compare,powered=false]",
                ),
                (
                    BlockPos::new(2, 1, 0),
                    "minecraft:redstone_wire[east=none,north=none,power=0,south=none,west=side]",
                ),
            ])
            .unwrap();
        document
            .set_inventory(
                BlockPos::new(0, 1, 0),
                vec![InventoryItem::new(0, "minecraft:redstone", 64)],
            )
            .unwrap();
        let mut simulation =
            RedstoneSimulation::start(&document, SettleMode::Placement, 0).unwrap();
        simulation.run(2);
        let dust = simulation.block(BlockPos::new(2, 1, 0));
        assert!(!dust.contains("power=0"), "dust={dust}");
    }

    #[test]
    fn dispenser_can_place_water_from_inventory() {
        let mut document = Document::new("dispenser-water");
        document
            .apply_cells([
                (BlockPos::new(0, 0, 0), "minecraft:stone"),
                (BlockPos::new(1, 0, 0), "minecraft:stone"),
                (
                    BlockPos::new(0, 1, 0),
                    "minecraft:lever[face=floor,facing=east,powered=false]",
                ),
                (
                    BlockPos::new(1, 1, 0),
                    "minecraft:dispenser[facing=east,triggered=false]",
                ),
            ])
            .unwrap();
        document
            .set_inventory(
                BlockPos::new(1, 1, 0),
                vec![InventoryItem::new(0, "minecraft:water_bucket", 1)],
            )
            .unwrap();
        let mut simulation = RedstoneSimulation::start(&document, SettleMode::InWorld, 0).unwrap();
        simulation.use_block(BlockPos::new(0, 1, 0));
        simulation.run(4);
        assert!(
            simulation
                .block(BlockPos::new(2, 1, 0))
                .starts_with("minecraft:water"),
            "changes={:?}",
            simulation.changes().unwrap()
        );
    }

    #[test]
    fn brewing_inventory_is_safely_excluded_from_tick_engine() {
        let mut document = Document::new("brewing-snapshot");
        let pos = BlockPos::new(0, 1, 0);
        document
            .apply_cells([(
                pos,
                "minecraft:brewing_stand[has_bottle_0=false,has_bottle_1=false,has_bottle_2=false]",
            )])
            .unwrap();
        document
            .set_inventory(
                pos,
                vec![
                    InventoryItem::new(0, "minecraft:potion", 1).with_potion("minecraft:water"),
                    InventoryItem::new(3, "minecraft:nether_wart", 1),
                ],
            )
            .unwrap();
        let mut simulation = RedstoneSimulation::start(&document, SettleMode::InWorld, 0).unwrap();
        simulation.step();
        assert!(simulation.block(pos).starts_with("minecraft:brewing_stand"));
    }
}
