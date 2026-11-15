//! The most recent soft-ground tracks. They carry no inventory or collision.
use super::{ActorKind, HOUR, MINUTE, YardWeather};
use crate::map_data::TilePos;

const MAX_TRACKS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum Impression {
    Light = 1,
    Deep = 2,
    Lasting = 3,
}

impl Impression {
    fn lifetime(self) -> u64 {
        u64::from(self as u8) * (HOUR / 2)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Footprint {
    pub tile: TilePos,
    pub toward: TilePos,
    pub kind: ActorKind,
    pub snow: bool,
    pub left_foot: bool,
}

#[derive(Clone, Copy, Debug)]
struct Track {
    mark: Footprint,
    laid_at: u64,
    impression: Impression,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Tracks {
    entries: [Option<Track>; MAX_TRACKS],
    next: usize,
    left_foot: [bool; 3],
    trail: [Option<(TilePos, u64, u8, Impression)>; 3],
}

// Transient marks must not participate in gameplay equality after reload.
impl PartialEq for Tracks {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Tracks {
    pub(super) fn add(&mut self, tile: TilePos, toward: TilePos, kind: ActorKind, snow: bool, impression: Impression, now: u64) -> bool {
        let left_foot = &mut self.left_foot[kind as usize];
        *left_foot = !*left_foot;
        self.entries[self.next] = Some(Track {
            mark: Footprint { tile, toward, kind, snow, left_foot: *left_foot },
            laid_at: now,
            impression,
        });
        self.next = (self.next + 1) % MAX_TRACKS;
        let trail = &mut self.trail[kind as usize];
        let steps = trail.filter(|&(prior, at, _, earlier)| {
            prior == tile && earlier == impression && now.saturating_sub(at) <= 4 * MINUTE
        }).map_or(1, |(_, _, steps, _)| (steps + 1).min(4));
        *trail = Some((toward, now, steps, impression));
        steps == 3
    }

    pub(super) fn interrupt(&mut self, kind: ActorKind) {
        self.trail[kind as usize] = None;
    }

    pub(super) fn visible(&self, now: u64, weather: YardWeather) -> impl Iterator<Item = Footprint> + '_ {
        self.entries.iter().flatten().filter_map(move |entry| {
            let not_erased = entry.laid_at / (3 * HOUR) == now / (3 * HOUR)
                || !matches!(weather, YardWeather::Rain | YardWeather::Snow);
            (not_erased && now.saturating_sub(entry.laid_at) < entry.impression.lifetime()).then_some(entry.mark)
        })
    }

    pub(super) fn erase(&mut self) {
        self.entries.fill(None);
        self.trail = [None; 3];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_marks_have_bounded_space_and_disappear_after_rain() {
        let mut tracks = Tracks::default();
        let laid = 3 * HOUR - 1;
        for x in 1..=48 {
            tracks.add(TilePos::new(x, 20), TilePos::new(x + 1, 20), ActorKind::Orange, false, Impression::Light, laid);
        }
        let remaining: Vec<_> = tracks.visible(laid, YardWeather::Clouds).collect();
        assert_eq!(remaining.len(), MAX_TRACKS);
        assert!(remaining.iter().all(|m| m.tile.x > 16));
        assert_eq!(tracks.visible(laid + 1, YardWeather::Rain).count(), 0);
        assert_eq!(tracks.visible(laid + HOUR / 2, YardWeather::Clouds).count(), 0);
        tracks.erase();
        assert_eq!(tracks.visible(laid, YardWeather::Clouds).count(), 0);
    }

    #[test]
    fn a_wet_trail_requires_three_contiguous_recent_steps() {
        let mut tracks = Tracks::default();
        let kind = ActorKind::Orange;
        let mut step = |from: u16, minute: u64| {
            tracks.add(TilePos::new(from, 20), TilePos::new(from + 1, 20), kind, false, Impression::Light, minute * MINUTE)
        };
        assert!(!step(1, 1));
        assert!(!step(2, 2));
        assert!(!step(3, 10));
        assert!(!step(4, 11));
        assert!(step(5, 12));
        assert!(!step(6, 13));
    }

    #[test]
    fn distinct_ground_impressions_survive_thirty_sixty_or_ninety_minutes() {
        let mut tracks = Tracks::default();
        for (i, impression) in [Impression::Light, Impression::Deep, Impression::Lasting].into_iter().enumerate() {
            let x = 2 * i as u16 + 1;
            tracks.add(TilePos::new(x, 20), TilePos::new(x + 1, 20), ActorKind::Orange, false, impression, 2 * HOUR);
        }
        let remaining = |minutes| tracks.visible(2 * HOUR + minutes * MINUTE, YardWeather::Clouds).count();
        assert_eq!([remaining(29), remaining(30), remaining(60), remaining(90)], [3, 2, 1, 0]);
    }
}
