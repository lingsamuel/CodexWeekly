use limit_core::settings::OverlayPosition;

pub const OVERLAY_SIZE: f64 = 176.0;
pub const DETAILS_WIDTH: f64 = 288.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailsSide {
    Left,
    Right,
}

impl DetailsSide {
    pub fn ring_offset(self, scale: f64) -> i32 {
        match self {
            Self::Left => (DETAILS_WIDTH * scale).round() as i32,
            Self::Right => 0,
        }
    }

    #[cfg(windows)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

#[derive(Clone, Copy)]
pub struct WorkArea {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

impl WorkArea {
    fn overlay_size(self) -> (u32, u32) {
        let edge = (OVERLAY_SIZE * self.scale).round() as u32;
        (edge, edge)
    }

    fn contains_overlay(self, point: OverlayPosition) -> bool {
        let (width, height) = self.overlay_size();
        // 延续跨屏摆放规则，但不依赖可能已被系统裁小的当前 HWND 尺寸。
        i64::from(point.x) + i64::from(width) >= i64::from(self.x) + 48
            && i64::from(point.y) + i64::from(height) >= i64::from(self.y) + 48
            && i64::from(point.x) + 48 <= i64::from(self.x) + i64::from(self.width)
            && i64::from(point.y) + 48 <= i64::from(self.y) + i64::from(self.height)
    }

    fn intersection(self, point: OverlayPosition) -> i64 {
        let (width, height) = self.overlay_size();
        let width = (i64::from(point.x) + i64::from(width))
            .min(i64::from(self.x) + i64::from(self.width))
            - i64::from(point.x).max(i64::from(self.x));
        let height = (i64::from(point.y) + i64::from(height))
            .min(i64::from(self.y) + i64::from(self.height))
            - i64::from(point.y).max(i64::from(self.y));
        width.max(0) * height.max(0)
    }
}

pub fn work_area(
    preferred: Option<OverlayPosition>,
    areas: &[WorkArea],
    primary: usize,
) -> Option<&WorkArea> {
    preferred
        .and_then(|point| {
            areas
                .iter()
                .filter(|area| area.contains_overlay(point))
                .max_by_key(|area| area.intersection(point))
        })
        .or_else(|| areas.get(primary))
        .or_else(|| areas.first())
}

pub fn placement(
    preferred: Option<OverlayPosition>,
    areas: &[WorkArea],
    primary: usize,
) -> Option<(OverlayPosition, (u32, u32))> {
    let area = work_area(preferred, areas, primary)?;
    if let Some(point) = preferred {
        if area.contains_overlay(point) {
            return Some((point, area.overlay_size()));
        }
    }
    let (width, height) = area.overlay_size();
    Some((
        OverlayPosition {
            x: (i64::from(area.x) + (i64::from(area.width) - i64::from(width) - 32).max(0)) as i32,
            y: (i64::from(area.y) + 40.min((i64::from(area.height) - i64::from(height)).max(0)))
                as i32,
        },
        (width, height),
    ))
}

pub fn details_side(anchor: OverlayPosition, area: WorkArea) -> DetailsSide {
    let left = i64::from(anchor.x) - i64::from(area.x);
    let right = i64::from(area.width) - left - i64::from(area.overlay_size().0);
    if right >= (DETAILS_WIDTH * area.scale).round() as i64 || right >= left {
        DetailsSide::Right
    } else {
        DetailsSide::Left
    }
}

#[derive(Default)]
pub struct DragState {
    active: bool,
    revision: u64,
    pending_saves: usize,
}

impl DragState {
    pub fn begin(&mut self) {
        self.active = true;
    }

    pub fn interrupt(&mut self) {
        self.active = false;
    }

    pub fn finish(&mut self) -> Option<u64> {
        if !std::mem::take(&mut self.active) {
            return None;
        }
        self.revision += 1;
        self.pending_saves += 1;
        Some(self.revision)
    }

    pub fn active(&self) -> bool {
        self.active
    }

    pub fn busy(&self) -> bool {
        self.active || self.pending_saves > 0
    }

    pub fn is_current(&self, revision: u64) -> bool {
        self.revision == revision
    }

    pub fn saved(&mut self) {
        self.pending_saves -= 1;
    }
}
