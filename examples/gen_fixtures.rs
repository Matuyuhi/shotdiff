// Generates the demo screenshots used by the README: a believable iOS-style
// profile screen, rendered at 3x and downscaled for anti-aliasing. The BEFORE
// and AFTER frames are identical except for three natural changes:
//   * the primary button flips Follow (filled) -> Following (outlined)
//   * a notification badge appears on the bell
//   * the first card's thumbnail changes colour
//
//   cargo run --release --example gen_fixtures
use image::{Rgba, RgbaImage};

const S: i64 = 3; // supersample factor
const W: u32 = 390;
const H: u32 = 844;

type Col = [u8; 4];
const BG: Col = [242, 242, 247, 255];
const WHITE: Col = [255, 255, 255, 255];
const SEP: Col = [229, 229, 234, 255];
const INK: Col = [58, 58, 66, 255]; // "text"
const FAINT: Col = [199, 199, 204, 255]; // secondary "text"
const BLUE: Col = [0, 122, 255, 255];
const ORANGE: Col = [255, 149, 0, 255];
const PURPLE: Col = [175, 82, 222, 255];
const TEAL: Col = [48, 176, 199, 255];
const INDIGO: Col = [88, 86, 214, 255];
const RED: Col = [255, 59, 48, 255];

struct P {
    img: RgbaImage,
}

impl P {
    fn new() -> Self {
        let bg = Rgba(BG);
        P {
            img: RgbaImage::from_pixel(W * S as u32, H * S as u32, bg),
        }
    }

    fn put(&mut self, x: i64, y: i64, c: Col) {
        if x >= 0 && y >= 0 && (x as u32) < self.img.width() && (y as u32) < self.img.height() {
            self.img.put_pixel(x as u32, y as u32, Rgba(c));
        }
    }

    /// Filled rectangle in logical coordinates.
    fn rect(&mut self, x: i64, y: i64, w: i64, h: i64, c: Col) {
        for yy in (y * S)..((y + h) * S) {
            for xx in (x * S)..((x + w) * S) {
                self.put(xx, yy, c);
            }
        }
    }

    /// Rounded rectangle (logical coords, logical corner radius `r`).
    fn rrect(&mut self, x: i64, y: i64, w: i64, h: i64, r: i64, c: Col) {
        let (x0, y0) = (x * S, y * S);
        let (w, h, r) = (w * S, h * S, r * S);
        for yy in y0..(y0 + h) {
            for xx in x0..(x0 + w) {
                let dx = if xx < x0 + r {
                    x0 + r - xx
                } else if xx >= x0 + w - r {
                    xx - (x0 + w - r) + 1
                } else {
                    0
                };
                let dy = if yy < y0 + r {
                    y0 + r - yy
                } else if yy >= y0 + h - r {
                    yy - (y0 + h - r) + 1
                } else {
                    0
                };
                if dx * dx + dy * dy <= r * r {
                    self.put(xx, yy, c);
                }
            }
        }
    }

    /// Rounded-rectangle outline of thickness `t` (logical).
    #[allow(clippy::too_many_arguments)]
    fn rrect_outline(&mut self, x: i64, y: i64, w: i64, h: i64, r: i64, t: i64, c: Col) {
        self.rrect(x, y, w, h, r, c);
        // Carve the interior back out — caller draws onto a known background,
        // so punch with the background colour.
        self.rrect(x + t, y + t, w - 2 * t, h - 2 * t, (r - t).max(0), BG);
    }

    fn circle(&mut self, cx: i64, cy: i64, r: i64, c: Col) {
        let (cx, cy, r) = (cx * S, cy * S, r * S);
        for yy in (cy - r)..(cy + r) {
            for xx in (cx - r)..(cx + r) {
                let (dx, dy) = (xx - cx, yy - cy);
                if dx * dx + dy * dy <= r * r {
                    self.put(xx, yy, c);
                }
            }
        }
    }

