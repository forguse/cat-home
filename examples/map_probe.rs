//! Isolated new-map geometry probe; never opens or migrates a player's save.
use cat_home::{
    map_data::{PhaseMask, WILLOW_VILLAGE},
    map_nav::{MapNav, SearchResult},
    map_view::MapView,
};
use serde_json::json;
use std::time::Instant;

fn main() {
    let map = MapView::new(&WILLOW_VILLAGE, PhaseMask::DAY_ONE);
    let start = map.poi_approach("home_xiaohe.door").unwrap();
    let mut search = MapNav::new();
    let mut trips = Vec::new();
    for destination in ["market_gate", "residence_seed_merchant.door", "quarry_work"] {
        let target = map.poi_approach(destination).unwrap();
        let mut position = start;
        let mut walked = 0;
        let mut segments = 0;
        let mut slices = 0;
        let mut max_work = 0;
        let begun = Instant::now();
        while position != target {
            assert!(segments < 8, "route must keep making bounded progress");
            let mut result = search.begin(&map, position, target);
            while result == SearchResult::Pending {
                result = search.advance(&map, 64);
                max_work = max_work.max(search.last_work());
                slices += 1;
            }
            let SearchResult::Found(segment) = result else {
                panic!("route to {destination} failed: {result:?}");
            };
            assert_eq!(segment.target, target);
            assert_eq!(segment.revision, map.revision());
            assert_eq!(segment.view_id, map.instance_id());
            for &next in segment.steps() {
                assert!(map.can_step(position, next), "invalid walking edge");
                position = next;
                walked += 1;
            }
            segments += 1;
            assert_eq!(position, segment.end);
            assert_eq!(segment.is_complete(), position == target);
        }
        trips.push(json!({"destination":destination,"walked_edges":walked,
            "segments":segments,"slices":slices,"max_work_per_slice":max_work,
            "search_and_edge_validation_us":begun.elapsed().as_micros()}));
    }
    let mut visible_small = Vec::new();
    let mut visible_large = Vec::new();
    map.for_each_visible_chunk(map.visible_tiles(80,96,160,100),|x,y| visible_small.push([x,y]));
    map.for_each_visible_chunk(map.visible_tiles(80,96,320,200),|x,y| visible_large.push([x,y]));
    assert!(map.poi_approach("wetland_observe").is_none());
    assert!(map.poi_approach("craft_carpentry.door").is_none());
    println!("{}", serde_json::to_string_pretty(&json!({
        "scope":"new-map geometry/navigation only; not desktop rendering or full simulation",
        "layout":WILLOW_VILLAGE.layout_id,"source_sha256":WILLOW_VILLAGE.source_sha256,
        "map_size":[map.width(),map.height()],"tiles":u32::from(map.width())*u32::from(map.height()),
        "trips":trips,"visible_chunks_small":visible_small,"visible_chunks_large":visible_large,
        "closed_future_services_rejected":true,"player_save_opened":false
    })).unwrap());
}
