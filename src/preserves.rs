//! A dedicated jar and mechanical drying cycle for a finite plant batch.
use super::*;

pub(super) const CROP_UNITS: u16 = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(super) enum Work {
    InspectJar,
    RepairRack,
    LoadRack,
    TurnRack,
    JarBatch,
    CookBatch,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
enum Batch {
    #[default]
    Absent,
    Rack {
        begun: u64,
        due: u64,
        turned: bool,
    },
    Jar {
        expires: u64,
    },
    Used,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(super) struct State {
    batch: Batch,
    jar_checked: bool,
    jar_loaded: bool,
}

impl State {
    pub(super) fn journey_started(&self) -> bool {
        self.jar_checked || self.batch != Batch::Absent
    }

    pub(super) fn completed(&self, chapter: &str) -> bool {
        match chapter {
            "Q26" => self.batch == Batch::Used,
            "Q27" => self.jar_loaded,
            _ => false,
        }
    }

    pub(super) fn wants_rack(&self) -> bool {
        matches!(self.batch, Batch::Absent | Batch::Rack { .. })
    }

    pub(super) fn rack_reserved(&self) -> bool {
        matches!(self.batch, Batch::Rack { .. })
    }

    pub(super) fn work(&self, now: u64, rack: bool) -> Option<Work> {
        if !self.jar_checked {
            return Some(Work::InspectJar);
        }
        match self.batch {
            Batch::Absent if rack => Some(Work::LoadRack),
            Batch::Rack {
                begun,
                turned: false,
                ..
            } if rack && now >= begun + DAY_MS => Some(Work::TurnRack),
            Batch::Rack {
                due, turned: true, ..
            } if rack && now >= due => Some(Work::JarBatch),
            Batch::Jar { expires } if now < expires => Some(Work::CookBatch),
            _ => None,
        }
    }

    pub(super) fn advance(&mut self, now: u64) -> bool {
        if matches!(self.batch, Batch::Jar { expires } if now >= expires) {
            self.batch = Batch::Absent;
            true
        } else {
            false
        }
    }

    pub(super) fn commit(&mut self, work: Work, now: u64, damp: bool) {
        match work {
            Work::InspectJar => self.jar_checked = true,
            Work::RepairRack => {}
            Work::LoadRack => {
                self.batch = Batch::Rack {
                    begun: now,
                    due: now + if damp { 4 * DAY_MS } else { 3 * DAY_MS },
                    turned: false,
                }
            }
            Work::TurnRack => {
                if let Batch::Rack { begun, due, .. } = self.batch {
                    self.batch = Batch::Rack {
                        begun,
                        due: due + if damp { DAY_MS } else { 0 },
                        turned: true,
                    };
                }
            }
            Work::JarBatch => {
                self.jar_loaded = true;
                self.batch = Batch::Jar {
                    expires: now + 16 * DAY_MS,
                };
            }
            Work::CookBatch => self.batch = Batch::Used,
        }
    }

    pub(super) fn validate(&self, now: u64) -> bool {
        (!self.jar_loaded || self.jar_checked)
            && match self.batch {
                Batch::Absent => true,
                Batch::Rack { begun, due, .. } => {
                    self.jar_checked
                        && begun <= now
                        && due
                            .checked_sub(begun)
                            .is_some_and(|span| (3 * DAY_MS..=5 * DAY_MS).contains(&span))
                }
                Batch::Jar { expires } => {
                    self.jar_loaded && expires <= now.saturating_add(16 * DAY_MS)
                }
                Batch::Used => self.jar_loaded,
            }
    }
}
