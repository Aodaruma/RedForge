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

pub struct RedstoneSimulation {
    inner: Box<TickSimulation>,
    minimum: BlockPos,
    pub mode: SettleMode,
    pub seed: i64,
}

impl RedstoneSimulation {
    pub fn start(document: &Document, mode: SettleMode, seed: i64) -> Result<Self, String> {
        let bytes = litematic::to_litematic(document.schematic())
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
}
