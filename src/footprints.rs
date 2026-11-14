//! The most recent soft-ground tracks. They carry no inventory or collision.
use super::{ActorKind, HOUR, YardWeather};
use crate::map_data::TilePos;

const MAX_TRACKS: usize = 32;
const TRACK_LIFE: u64 = HOUR / 2;

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
}

#[derive(Clone, Debug, Default)]
pub(super) struct Tracks {
    entries: [Option<Track>; MAX_TRACKS],
    next: usize,
    left_foot: [bool; 3],
}

// Transient marks must not participate in gameplay equality after reload.
impl PartialEq for Tracks {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Tracks {
    pub(super) fn add(&mut self, tile: TilePos, toward: TilePos, kind: ActorKind, snow: bool, now: u64) {
        let left_foot = &mut self.left_foot[kind as usize];
        *left_foot = !*left_foot;
        self.entries[self.next] = Some(Track {
            mark: Footprint { tile, toward, kind, snow, left_foot: *left_foot },
            laid_at: now,
        });
        self.next = (self.next + 1) % MAX_TRACKS;
    }

    pub(super) fn visible(&self, now: u64, weather: YardWeather) -> impl Iterator<Item = Footprint> + '_ {
        self.entries.iter().flatten().filter_map(move |entry| {
            let not_erased = entry.laid_at / (3 * HOUR) == now / (3 * HOUR)
                || !matches!(weather, YardWeather::Rain | YardWeather::Snow);
            (not_erased && now.saturating_sub(entry.laid_at) < TRACK_LIFE).then_some(entry.mark)
        })
    }

    pub(super) fn erase(&mut self) {
        self.entries.fill(None);
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
            tracks.add(TilePos::new(x, 20), TilePos::new(x + 1, 20), ActorKind::Orange, false, laid);
        }
        let remaining: Vec<_> = tracks.visible(laid, YardWeather::Clouds).collect();
        assert_eq!(remaining.len(), MAX_TRACKS);
        assert!(remaining.iter().all(|m| m.tile.x > 16));
        assert_eq!(tracks.visible(laid + 1, YardWeather::Rain).count(), 0);
        assert_eq!(tracks.visible(laid + TRACK_LIFE, YardWeather::Clouds).count(), 0);
        tracks.erase();
        assert_eq!(tracks.visible(laid, YardWeather::Clouds).count(), 0);
    }
}
