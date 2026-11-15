//! Fixed-camera map audit at the real widget scale; never modifies a user save.
use cat_home::living_view::{Camera, LivingView, premium_source_size};
use cat_home::living_world::LivingWorld;
use cat_home::residents::VillageLife;
use std::{fs, io::{self, Write}, path::Path};

const SITES: &[(&str, f32, f32)] = &[
    ("01-home",23.0,20.5), ("02-pond-east",29.0,33.0),
    ("03-home-east",43.0,32.0), ("04-farm",25.0,51.0),
    ("05-orchard",26.0,78.0), ("06-forest",24.0,108.0),
    ("07-bridge",63.0,42.0), ("08-creek",62.0,20.0),
    ("09-north-homes",104.0,20.0), ("10-market",91.0,48.0),
    ("11-craft",144.0,51.0), ("12-east-homes",139.0,80.0),
    ("13-commons",104.0,79.0), ("14-south-farm",113.0,109.0),
    ("15-quarry",165.0,18.0), ("16-east-woods",181.0,75.0),
    ("17-wetland",67.0,110.0), ("18-future",162.0,111.0),
    ("19-north-connector",103.0,34.0), ("20-entry",185.0,56.0),
];

fn main() -> io::Result<()> {
    let args:Vec<_> = std::env::args_os().collect();
    let out=Path::new(args.get(1).ok_or_else(||io::Error::other("map_density_preview OUTPUT"))?);
    fs::create_dir_all(out)?;
    let (world,life) = if let Some(path)=args.get(2) {
        let data:serde_json::Value=serde_json::from_slice(&fs::read(path)?).map_err(io::Error::other)?;
        if data["map_revision"].as_u64()!=Some(cat_home::map_data::WILLOW_VILLAGE.map_revision.into()) {
            return Err(io::Error::other("Use a migrated audit copy for the current map revision"));
        }
        (serde_json::from_value::<LivingWorld>(data["world"].clone()).map_err(io::Error::other)?,
         serde_json::from_value::<VillageLife>(data["life"].clone()).map_err(io::Error::other)?)
    } else {(LivingWorld::new_village_wall_ms(0),VillageLife::default())};
    if args.get(2).is_some() && (!world.validate() || !life.validate_for_world(&world)) {
        return Err(io::Error::other("Snapshot failed the current world and resident checks"));
    }
    let size=(724,540);
    let focus = match (args.get(3), args.get(4)) {
        (None, None) => None,
        (Some(x), Some(y)) => {
            let parse = |v: &std::ffi::OsString| -> io::Result<f32> {
                let n = v.to_string_lossy().parse::<f32>().map_err(io::Error::other)?;
                if !n.is_finite() { return Err(io::Error::other("finite tile coordinates required")); }
                Ok(n)
            };
            Some([("focus", parse(x)?, parse(y)?)])
        }
        _ => return Err(io::Error::other("map_density_preview OUTPUT [SAVE [TILE_X TILE_Y]]")),
    };
    let mut view=LivingView::new();
    view.enable_builtin_premium().map_err(io::Error::other)?;
    view.set_companionship_rewards(life.companionship_received());
    view.set_village_public_corner(life.social_public_corner());
    view.set_resident_presence(life.residents());
    view.set_village_menu(life.kitchen_menu(&world));
    view.set_village_art_boards(life.village_art_visuals(&world));
    use cat_home::living_world::ActorKind;
    view.set_village_household_payload([life.household_held(ActorKind::Xiaohe),life.household_held(ActorKind::Ayu)]);
    println!("map revision {} source {} phases {}",cat_home::map_data::WILLOW_VILLAGE.map_revision,cat_home::map_data::WILLOW_VILLAGE.source_sha256,world.phase_mask().bits());
    for &(name,x,y) in focus.as_ref().map_or(SITES, |s| s.as_slice()) {
        let mut camera=Camera{x:x*8.0-size.0 as f32/12.0,y:y*8.0-size.1 as f32/12.0,follow:None};
        camera.clamp_in_world(&world,cat_home::viewport::logical_size(size));
        view.render_village(&world,&camera,size,&[],None,&life.visuals(&world),&life.wildlife_visuals(&world),&life.stall_visuals(&world));
        let source=premium_source_size(size);
        let pixels=view.pixels();
        let mut f=io::BufWriter::new(fs::File::create(out.join(format!("{name}.pam")))?);
        write!(f,"P7\nWIDTH {}\nHEIGHT {}\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR\n",size.0,size.1)?;
        for y in 0..size.1 {
            let mut row=Vec::with_capacity(size.0 as usize*4);
            for x in 0..size.0 {
                let ix=cat_home::viewport::source_coordinate(x);
                let iy=cat_home::viewport::source_coordinate(y);
                let i=((iy*source.0+ix)*4) as usize;
                let p=&pixels[i..i+4];let a=u16::from(p[3]);
                for c in [2,1,0] {row.push(if a==0 {0} else {((u16::from(p[c])*255+a/2)/a).min(255) as u8});}
                row.push(p[3]);
            }
            f.write_all(&row)?;
        }
        f.flush()?;
        println!("{name}: origin {},{}",camera.x/8.0,camera.y/8.0);
    }
    Ok(())
}
