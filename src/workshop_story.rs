//! A single traced timber purchase-free construction job and tool upkeep.
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum Work {
    OilRod,
    ChooseTree,
    MillBeam,
    FitBench,
    Replant,
    AcceptOrder,
    CutOrder,
    FinishOrder,
    DeliverOrder,
    UseOrder,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    pub tree: Option<u8>,
    pub timber: bool,
    pub milled: bool,
    pub built: bool,
    pub planted: bool,
    pub rod_oiled: bool,
    pub rod_tested: bool,
    pub rod_rehung: bool,
    pub rod_wear: u8,
    pub order: Option<(u8, u64)>,
    pub order_used: bool,
}

impl State {
    pub(super) fn completed(&self, chapter: &str) -> bool {
        match chapter {
            "Q20" => self.planted,
            "Q24" => self.rod_rehung,
            "Q29" => self.order_used,
            _ => false,
        }
    }

    pub(super) fn priority(&self) -> Option<Task> {
        if self.built && !self.planted {
            return Some(Task::WorkshopStory(Work::Replant));
        }
        if self.milled && !self.built {
            return Some(Task::WorkshopStory(Work::FitBench));
        }
        if self.timber && !self.milled {
            return Some(Task::WorkshopStory(Work::MillBeam));
        }
        None
    }

    pub(super) fn order_work(&self, i: usize, now: u64) -> Option<Work> {
        if self.order_used || now / DAY_MS < 56 {
            return None;
        }
        Some(match self.order {
            None if i == 1 => Work::AcceptOrder,
            Some((0, at)) if i == 1 && now >= at + DAY_MS => Work::CutOrder,
            Some((1, at)) if i == 1 && now >= at + DAY_MS => Work::FinishOrder,
            Some((2, at)) if i == 1 && now >= at + DAY_MS => Work::DeliverOrder,
            Some((3, _)) if i == 0 => Work::UseOrder,
            _ => return None,
        })
    }

    pub(super) fn validate(&self, now: u64) -> bool {
        self.tree
            .is_none_or(|id| usize::from(id) < TREES.len() && !protected_tree(id.into()))
            && (!self.timber || self.tree.is_some())
            && (!self.milled || self.timber)
            && (!self.built || self.milled)
            && (!self.planted || self.built)
            && self.rod_wear <= 3
            && (!self.rod_tested || self.rod_oiled)
            && (!self.rod_rehung || self.rod_tested)
            && self
                .order
                .is_none_or(|(stage, at)| stage <= 3 && (56 * DAY_MS..=now).contains(&at))
            && (!self.order_used || self.order.is_some_and(|(stage, _)| stage == 3))
    }
}
