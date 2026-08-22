use crate::document::InventoryItem;

pub const BREW_TICKS: u16 = 400;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Face {
    Up,
    Down,
    Side,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrewEvent {
    FuelLoaded,
    Started,
    Cancelled,
    Completed,
}

#[derive(Clone, Debug)]
pub struct BrewingStand {
    items: Vec<InventoryItem>,
    pub brew_time: u16,
    pub fuel: u8,
    active_ingredient: Option<String>,
    pub observer_pulse: bool,
}

impl BrewingStand {
    pub fn new(items: Vec<InventoryItem>, brew_time: u16, fuel: u8) -> Self {
        let mut stand = Self {
            items: Vec::new(),
            brew_time: brew_time.min(BREW_TICKS),
            fuel: fuel.min(20),
            active_ingredient: None,
            observer_pulse: false,
        };
        for item in items {
            if item.slot < 5 && item.count > 0 {
                stand.items.retain(|existing| existing.slot != item.slot);
                stand.items.push(item);
            }
        }
        stand.items.sort_by_key(|item| item.slot);
        if stand.brew_time > 0 {
            stand.active_ingredient = stand.item(3).map(|item| item.id.clone());
        }
        stand
    }

    pub fn items(&self) -> &[InventoryItem] {
        &self.items
    }

    pub fn progress(&self) -> f32 {
        if self.brew_time == 0 {
            0.0
        } else {
            1.0 - f32::from(self.brew_time) / f32::from(BREW_TICKS)
        }
    }

    pub fn bottle_flags(&self) -> [bool; 3] {
        [
            self.item(0).is_some(),
            self.item(1).is_some(),
            self.item(2).is_some(),
        ]
    }

    pub fn comparator_output(&self) -> u8 {
        if self.items.is_empty() {
            return 0;
        }
        let fullness: f32 = self
            .items
            .iter()
            .map(|item| f32::from(item.count) / f32::from(max_stack(item)))
            .sum::<f32>()
            / 5.0;
        (1.0 + fullness * 14.0).floor().clamp(1.0, 15.0) as u8
    }

    pub fn slots_for(face: Face) -> &'static [u8] {
        match face {
            Face::Up => &[3],
            Face::Down => &[0, 1, 2, 3],
            Face::Side => &[0, 1, 2, 4],
        }
    }

    pub fn can_extract(&self, face: Face, slot: u8) -> bool {
        Self::slots_for(face).contains(&slot)
            && (slot != 3
                || self
                    .item(slot)
                    .is_some_and(|item| item.id == "minecraft:glass_bottle"))
    }

    pub fn set_slot(&mut self, slot: u8, item: Option<InventoryItem>) {
        if slot >= 5 {
            return;
        }
        let before = self.item(slot).is_some();
        self.items.retain(|existing| existing.slot != slot);
        if let Some(mut item) = item.filter(|item| item.count > 0) {
            item.slot = slot;
            self.items.push(item);
            self.items.sort_by_key(|item| item.slot);
        }
        if slot < 3 && before != self.item(slot).is_some() {
            self.observer_pulse = true;
        }
    }

    pub fn tick(&mut self) -> Option<BrewEvent> {
        self.observer_pulse = false;

        if self.brew_time > 0 {
            let ingredient = self.item(3).map(|item| item.id.as_str());
            let valid = ingredient == self.active_ingredient.as_deref()
                && ingredient.is_some_and(|ingredient| self.can_brew(ingredient));
            if !valid {
                self.brew_time = 0;
                self.active_ingredient = None;
                return Some(BrewEvent::Cancelled);
            }
            self.brew_time -= 1;
            if self.brew_time == 0 {
                let ingredient = self
                    .active_ingredient
                    .take()
                    .expect("active brew ingredient");
                self.finish_brew(&ingredient);
                return Some(BrewEvent::Completed);
            }
            return None;
        }

        let mut event = None;
        if self.fuel == 0 && self.item(4).is_some_and(is_blaze_powder) {
            self.decrement(4);
            self.fuel = 20;
            event = Some(BrewEvent::FuelLoaded);
        }
        let ingredient = self.item(3).map(|item| item.id.clone());
        if self.fuel > 0 && ingredient.as_deref().is_some_and(|id| self.can_brew(id)) {
            self.fuel -= 1;
            self.brew_time = BREW_TICKS;
            self.active_ingredient = ingredient;
            return Some(BrewEvent::Started);
        }
        event
    }

    fn item(&self, slot: u8) -> Option<&InventoryItem> {
        self.items.iter().find(|item| item.slot == slot)
    }

    fn can_brew(&self, ingredient: &str) -> bool {
        (0..3).any(|slot| {
            self.item(slot)
                .and_then(|item| recipe(item, ingredient))
                .is_some()
        })
    }

    fn finish_brew(&mut self, ingredient: &str) {
        for slot in 0..3 {
            if let Some(index) = self.items.iter().position(|item| item.slot == slot)
                && let Some(output) = recipe(&self.items[index], ingredient)
            {
                let item = self.items[index].clone().with_potion(output);
                self.items[index] = item;
            }
        }
        self.decrement(3);
    }

    fn decrement(&mut self, slot: u8) {
        let Some(index) = self.items.iter().position(|item| item.slot == slot) else {
            return;
        };
        self.items[index].count -= 1;
        if self.items[index].count == 0 {
            self.items.remove(index);
        }
    }
}

