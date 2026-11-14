//! Optional embedded two-texel art. Coordinates in this index are source texels.
use serde::Deserialize;

const PAGE_SIDE: usize = 256;
const PAGE_BYTES: usize = PAGE_SIDE * PAGE_SIDE * 4;
const MAX_PAGES: usize = 16;
const MAX_FRAMES: usize = 1024;
const MAX_INDEX_BYTES: usize = 192 * 1024;

static PREMIUM_PAGES: [&[u8]; 11] = [
    include_bytes!("../assets/premium/page_00.bgra"),
    include_bytes!("../assets/premium/page_01.bgra"),
    include_bytes!("../assets/premium/page_02.bgra"),
    include_bytes!("../assets/premium/page_03.bgra"),
    include_bytes!("../assets/premium/page_04.bgra"),
    include_bytes!("../assets/premium/page_05.bgra"),
    include_bytes!("../assets/premium/page_06.bgra"),
    include_bytes!("../assets/premium/page_07.bgra"),
    include_bytes!("../assets/premium/page_08.bgra"),
    include_bytes!("../assets/premium/page_09.bgra"),
    include_bytes!("../assets/premium/page_10.bgra"),
];
static WILDLIFE_PAGES: [&[u8]; 1] = [include_bytes!("../assets/premium/wildlife/page_00.bgra")];
static VILLAGE_PAGES: [&[u8]; 4] = [
    include_bytes!("../assets/premium/village/page_00.bgra"),
    include_bytes!("../assets/premium/village/page_01.bgra"),
    include_bytes!("../assets/premium/village/page_02.bgra"),
    include_bytes!("../assets/premium/village/page_03.bgra"),
];

pub(super) fn builtin() -> Result<(ArtAtlas, ArtAtlas, ArtAtlas), &'static str> {
    Ok((
        ArtAtlas::load(
            include_bytes!("../assets/premium/index.json"),
            &PREMIUM_PAGES,
        )?,
        ArtAtlas::load(
            include_bytes!("../assets/premium/wildlife/index.json"),
            &WILDLIFE_PAGES,
        )?,
        ArtAtlas::load(
            include_bytes!("../assets/premium/village/index.json"),
            &VILLAGE_PAGES,
        )?,
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    width: u16,
    height: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ArtFrame {
    id: String,
    pub page: usize,
    pub rect: [u16; 4],
    pub anchor: [u16; 2],
    pub hand_anchor: Option<[u16; 2]>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    pages: Vec<Page>,
    frames: Vec<ArtFrame>,
}

pub(super) struct ArtAtlas {
    manifest: Manifest,
    pages: &'static [&'static [u8]],
}

impl ArtAtlas {
    pub fn load(index: &[u8], pages: &'static [&'static [u8]]) -> Result<Self, &'static str> {
        if index.len() > MAX_INDEX_BYTES || pages.len() > MAX_PAGES {
            return Err("premium atlas exceeds the resource limit");
        }
        let mut manifest: Manifest =
            serde_json::from_slice(index).map_err(|_| "invalid premium atlas index")?;
        if manifest.pages.len() != pages.len() || manifest.frames.len() > MAX_FRAMES {
            return Err("premium atlas page or frame count does not match");
        }
        for (page, bytes) in manifest.pages.iter().zip(pages) {
            if usize::from(page.width) != PAGE_SIDE
                || usize::from(page.height) != PAGE_SIDE
                || bytes.len() != PAGE_BYTES
            {
                return Err("invalid premium atlas page dimensions");
            }
        }
        manifest.frames.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        for (i, frame) in manifest.frames.iter().enumerate() {
            let [x, y, w, h] = frame.rect.map(usize::from);
            if frame.id.is_empty()
                || frame.id.len() > 96
                || frame.page >= pages.len()
                || w == 0
                || h == 0
                || x + w > PAGE_SIDE
                || y + h > PAGE_SIDE
                || usize::from(frame.anchor[0]) > w
                || usize::from(frame.anchor[1]) > h
                || frame
                    .hand_anchor
                    .is_some_and(|[hx, hy]| usize::from(hx) > w || usize::from(hy) > h)
                || i > 0 && manifest.frames[i - 1].id == frame.id
            {
                return Err("invalid premium atlas frame");
            }
        }
        Ok(Self { manifest, pages })
    }

    pub fn frame(&self, key: &str) -> Option<(&ArtFrame, &'static [u8])> {
        let frame = self.metadata(key)?;
        Some((frame, self.page(frame)))
    }

    pub(super) fn metadata(&self, key: &str) -> Option<&ArtFrame> {
        let index = self
            .manifest
            .frames
            .binary_search_by(|frame| frame.id.as_str().cmp(key))
            .ok()?;
        Some(&self.manifest.frames[index])
    }

    pub(super) fn page(&self, frame: &ArtFrame) -> &'static [u8] {
        self.pages[frame.page]
    }
}
