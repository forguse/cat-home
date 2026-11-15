//! A single reusable rope follows its surveyor around four corners of a genuine house lot.
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Visit { pub order: HousingOrder, pub corner: u8, pub stand: TilePos }

impl Visit {
    pub(super) fn valid(self) -> bool {
        self.order.valid_for(true) && self.order.phase == HousingPhase::Clear
            && self.corner < 4 && self.stand.index().is_some()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Survey { order: HousingOrder, corners: [TilePos; 4], owner: u8, next: u8, expires: u64 }

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State { active: Option<Survey>, outline: Option<(TilePos, [TilePos; 4])> }

impl State {
    pub(super) fn is_empty(&self) -> bool { self == &Self::default() }

    pub(super) fn valid(&self, now: u64) -> bool {
        self.outline.is_none_or(|(lot, corners)| lot.index().is_some() && corners.iter().all(|tile| tile.index().is_some()))
            && self.active.is_none_or(|survey| {
            survey.owner < 2 && survey.expires <= now.saturating_add(6 * HOUR)
                && survey.corners.iter().enumerate().all(|(i, col)| col.index().is_some() && !survey.corners[..i].contains(col))
                && Visit { order: survey.order, corner: survey.next, stand: survey.corners[0] }.valid()
        })
    }
}

impl LivingWorld {
    pub fn yard_survey_line(&self) -> Option<([TilePos; 4], u8)> {
        let state = self.village.as_ref()?.family_routines.survey;
        if let Some(survey) = state.active.filter(|survey| survey.expires >= self.now_ms) {
            return Some((survey.corners, survey.next));
        }
        let (site, corners) = state.outline?;
        (!self.story.housing.orders().any(|order| {
            let [x, y] = order.lot_origin();
            site == TilePos::new(u16::from(x), u16::from(y))
                && !matches!(order.phase, HousingPhase::Clear | HousingPhase::Foundation)
        })).then_some((corners, 4))
    }

    fn survey_corners(&self, i: usize, order: HousingOrder) -> Option<[TilePos; 4]> {
        let [x, y] = order.lot_origin();
        let [w, h] = order.plan.footprint();
        let west = x.checked_sub(1)?;
        let north = y.checked_sub(1)?;
        let east = x.checked_add(w)?;
        let south = y.checked_add(h)?;
        let points = [(west, north, -1, -1), (east, north, 1, -1), (east, south, 1, 1), (west, south, -1, 1)];
        let mut result = [TilePos::new(0, 0); 4];
        for (index, (x, y, dx, dy)) in points.into_iter().enumerate() {
            let corner = TilePos::new(u16::from(x), u16::from(y));
            result[index] = [(0, 0), (dx, 0), (0, dy), (dx, dy)].into_iter()
                .filter_map(|(ox, oy)| corner.offset(ox, oy))
                .find(|&tile| self.quiet_visit_site_available(i, tile) && land_reuse::work_foot_unreserved(tile))?;
        }
        Some(result)
    }

    pub(super) fn maybe_survey_home(&mut self, i: usize, task: Task) -> Task {
        let Task::BuildHome(order) = task else { return task; };
        if !self.is_village() || order.phase != HousingPhase::Clear || self.stock.count(Item::Rope) == 0
            || self.actors[i].carried.is_some()
            || self.actors[i].cell.distance(Cell::from(TilePos::new(u16::from(order.door()[0]), u16::from(order.door()[1])))) > 26
            || self.minute_of_day() >= 16 * 60
        { return task; }
        let [x, y] = order.lot_origin();
        let site = TilePos::new(u16::from(x), u16::from(y));
        let old_survey = self.village.as_ref().unwrap().family_routines.survey;
        if old_survey.outline.is_some_and(|(marked, _)| marked == site) || old_survey.active.is_some() { return task; }
        let Some(corners) = self.survey_corners(i, order) else { return task; };
        if self.actors[i].tool.is_some() {
            return Task::ReturnTool;
        }
        self.village.as_mut().unwrap().family_routines.survey.active = Some(Survey {
            order, corners, owner: i as u8, next: 0, expires: self.now_ms + 6 * HOUR,
        });
        Task::SurveyHome(Visit { order, corner: 0, stand: corners[0] })
    }

    pub(super) fn continue_house_survey(&mut self, i: usize) -> Option<Task> {
        let survey = self.village.as_ref()?.family_routines.survey.active?;
        if usize::from(survey.owner) != i { return None; }
        if survey.expires < self.now_ms || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow)
            || self.actors[i].hunger >= 70 || self.actors[i].thirst >= 70
            || survey.next > 0 && !self.has_carry(i, Item::Rope, 1)
            || survey.next == 0 && !self.has_carry(i, Item::Rope, 1) && self.stock.count(Item::Rope) < 1
        {
            self.village.as_mut().unwrap().family_routines.survey.active = None;
            return None;
        }
        Some(Task::SurveyHome(Visit { order: survey.order, corner: survey.next, stand: survey.corners[usize::from(survey.next)] }))
    }

    pub(super) fn commit_house_survey(&mut self, i: usize, visit: Visit) {
        if !visit.valid() || self.actors[i].cell.tile() != visit.stand
            || !self.has_carry(i, Item::Rope, 1) || matches!(self.weather(), YardWeather::Rain | YardWeather::Snow) {
            return;
        }
        let state = &mut self.village.as_mut().unwrap().family_routines.survey;
        let Some(active) = &mut state.active else { return; };
        if active.expires < self.now_ms || usize::from(active.owner) != i || active.order != visit.order
            || active.next != visit.corner || active.corners[usize::from(visit.corner)] != visit.stand
        { return; }
        if visit.corner < 3 {
            active.next += 1;
        } else {
            let [x, y] = active.order.lot_origin();
            let site = TilePos::new(u16::from(x), u16::from(y));
            state.outline = Some((site, active.corners));
            state.active = None;
            // The same line is still in hand and returns through ordinary stock custody.
            self.record_household_completion("H01-19", i, site, LegacyPlace::Expanded);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reusable_line_marks_four_real_corners_and_keeps_rope_custody() {
        let mut world = LivingWorld::new_village_wall_ms(0);
        let worker = ActorKind::Ayu.index();
        let order: HousingOrder = serde_json::from_value(serde_json::json!({
            "plan":"single", "lot":0, "phase":"clear", "origin": [25,34]
        })).unwrap();
        let area = TilePos::new(27, 37);
        world.actors[worker].cell = Cell::from(area);
        world.actors[worker].position = Cell::from(area).point();
        world.stock.add(Item::Rope, 3);
        world.now_ms = (5..45).map(|d| d * DAY_MS + 10 * HOUR).find(|&now| {
            !matches!(LivingWorld::weather_at(now), YardWeather::Rain | YardWeather::Snow)
        }).unwrap();
        let Task::SurveyHome(mut visit) = world.maybe_survey_home(worker, Task::BuildHome(order)) else {
            panic!("reclaimed house has no legal survey corner arrangement");
        };
        let stock = world.stock.count(Item::Rope);
        assert!(world.take_stock(Item::Rope, 1));
        world.carry(worker, Item::Rope, 1);
        world.commit(worker, Task::SurveyHome(visit));
        assert_eq!(world.yard_survey_line().unwrap().1, 0);
        for corner in 0..4 {
            assert_eq!(visit.corner, corner);
            let cell = Cell::from(visit.stand);
            world.actors[worker].cell = cell;
            world.actors[worker].position = cell.point();
            world.commit(worker, Task::SurveyHome(visit));
            world.commit(worker, Task::SurveyHome(visit));
            assert_eq!(world.yard_survey_line().unwrap().1, corner + 1);
            let bytes = serde_json::to_vec(&world).unwrap();
            world = serde_json::from_slice(&bytes).unwrap();
            if let Some(Task::SurveyHome(next)) = world.continue_house_survey(worker) { visit = next; }
        }
        assert_eq!(world.yard_survey_line().unwrap().1, 4);
        assert!(world.continue_house_survey(worker).is_none());
        assert!(world.has_carry(worker, Item::Rope, 1));
        assert_eq!(world.stock.count(Item::Rope), stock - 1);
        let pantry = world.site(worker, Task::Store);
        world.actors[worker].cell = pantry;
        world.actors[worker].position = pantry.point();
        world.commit(worker, Task::Store);
        assert_eq!(world.stock.count(Item::Rope), stock);
    }
}
