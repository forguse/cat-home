//! Finite seasonal orchard leaves placed on their own roots as mulch.
use super::*;

const CYCLE: u64 = 112;
const FIRST_TREE: usize = 8;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub(super) struct Cover([Option<u64>; 4]);

impl Cover {
    pub(super) fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    pub(super) fn valid(&self, now: u64) -> bool {
        self.0.iter().all(|day| {
            day.is_none_or(|day| day <= now / DAY_MS && (56..84).contains(&(day % CYCLE)))
        })
    }

    pub(super) fn protects_orchard(&self, index: usize, day: u64) -> bool {
        self.0[index].is_some_and(|placed| {
            let next_buds = (placed / CYCLE + 1) * CYCLE + 5;
            day >= placed && day + index as u64 * 3 < next_buds
        })
    }
}

impl LivingWorld {
    fn orchard_mulch_foot(&self, tree: u8) -> Option<TilePos> {
        let index = usize::from(tree).checked_sub(FIRST_TREE)?;
        let state = self.village.as_ref()?;
        let orchard = state.orchard.get(index)?;
        let day = self.now_ms / DAY_MS;
        if self.season() != YardSeason::Autumn
            || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
            || orchard.phase != OrchardPhase::AfterHarvest
            || state.family_routines.orchard_cover.0[index]
                .is_some_and(|applied| applied / CYCLE == day / CYCLE)
            || self
                .actors
                .iter()
                .any(|actor| actor.task == Some(Task::CareOrchard(index as u8)))
        {
            return None;
        }
        let foot = self.site(0, Task::CareOrchard(index as u8));
        (self.valid_world_cell(foot)
            && !self.actors.iter().any(|actor| {
                !actor.inside
                    && (actor.cell == foot || actor.route.get(actor.route_next) == Some(&foot))
            }))
        .then_some(foot.tile())
    }

    pub(crate) fn village_leaf_mulch_site(&self) -> Option<(u8, TilePos)> {
        (8..12).find_map(|tree| self.orchard_mulch_foot(tree).map(|foot| (tree, foot)))
    }

    pub(crate) fn village_leaf_rubbing_foot(&self, tree: u8) -> Option<TilePos> {
        let index = usize::from(tree).checked_sub(FIRST_TREE)?;
        let day = self.now_ms / DAY_MS;
        // Until the local day 77, AfterHarvest can only come from the real
        // last fruit being lifted from this tree. From day 77 through leaf
        // fall it would also be set by the calendar, which proves nothing.
        let phase_day = (day + index as u64 * 3) % CYCLE;
        let orchard = self.village.as_ref()?.orchard.get(index)?;
        if !(49..=76).contains(&phase_day)
            || orchard.fruit != 0
            || orchard.phase != OrchardPhase::AfterHarvest
            || self.season() != YardSeason::Autumn
            || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
        {
            return None;
        }
        // One intact leaf catches on the side of the tree after the picking.
        // The loose ground litter collected from the southern stand is a
        // different source, and is never used for the paper impression.
        let trunk = Cell::at(self.trees[usize::from(tree)].position)?.tile();
        [(1, 0), (-1, 0), (0, -1)]
            .into_iter()
            .filter_map(|(dx, dy)| trunk.offset(dx, dy))
            .find(|&tile| {
                self.valid_world_cell(Cell::from(tile))
                    && !self.actors.iter().any(|actor| {
                        !actor.inside
                            && (actor.cell.tile() == tile
                                || actor.route.get(actor.route_next) == Some(&Cell::from(tile)))
                    })
            })
    }

    pub(crate) fn place_village_leaf_mulch(&mut self, tree: u8, at: TilePos) -> bool {
        if self.orchard_mulch_foot(tree) != Some(at) {
            return false;
        }
        self.village
            .as_mut()
            .unwrap()
            .family_routines
            .orchard_cover
            .0[usize::from(tree) - FIRST_TREE] = Some(self.now_ms / DAY_MS);
        true
    }

    pub fn village_tree_mulch(&self, tree: usize) -> Option<TilePos> {
        let index = tree.checked_sub(FIRST_TREE)?;
        let state = self.village.as_ref()?;
        (index < state.orchard.len()
            && state
                .family_routines
                .orchard_cover
                .protects_orchard(index, self.now_ms / DAY_MS))
        .then(|| Cell::at(self.trees[tree].position).map(Cell::tile))
        .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_local_leaf_layer_protects_the_same_orchard_roots_until_buds() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        world.now_ms = (77..83)
            .map(|day| day * DAY_MS + 10 * HOUR)
            .find(|&at| {
                matches!(
                    LivingWorld::weather_at(at),
                    YardWeather::Clear | YardWeather::Clouds
                )
            })
            .unwrap();
        let day = world.now_ms / DAY_MS;
        world.village.as_mut().unwrap().advance_orchard(day);
        let (tree, foot) = world.village_leaf_mulch_site().unwrap();
        assert!(!world.place_village_leaf_mulch(tree, TilePos::new(1, 1)));
        assert!(world.place_village_leaf_mulch(tree, foot));
        assert!(!world.place_village_leaf_mulch(tree, foot));
        assert!(world.village_tree_mulch(usize::from(tree)).is_some());
        let mut saved: LivingWorld =
            serde_json::from_slice(&serde_json::to_vec(&world).unwrap()).unwrap();
        let index = usize::from(tree) - FIRST_TREE;
        let state = saved.village.as_mut().unwrap();
        let protected = state.orchard[index].health;
        state.advance_orchard(91);
        assert_eq!(state.orchard[index].health, protected);
        state.advance_orchard(126);
        assert_eq!(state.orchard[index].health, protected - 1);
    }
}
