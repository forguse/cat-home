//! One bounded, resumable breadth-first search workspace for map navigation.
//!
//! A caller follows the returned segment before requesting another segment
//! from its end. Each segment preserves the original destination and the
//! revision under which its edges were checked.

use crate::{
    map_data::TilePos,
    map_view::{MAX_MAP_HEIGHT, MAX_MAP_WIDTH, MapView},
};

pub const MAX_SEGMENT_EDGES: usize = 128;
const MAX_CELLS: usize = MAX_MAP_WIDTH as usize * MAX_MAP_HEIGHT as usize;
const _: () = assert!(MAX_CELLS <= u16::MAX as usize + 1);
const NEIGHBORS: [(i32, i32); 4] = [(1, 0), (0, 1), (-1, 0), (0, -1)];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathSegment {
    pub start: TilePos,
    pub target: TilePos,
    pub end: TilePos,
    pub view_id: u64,
    pub revision: u64,
    total_edges: usize,
    len: usize,
    steps: [TilePos; MAX_SEGMENT_EDGES],
}

impl PathSegment {
    /// The arrival tile of each edge, in walking order.
    pub fn steps(&self) -> &[TilePos] {
        &self.steps[..self.len]
    }

    /// True when walking this entire segment reaches the requested target.
    pub fn is_complete(&self) -> bool {
        self.end == self.target
    }