    fn line(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, t: i64, c: Col) {
        let (mut x0, mut y0) = (x0 * S, y0 * S);
        let (x1, y1) = (x1 * S, y1 * S);
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
        let mut err = dx + dy;
        let rad = (t * S) / 2;
        loop {
            for yy in (y0 - rad)..=(y0 + rad) {
                for xx in (x0 - rad)..=(x0 + rad) {
                    self.put(xx, yy, c);
                }
            }
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    fn finish(self) -> RgbaImage {
        image::imageops::resize(&self.img, W, H, image::imageops::FilterType::Lanczos3)
    }
}

fn draw_screen(after: bool) -> RgbaImage {
    let mut p = P::new();

    // ----- status bar -----
    p.rrect(24, 17, 34, 13, 6, INK); // clock
    p.rect(330, 21, 18, 9, INK); // signal
    p.circle(360, 25, 6, INK); // wifi-ish
    p.rrect(372, 20, 14, 8, 2, INK); // battery

    // ----- app bar -----
    p.rect(0, 48, W as i64, 56, WHITE);
    p.rect(0, 103, W as i64, 1, SEP);
    p.line(28, 70, 20, 76, 3, BLUE); // back chevron
    p.line(20, 76, 28, 82, 3, BLUE);
    p.rrect(150, 70, 90, 13, 6, INK); // title
    // bell
    p.rrect(348, 66, 18, 16, 6, INK);
    p.rect(353, 82, 8, 3, INK);
    p.circle(357, 89, 2, INK);
    if after {
        p.circle(367, 65, 5, RED); // notification badge
    }

    // ----- profile header -----
    p.circle(195, 168, 44, INDIGO);
    p.circle(195, 152, 15, WHITE); // avatar head
    p.rrect(170, 178, 50, 30, 14, WHITE); // avatar body
    p.rrect(135, 232, 120, 15, 7, INK); // name
    p.rrect(155, 256, 80, 11, 5, FAINT); // handle

    // ----- stats row -----
    for (i, _) in [0, 1, 2].iter().enumerate() {
        let cx = 95 + i as i64 * 100;
        p.rrect(cx - 22, 286, 44, 14, 6, INK); // count
        p.rrect(cx - 30, 306, 60, 9, 4, FAINT); // label
    }

    // ----- primary button: Follow (filled) -> Following (outline) -----
    if after {
        p.rrect_outline(40, 338, 310, 50, 25, 3, BLUE);
        p.rrect(160, 357, 70, 12, 6, BLUE); // "Following"
    } else {
        p.rrect(40, 338, 310, 50, 25, BLUE);
        p.rrect(170, 357, 50, 12, 6, WHITE); // "Follow"
    }

    // ----- section header -----
    p.rrect(24, 416, 80, 12, 6, INK);

    // ----- cards -----
    let thumbs = [if after { PURPLE } else { ORANGE }, TEAL, BLUE];
    for (i, thumb) in thumbs.iter().enumerate() {
        let y = 444 + i as i64 * 104;
        p.rrect(16, y, 358, 92, 18, WHITE);
        p.rrect(32, y + 14, 64, 64, 16, *thumb); // thumbnail
        p.rrect(112, y + 22, 170, 13, 6, INK); // title
        p.rrect(112, y + 46, 120, 10, 5, FAINT); // subtitle
        p.line(352, y + 38, 358, y + 46, 2, FAINT); // chevron
        p.line(358, y + 46, 352, y + 54, 2, FAINT);
    }

    // ----- bottom tab bar -----
    p.rect(0, 788, W as i64, 1, SEP);
    p.rect(0, 789, W as i64, H as i64 - 789, WHITE);
    for i in 0..4 {
        let cx = 64 + i * 88;
        let c = if i == 0 { BLUE } else { FAINT };
        p.circle(cx, 812, 9, c);
    }

    p.finish()
}

fn main() {
    draw_screen(false).save("examples/before.png").unwrap();
    draw_screen(true).save("examples/after.png").unwrap();
    println!("wrote examples/before.png, examples/after.png ({W}x{H})");
}
