use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtlasRegion {
    pub id: u32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug)]
pub struct TextureAtlas {
    width: u32,
    height: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
    next_id: u32,
    regions: HashMap<u32, AtlasRegion>,
}

impl TextureAtlas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            cursor_x: 0,
            cursor_y: 0,
            row_height: 0,
            next_id: 0,
            regions: HashMap::new(),
        }
    }

    pub fn allocate(&mut self, width: u32, height: u32) -> Option<AtlasRegion> {
        if width > self.width || height > self.height {
            return None;
        }

        if self.cursor_x + width > self.width {
            self.cursor_x = 0;
            self.cursor_y += self.row_height;
            self.row_height = 0;
        }

        if self.cursor_y + height > self.height {
            return None;
        }

        let region = AtlasRegion {
            id: self.next_id,
            x: self.cursor_x,
            y: self.cursor_y,
            width,
            height,
        };

        self.cursor_x += width;
        if height > self.row_height {
            self.row_height = height;
        }

        self.regions.insert(self.next_id, region);
        self.next_id += 1;

        Some(region)
    }

    pub fn get(&self, id: u32) -> Option<&AtlasRegion> {
        self.regions.get(&id)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn region_count(&self) -> usize {
        self.regions.len()
    }

    pub fn clear(&mut self) {
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.row_height = 0;
        self.next_id = 0;
        self.regions.clear();
    }
}