fn is_blaze_powder(item: &InventoryItem) -> bool {
    item.id == "minecraft:blaze_powder"
}

fn recipe(item: &InventoryItem, ingredient: &str) -> Option<&'static str> {
    if item.id != "minecraft:potion" {
        return None;
    }
    match (item.potion()?, ingredient) {
        ("minecraft:water", "minecraft:nether_wart") => Some("minecraft:awkward"),
        ("minecraft:awkward", "minecraft:sugar") => Some("minecraft:swiftness"),
        ("minecraft:swiftness", "minecraft:redstone") => Some("minecraft:long_swiftness"),
        _ => None,
    }
}

fn max_stack(item: &InventoryItem) -> u8 {
    if item.id.ends_with("potion") { 1 } else { 64 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn potion(slot: u8, kind: &str) -> InventoryItem {
        InventoryItem::new(slot, "minecraft:potion", 1).with_potion(kind)
    }

    fn ready_stand() -> BrewingStand {
        BrewingStand::new(
            vec![
                potion(0, "minecraft:water"),
                potion(1, "minecraft:water"),
                potion(2, "minecraft:water"),
                InventoryItem::new(3, "minecraft:nether_wart", 1),
                InventoryItem::new(4, "minecraft:blaze_powder", 1),
            ],
            0,
            0,
        )
    }

    #[test]
    fn water_brews_into_awkward_after_400_ticks() {
        let mut stand = ready_stand();
        assert_eq!(stand.tick(), Some(BrewEvent::Started));
        assert_eq!((stand.fuel, stand.brew_time), (19, 400));
        for _ in 0..399 {
            assert_eq!(stand.tick(), None);
        }
        assert_eq!(stand.tick(), Some(BrewEvent::Completed));
        assert_eq!(stand.brew_time, 0);
        assert!(
            stand
                .items()
                .iter()
                .all(|item| item.slot >= 3 || item.potion() == Some("minecraft:awkward"))
        );
        assert!(stand.items().iter().all(|item| item.slot != 3));
    }

    #[test]
    fn changing_ingredient_cancels_active_brew() {
        let mut stand = ready_stand();
        stand.tick();
        stand.set_slot(3, Some(InventoryItem::new(3, "minecraft:sugar", 1)));
        assert_eq!(stand.tick(), Some(BrewEvent::Cancelled));
        assert_eq!(stand.brew_time, 0);
    }

    #[test]
    fn comparator_matches_five_slot_container_formula() {
        let mut stand = BrewingStand::new(vec![potion(0, "minecraft:water")], 0, 0);
        assert_eq!(stand.comparator_output(), 3);
        stand.set_slot(1, Some(potion(1, "minecraft:water")));
        stand.set_slot(2, Some(potion(2, "minecraft:water")));
        assert_eq!(stand.comparator_output(), 9);
        stand.set_slot(3, Some(InventoryItem::new(3, "minecraft:nether_wart", 64)));
        stand.set_slot(4, Some(InventoryItem::new(4, "minecraft:blaze_powder", 64)));
        assert_eq!(stand.comparator_output(), 15);
    }

    #[test]
    fn observer_only_pulses_when_bottle_occupancy_changes() {
        let mut stand = BrewingStand::new(Vec::new(), 0, 0);
        stand.set_slot(0, Some(potion(0, "minecraft:water")));
        assert!(stand.observer_pulse);
        stand.observer_pulse = false;
        stand.set_slot(0, Some(potion(0, "minecraft:awkward")));
        assert!(!stand.observer_pulse);
        stand.set_slot(0, None);
        assert!(stand.observer_pulse);
    }

    #[test]
    fn sided_slots_match_brewing_stand_rules() {
        assert_eq!(BrewingStand::slots_for(Face::Up), &[3]);
        assert_eq!(BrewingStand::slots_for(Face::Down), &[0, 1, 2, 3]);
        assert_eq!(BrewingStand::slots_for(Face::Side), &[0, 1, 2, 4]);
        let stand = BrewingStand::new(
            vec![InventoryItem::new(3, "minecraft:nether_wart", 1)],
            0,
            0,
        );
        assert!(!stand.can_extract(Face::Down, 3));
    }
}
