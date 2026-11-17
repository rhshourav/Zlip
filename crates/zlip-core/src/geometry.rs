//! Where the overlay opens. Pure arithmetic so it can be tested without a screen.

/// Screen rectangle in physical pixels (`right`/`bottom` exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    #[must_use]
    pub fn width(self) -> i32 {
        self.right - self.left
    }
    #[must_use]
    pub fn height(self) -> i32 {
        self.bottom - self.top
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Top-left for an overlay of `size` (w, h).
///
/// With a text caret, the panel opens just below it (or above, if there is no
/// room): the paste target is where the eyes already are. Otherwise it sits
/// bottom-centre of the work area with `gap` pixels of air. The result is
/// always clamped inside `work`.
#[must_use]
pub fn place_overlay(work: Rect, caret: Option<Rect>, size: (i32, i32), gap: i32) -> Point {
    let (w, h) = size;
    let (mut x, mut y) = match caret {
        Some(c) => {
            let below = c.bottom + gap;
            let y = if below + h <= work.bottom {
                below
            } else {
                c.top - gap - h
            };
            (c.left, y)
        }
        None => (work.left + (work.width() - w) / 2, work.bottom - h - gap),
    };
    x = x.min(work.right - w).max(work.left);
    y = y.min(work.bottom - h).max(work.top);
    Point { x, y }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };

    #[test]
    fn no_caret_is_bottom_centre() {
        let p = place_overlay(WORK, None, (440, 640), 24);
        assert_eq!(p, Point { x: 740, y: 376 });
    }

    #[test]
    fn opens_below_the_caret() {
        let caret = Rect {
            left: 300,
            top: 100,
            right: 302,
            bottom: 120,
        };
        assert_eq!(
            place_overlay(WORK, Some(caret), (440, 640), 8),
            Point { x: 300, y: 128 }
        );
    }

    #[test]
    fn flips_above_when_there_is_no_room_below() {
        let caret = Rect {
            left: 300,
            top: 700,
            right: 302,
            bottom: 720,
        };
        let p = place_overlay(WORK, Some(caret), (440, 640), 8);
        assert_eq!(p.y, 700 - 8 - 640);
    }

    #[test]
    fn clamps_to_the_work_area() {
        let caret = Rect {
            left: 1900,
            top: 10,
            right: 1902,
            bottom: 30,
        };
        let p = place_overlay(WORK, Some(caret), (440, 640), 8);
        assert_eq!(p.x, 1920 - 440);
    }

    #[test]
    fn handles_a_window_bigger_than_the_screen() {
        let tiny = Rect {
            left: 100,
            top: 50,
            right: 300,
            bottom: 250,
        };
        let p = place_overlay(tiny, None, (440, 640), 24);
        assert_eq!(
            p,
            Point { x: 100, y: 50 },
            "pins to the top-left instead of going off-screen"
        );
    }

    #[test]
    fn respects_a_secondary_monitor_offset() {
        let second = Rect {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        let p = place_overlay(second, None, (440, 640), 24);
        assert!(p.x >= -1920 && p.x + 440 <= 0);
    }
}