    /// Number of edges to the original target, starting at this segment's start.
    pub fn total_edges(&self) -> usize {
        self.total_edges
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchResult {
    Pending,
    Found(PathSegment),
    Unreachable,
    Invalid,
    Stale,
}

#[derive(Clone, Copy)]
struct Request {
    start: TilePos,
    target: TilePos,
    width: u16,
    height: u16,
    view_id: u64,
    revision: u64,
}

impl Request {
    fn contains(self, tile: TilePos) -> bool {
        tile.x < self.width && tile.y < self.height
    }

    fn index(self, tile: TilePos) -> u16 {
        // Callers have checked both coordinates and the supported extent.
        tile.y * self.width + tile.x
    }

    fn tile(self, index: u16) -> TilePos {
        TilePos::new(index % self.width, index / self.width)
    }
}

enum Phase {
    Explore { head: usize, tail: usize },
    Trace { node: u16, len: usize },
    Ready { len: usize },
}

struct ActiveSearch {
    request: Request,
    phase: Phase,
}

pub struct MapNav {
    parents: Box<[u16]>,
    visited: Box<[u64]>,
    // The frontier is reused for the reversed route once the goal is reached.
    queue: Box<[u16]>,
    active: Option<ActiveSearch>,
    last_work: usize,
}

impl Default for MapNav {
    fn default() -> Self {
        Self::new()
    }
}

impl MapNav {
    pub fn new() -> Self {
        Self {
            parents: vec![0; MAX_CELLS].into_boxed_slice(),
            visited: vec![0; MAX_CELLS.div_ceil(64)].into_boxed_slice(),
            queue: vec![0; MAX_CELLS].into_boxed_slice(),
            active: None,
            last_work: 0,
        }
    }

    /// Replace the workspace's current request. Valid unequal endpoints start
    /// pending; expansion only happens in `advance`.
    pub fn begin(&mut self, map: &MapView<'_>, start: TilePos, target: TilePos) -> SearchResult {
        self.active = None;
        self.last_work = 0;
        let request = Request {
            start,
            target,
            width: map.width(),
            height: map.height(),
            view_id: map.instance_id(),
            revision: map.revision(),
        };
        if request.width > MAX_MAP_WIDTH
            || request.height > MAX_MAP_HEIGHT
            || !request.contains(start)
            || !request.contains(target)
            || !map.can_walk(start)
            || !map.can_walk(target)
        {
            return SearchResult::Invalid;
        }
        if start == target {
            return self.finish(request, 0);
        }
        let used_words = (usize::from(request.width) * usize::from(request.height)).div_ceil(64);
        self.visited[..used_words].fill(0);
        let start_index = request.index(start);
        self.mark_visited(start_index);
        self.queue[0] = start_index;
        self.active = Some(ActiveSearch {
            request,
            phase: Phase::Explore { head: 0, tail: 1 },
        });
        SearchResult::Pending
    }

    /// Spend at most `node_budget` units of search or route tracing work.
    /// A ready segment remains readable until the next request. A changed
    /// view or revision invalidates it, even when the caller has no budget.
    pub fn advance(&mut self, map: &MapView<'_>, node_budget: usize) -> SearchResult {
        self.last_work = 0;
        let Some(mut search) = self.active.take() else {
            return SearchResult::Invalid;
        };
        let request = search.request;
        if request.view_id != map.instance_id()
            || request.revision != map.revision()
            || request.width != map.width()
            || request.height != map.height()
        {
            return SearchResult::Stale;
        }
        if let Phase::Ready { len } = search.phase {
            return self.finish(request, len);
        }

        while self.last_work < node_budget {
            self.last_work += 1;
            match &mut search.phase {
                Phase::Explore { head, tail } => {
                    let index = self.queue[*head];
                    *head += 1;
                    let tile = request.tile(index);
                    if tile == request.target {
                        search.phase = Phase::Trace {
                            node: index,
                            len: 0,
                        };
                    } else {
                        self.expand(map, request, tile, index, tail);
                        if head == tail {
                            return SearchResult::Unreachable;
                        }
                    }
                }
                Phase::Trace { node, len } => {
                    self.queue[*len] = *node;
                    *len += 1;
                    *node = self.parents[usize::from(*node)];
                    if *node == request.index(request.start) {
                        return self.finish(request, *len);
                    }
                }
                Phase::Ready { .. } => unreachable!("ready segments return before the work loop"),
            }
        }

        self.active = Some(search);
        SearchResult::Pending
    }

    /// Units used by the latest call to `advance`, including route tracing.
    pub fn last_work(&self) -> usize {
        self.last_work
    }

    fn expand(
        &mut self,
        map: &MapView<'_>,
        request: Request,
        tile: TilePos,
        index: u16,
        tail: &mut usize,
    ) {
        for (dx, dy) in NEIGHBORS {
            let x = i32::from(tile.x) + dx;
            let y = i32::from(tile.y) + dy;
            if x < 0 || y < 0 || x >= i32::from(request.width) || y >= i32::from(request.height) {
                continue;
            }
            let next = TilePos::new(x as u16, y as u16);
            let next_index = request.index(next);
            let bit_index = usize::from(next_index);
            if self.visited[bit_index / 64] & (1 << (bit_index % 64)) != 0
                || !map.can_step(tile, next)
            {
                continue;
            }
            self.mark_visited(next_index);
            self.parents[bit_index] = index;
            self.queue[*tail] = next_index;
            *tail += 1;
        }
    }

    fn mark_visited(&mut self, index: u16) {
        let index = usize::from(index);
        self.visited[index / 64] |= 1 << (index % 64);
    }

    fn finish(&mut self, request: Request, total_edges: usize) -> SearchResult {
        self.active = Some(ActiveSearch {
            request,
            phase: Phase::Ready { len: total_edges },
        });
        let len = total_edges.min(MAX_SEGMENT_EDGES);
        let mut steps = [request.start; MAX_SEGMENT_EDGES];
        for (step, &index) in steps[..len]
            .iter_mut()
            .zip(self.queue[..total_edges].iter().rev())
        {
            *step = request.tile(index);
        }
        SearchResult::Found(PathSegment {
            start: request.start,
            target: request.target,
            end: steps[..len].last().copied().unwrap_or(request.start),
            view_id: request.view_id,
            revision: request.revision,
            total_edges,
            len,
            steps,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use super::*;
    use crate::map_data::{PhaseMask, WILLOW_VILLAGE};

    fn poi(id: &str) -> TilePos {
        WILLOW_VILLAGE.poi(id).unwrap().tile
    }

    fn reference(map: &MapView<'_>, start: TilePos, target: TilePos) -> Option<Vec<TilePos>> {
        let width = usize::from(map.width());
        let index = |p: TilePos| usize::from(p.y) * width + usize::from(p.x);
        let mut parent = vec![None; width * usize::from(map.height())];
        let mut queue = VecDeque::from([start]);
        parent[index(start)] = Some(start);
        while let Some(current) = queue.pop_front() {
            if current == target {
                let mut cursor = target;
                let mut path = Vec::new();
                while cursor != start {
                    path.push(cursor);
                    cursor = parent[index(cursor)].unwrap();
                }
                path.reverse();
                return Some(path);
            }
            let TilePos { x, y } = current;
            let neighbors = [
                (x + 1 < map.width()).then(|| TilePos::new(x + 1, y)),
                (y + 1 < map.height()).then(|| TilePos::new(x, y + 1)),
                x.checked_sub(1).map(|west| TilePos::new(west, y)),
                y.checked_sub(1).map(|north| TilePos::new(x, north)),
            ];
            for next in neighbors.into_iter().flatten() {
                if map.can_walk(next) && parent[index(next)].is_none() {
                    parent[index(next)] = Some(current);
                    queue.push_back(next);
                }
            }
        }
        None
    }

    fn search(
        nav: &mut MapNav,
        map: &MapView<'_>,
        start: TilePos,
        target: TilePos,
        budget: usize,
    ) -> SearchResult {
        let mut result = nav.begin(map, start, target);
        let mut used = 0;
        while result == SearchResult::Pending {
            result = nav.advance(map, budget);
            assert!(nav.last_work() <= budget);
            used += nav.last_work();
            assert!(used <= MAX_CELLS * 2);
        }
        result
    }

    #[test]
    fn cell_indices_round_trip_at_the_supported_extent() {
        let end = TilePos::new(MAX_MAP_WIDTH - 1, MAX_MAP_HEIGHT - 1);
        let request = Request {
            start: TilePos::new(0, 0),
            target: end,
            width: MAX_MAP_WIDTH,
            height: MAX_MAP_HEIGHT,
            view_id: 1,
            revision: 1,
        };
        assert!(request.contains(end));
        assert!(!request.contains(TilePos::new(MAX_MAP_WIDTH, MAX_MAP_HEIGHT)));
        let index = request.index(end);
        assert_eq!(usize::from(index), MAX_CELLS - 1);
        assert_eq!(request.tile(index), end);
    }

    #[test]
    fn splits_the_quarry_route_without_claiming_arrival() {
        let map = MapView::new(&WILLOW_VILLAGE, PhaseMask::DAY_ONE);
        let home = poi("home_xiaohe.door");
        let quarry = poi("quarry_work");
        let route = reference(&map, home, quarry).unwrap();
        assert_eq!(route.len(), 214);
        let mut nav = MapNav::new();
        let SearchResult::Found(first) = search(&mut nav, &map, home, quarry, 13) else {
            panic!("the quarry must be reachable");
        };
        assert_eq!(first.total_edges(), route.len());
        assert_eq!(first.steps(), &route[..MAX_SEGMENT_EDGES]);
        assert_eq!(first.target, quarry);
        assert!(!first.is_complete());
        assert_eq!(nav.advance(&map, 0), SearchResult::Found(first.clone()));
        assert_eq!(nav.last_work(), 0);
        let SearchResult::Found(last) = search(&mut nav, &map, first.end, first.target, 128) else {
            panic!("the segment end must lead to the original destination");
        };
        assert_eq!(last.steps(), &route[MAX_SEGMENT_EDGES..]);
        assert!(last.is_complete());
        assert_eq!(last.end, quarry);
    }

    #[test]
    fn matches_reference_on_seeded_pairs_at_different_budgets() {
        let map = MapView::new(&WILLOW_VILLAGE, PhaseMask::ALL);
        let mut nav = MapNav::new();
        let mut state = 0x219a_57d5u32;
        let mut sample = || loop {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let x = (state >> 16) as u16 % map.width();
            let y = (state & 0xffff) as u16 % map.height();
            let tile = TilePos::new(x, y);
            if map.can_walk(tile) {
                break tile;
            }
        };
        for _ in 0..16 {
            let start = sample();
            let target = sample();
            let expected = reference(&map, start, target);
            let single = search(&mut nav, &map, start, target, 1);
            assert_eq!(single, search(&mut nav, &map, start, target, 257));
            match (single, expected) {
                (SearchResult::Found(found), Some(route)) => {
                    assert_eq!(found.total_edges(), route.len());
                    assert_eq!(found.steps(), &route[..route.len().min(MAX_SEGMENT_EDGES)]);
                    for (from, to) in std::iter::once(start)
                        .chain(found.steps().iter().copied())
                        .zip(found.steps().iter().copied())
                    {
                        assert!(map.can_step(from, to));
                    }
                }
                (SearchResult::Unreachable, None) => (),
                (actual, expected) => panic!("reference mismatch: {actual:?} / {expected:?}"),
            }
        }
    }

    #[test]
    fn zero_budget_is_idle_and_topology_change_invalidates_pending_search() {
        let mut map = MapView::new(&WILLOW_VILLAGE, PhaseMask::DAY_ONE);
        let home = poi("home_xiaohe.door");
        let market = poi("market_gate");
        let mut nav = MapNav::new();
        assert_eq!(nav.begin(&map, home, market), SearchResult::Pending);
        for _ in 0..3 {
            assert_eq!(nav.advance(&map, 0), SearchResult::Pending);
            assert_eq!(nav.last_work(), 0);
        }
        assert_eq!(nav.advance(&map, 1), SearchResult::Pending);
        assert_eq!(nav.last_work(), 1);
        let before = map.revision();
        map.set_blocked(market, true);
        assert_ne!(before, map.revision());
        assert_eq!(nav.advance(&map, 0), SearchResult::Stale);
        assert_eq!(nav.last_work(), 0);
        assert_eq!(nav.advance(&map, 64), SearchResult::Invalid);
        assert_eq!(nav.begin(&map, home, market), SearchResult::Invalid);
        map.set_blocked(market, false);
        let SearchResult::Found(found) = search(&mut nav, &map, home, market, 64) else {
            panic!("reopening the market tile must restore its route");
        };
        assert_eq!(found.revision, map.revision());
    }

    #[test]
    fn a_second_view_with_the_same_revision_invalidates_search() {
        let mut first = MapView::new(&WILLOW_VILLAGE, PhaseMask::DAY_ONE);
        let mut second = MapView::new(&WILLOW_VILLAGE, PhaseMask::DAY_ONE);
        let home = poi("home_xiaohe.door");
        let market = poi("market_gate");
        assert!(first.set_blocked(poi("home_ayu.door"), true));
        assert!(second.set_blocked(market, true));
        assert_eq!(first.revision(), second.revision());
        let mut nav = MapNav::new();
        assert_eq!(nav.begin(&first, home, market), SearchResult::Pending);
        assert_eq!(nav.advance(&second, 1), SearchResult::Stale);
        assert_eq!(nav.last_work(), 0);
        let found = search(&mut nav, &first, home, market, 128);
        assert!(matches!(found, SearchResult::Found(_)));
        assert_eq!(nav.advance(&second, 0), SearchResult::Stale);
    }

    #[test]
    fn distinguishes_invalid_endpoints_from_an_exhausted_search() {
        let mut map = MapView::new(&WILLOW_VILLAGE, PhaseMask::DAY_ONE);
        let mut nav = MapNav::new();
        let home = poi("home_xiaohe.door");
        let market = poi("market_gate");
        for invalid in [
            TilePos::new(map.width(), 0),
            TilePos::new(0, map.height()),
            TilePos::new(62, 50),
            TilePos::new(u16::MAX, u16::MAX),
        ] {
            assert_eq!(nav.begin(&map, home, invalid), SearchResult::Invalid);
            assert_eq!(nav.begin(&map, invalid, home), SearchResult::Invalid);
        }
        let SearchResult::Found(same) = nav.begin(&map, home, home) else {
            panic!("a valid endpoint requires no steps to reach itself");
        };
        assert_eq!(same.steps(), &[]);
        assert!(same.is_complete());
        for (dx, dy) in NEIGHBORS {
            let next = TilePos::new(
                (i32::from(home.x) + dx) as u16,
                (i32::from(home.y) + dy) as u16,
            );
            if map.can_walk(next) {
                map.set_blocked(next, true);
            }
        }
        assert!(map.can_walk(home) && map.can_walk(market));
        assert_eq!(
            search(&mut nav, &map, home, market, 1),
            SearchResult::Unreachable
        );
    }
}
