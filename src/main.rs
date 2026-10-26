//! shotdiff — side-by-side screenshot diff.
//!
//! Takes two image paths (BEFORE, AFTER) and produces a `BEFORE | DIFF | AFTER`
//! comparison where changed pixels are highlighted in pink. Two output modes:
//!   * PNG mode (default): writes a composite PNG. No browser/GUI — works in CI.
//!   * Browser mode (`--browser`): writes a self-contained HTML viewer with an
//!     interactive threshold slider and opens it in the default browser.
//!
//! Raster inputs are decoded by the `image` crate; SVG inputs are rasterised with
//! resvg before the pixel comparison, so both modes see plain RGBA either way.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, OnceLock};

use image::{Rgba, RgbaImage};
use resvg::{tiny_skia, usvg};

/// Pink used to paint changed pixels (#FF2D87).
const PINK: Rgba<u8> = Rgba([0xFF, 0x2D, 0x87, 0xFF]);
/// Gap (px) between panels in the composite, and its colour.
const GAP: u32 = 16;
const GAP_BG: Rgba<u8> = Rgba([0x1a, 0x1a, 0x1a, 0xFF]);
/// Fill colour for padded regions when the two inputs differ in size.
const PAD_BG: Rgba<u8> = Rgba([0x00, 0x00, 0x00, 0x00]);
/// Transparent — the default backdrop behind a rasterised SVG.
const TRANSPARENT: Rgba<u8> = Rgba([0x00, 0x00, 0x00, 0x00]);
/// Guards against absurd rasterisation targets (a 4x4 icon matched to a huge raster).
const MAX_SVG_DIM: u32 = 20_000;
const MAX_SVG_PIXELS: u64 = 100_000_000;

struct Args {
    before: PathBuf,
    after: PathBuf,
    output: PathBuf,
    threshold: u8,
    diff_only: bool,
    browser: bool,
    fail_on_diff: bool,
    /// Max tolerated change before failing: (value, is_percent).
    max_diff: Option<(f64, bool)>,
    /// Uniform scale for SVG inputs; `None` = auto (match the raster it is compared to).
    svg_scale: Option<f32>,
    /// Colour painted behind a rasterised SVG.
    svg_background: Rgba<u8>,
}

const USAGE: &str = "\
shotdiff — side-by-side screenshot diff (BEFORE | DIFF | AFTER, changes in pink)

USAGE:
    shotdiff <BEFORE> <AFTER> [OPTIONS]

ARGS:
    <BEFORE>    Path to the first/old image
    <AFTER>     Path to the second/new image

    Raster (PNG/JPEG/WebP/GIF/BMP) and vector (SVG/SVGZ) inputs may be mixed.

OPTIONS:
    -o, --output <PATH>       Output PNG path (default: shotdiff-out.png)
    -t, --threshold <N>       Per-channel diff threshold 0-255 (default: 16)
        --diff-only           Output only the centre DIFF panel
        --browser             Build an interactive HTML viewer and open it
        --fail-on-diff        Exit 1 if any pixel changed (visual-regression gate)
        --max-diff <V>        Fail only if change exceeds V; e.g. 1200 or 0.5%
        --svg-scale <F>       Rasterise SVG input at F x its intrinsic size.
                              Default: auto — an SVG compared against a raster is
                              scaled to that raster's width; otherwise 1.
        --svg-background <C>  Backdrop behind rasterised SVG: transparent (default),
                              white, black, #rgb, #rrggbb or #rrggbbaa
    -h, --help                Show this help

EXIT CODES:
    0  success (no diff, or diff but no failure gate triggered)
    1  diff exceeded the configured gate (--fail-on-diff / --max-diff)
    2  usage or I/O error
";

fn parse_args() -> Result<Args, String> {
    let mut before: Option<PathBuf> = None;
    let mut after: Option<PathBuf> = None;
    let mut output: Option<PathBuf> = None;
    let mut threshold: u8 = 16;
    let mut diff_only = false;
    let mut browser = false;
    let mut fail_on_diff = false;
    let mut max_diff: Option<(f64, bool)> = None;
    let mut svg_scale: Option<f32> = None;
    let mut svg_background = TRANSPARENT;

    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                std::process::exit(0);
            }
            "-o" | "--output" => {
                output = Some(PathBuf::from(it.next().ok_or("--output requires a value")?));
            }
            "-t" | "--threshold" => {
                let v = it.next().ok_or("--threshold requires a value")?;
                threshold = v
                    .parse()
                    .map_err(|_| format!("invalid --threshold: {v} (expected 0-255)"))?;
            }
            "--diff-only" => diff_only = true,
            "--browser" => browser = true,
            "--fail-on-diff" => fail_on_diff = true,
            "--max-diff" => {
                let v = it.next().ok_or("--max-diff requires a value")?;
                max_diff = Some(parse_max_diff(&v)?);
            }
            "--svg-scale" => {
                let v = it.next().ok_or("--svg-scale requires a value")?;
                svg_scale = Some(parse_svg_scale(&v)?);
            }
            "--svg-background" => {
                let v = it.next().ok_or("--svg-background requires a value")?;
                svg_background = parse_color(&v)?;
            }
            s if s.starts_with('-') && s != "-" => {
                return Err(format!("unknown option: {s}"));
            }
            _ => {
                if before.is_none() {
                    before = Some(PathBuf::from(arg));
                } else if after.is_none() {
                    after = Some(PathBuf::from(arg));
                } else {
                    return Err(format!("unexpected extra argument: {arg}"));
                }
            }
        }
    }

    Ok(Args {
        before: before.ok_or("missing <BEFORE> image path")?,
        after: after.ok_or("missing <AFTER> image path")?,
        output: output.unwrap_or_else(|| PathBuf::from("shotdiff-out.png")),
        threshold,
        diff_only,
        browser,
        fail_on_diff,
        max_diff,
        svg_scale,
        svg_background,
    })
}

fn parse_max_diff(v: &str) -> Result<(f64, bool), String> {
    if let Some(p) = v.strip_suffix('%') {
        let n: f64 = p
            .trim()
            .parse()
            .map_err(|_| format!("invalid --max-diff percent: {v}"))?;
        Ok((n, true))
    } else {
        let n: f64 = v
            .parse()
            .map_err(|_| format!("invalid --max-diff count: {v}"))?;
        Ok((n, false))
    }
}

fn parse_svg_scale(v: &str) -> Result<f32, String> {
    let n: f32 = v.parse().map_err(|_| format!("invalid --svg-scale: {v}"))?;
    if !n.is_finite() || n <= 0.0 {
        return Err(format!(
            "invalid --svg-scale: {v} (expected a positive number)"
        ));
    }
    Ok(n)
}

/// `transparent` / `none` / `white` / `black` / `#rgb` / `#rrggbb` / `#rrggbbaa`.
fn parse_color(v: &str) -> Result<Rgba<u8>, String> {
    let s = v.trim();
    match s.to_ascii_lowercase().as_str() {
        "transparent" | "none" => return Ok(TRANSPARENT),
        "white" => return Ok(Rgba([0xFF, 0xFF, 0xFF, 0xFF])),
        "black" => return Ok(Rgba([0x00, 0x00, 0x00, 0xFF])),
        _ => {}
    }
    let hex = s.strip_prefix('#').unwrap_or(s);
    let nib = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
    let bytes = hex.as_bytes();
    let chan: Vec<u8> = match bytes.len() {
        // #rgb / #rgba — each nibble doubled, as in CSS.
        3 | 4 => bytes
            .iter()
            .map(|&c| nib(c).map(|d| d * 0x11))
            .collect::<Option<_>>(),
        6 | 8 => bytes
            .chunks(2)
            .map(|p| Some(nib(p[0])? * 16 + nib(p[1])?))
            .collect::<Option<_>>(),
        _ => None,
    }
    .ok_or_else(|| format!("invalid colour: {v}"))?;
    Ok(Rgba([
        chan[0],
        chan[1],
        chan[2],
        *chan.get(3).unwrap_or(&0xFF),
    ]))
}

// ---------- input loading ----------

/// System fonts, loaded once and shared by every SVG parse in the run.
fn font_database() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        db.load_system_fonts();
        Arc::new(db)
    })
    .clone()
}

/// Cheap sniff for SVG/SVGZ, so we only hand XML to usvg and bytes to `image`.
fn looks_like_svg(bytes: &[u8]) -> bool {
    // .svgz is gzip; no raster format the `image` crate decodes starts with that magic.
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return true;
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let head = head.trim_start_matches('\u{feff}').trim_start();
    head.starts_with("<svg")
        || head.starts_with("<?xml")
        || head.starts_with("<!--")
        || head.starts_with("<!DOCTYPE")
}

/// One CLI-provided image, held in its source form until we know what size a
/// vector input should be rasterised at.
struct Input {
    path: PathBuf,
    bytes: Vec<u8>,
    /// `Some` for SVG input; `None` for raster formats decoded by the `image` crate.
    tree: Option<usvg::Tree>,
    /// Intrinsic size: SVG user units, or the raster's pixel dimensions.
    size: (f32, f32),
}

impl Input {
    fn load(path: &Path) -> Result<Self, String> {
        let bytes =
            std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;

        if looks_like_svg(&bytes) {
            let mut opt = usvg::Options {
                // Lets <image href="logo.png"> and friends resolve next to the SVG.
                resources_dir: path.parent().map(Path::to_path_buf),
                ..Default::default()
            };
            opt.fontdb = font_database();
            let tree = usvg::Tree::from_data(&bytes, &opt)
                .map_err(|e| format!("failed to parse SVG {}: {e}", path.display()))?;
            let size = (tree.size().width(), tree.size().height());
            return Ok(Self {
                path: path.to_path_buf(),
                bytes,
                tree: Some(tree),
                size,
            });
        }

        let (w, h) = image::ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?
            .into_dimensions()
            .map_err(|e| format!("failed to read {}: {e}", path.display()))?;
        Ok(Self {
            path: path.to_path_buf(),
            bytes,
            tree: None,
            size: (w as f32, h as f32),
        })
    }

    fn is_svg(&self) -> bool {
        self.tree.is_some()
    }

    /// Rasterise (SVG) or decode (raster) into RGBA. `scale` applies to SVG only.
    fn to_rgba(&self, scale: f32, bg: Rgba<u8>) -> Result<RgbaImage, String> {
        match &self.tree {
            Some(tree) => rasterize_svg(tree, scale, bg)
                .map_err(|e| format!("failed to render SVG {}: {e}", self.path.display())),
            None => Ok(image::load_from_memory(&self.bytes)
                .map_err(|e| format!("failed to decode {}: {e}", self.path.display()))?
                .to_rgba8()),
        }
    }
}

/// Render an SVG tree at `scale`, over `bg`, into an un-premultiplied RGBA image.
fn rasterize_svg(tree: &usvg::Tree, scale: f32, bg: Rgba<u8>) -> Result<RgbaImage, String> {
    let w = (tree.size().width() * scale).round().max(1.0);
    let h = (tree.size().height() * scale).round().max(1.0);
    if w > MAX_SVG_DIM as f32 || h > MAX_SVG_DIM as f32 || w * h > MAX_SVG_PIXELS as f32 {
        return Err(format!(
            "rasterised size {w:.0}x{h:.0} is too large (scale {scale}); lower --svg-scale"
        ));
    }
    let (w, h) = (w as u32, h as u32);

    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("could not allocate the render target")?;
    if bg[3] != 0 {
        pixmap.fill(tiny_skia::Color::from_rgba8(bg[0], bg[1], bg[2], bg[3]));
    }
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );

    // tiny-skia stores premultiplied colour; the diff works on straight RGBA.
    let mut out = RgbaImage::new(w, h);
    for (src, dst) in pixmap.pixels().iter().zip(out.pixels_mut()) {
        let c = src.demultiply();
        *dst = Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Ok(out)
}

/// Rasterisation scale for each input. An explicit `--svg-scale` wins; otherwise a
/// lone SVG is matched to the width of the raster it is compared against, so the two
/// line up instead of diffing as one big block of pink.
fn svg_scales(a: &Input, b: &Input, explicit: Option<f32>) -> (f32, f32) {
    if let Some(s) = explicit {
        return (s, s);
    }
    let fit = |svg_w: f32, raster_w: f32| {
        if svg_w > 0.0 { raster_w / svg_w } else { 1.0 }
    };
    match (a.is_svg(), b.is_svg()) {
        (true, false) => (fit(a.size.0, b.size.0), 1.0),
        (false, true) => (1.0, fit(b.size.0, a.size.0)),
        _ => (1.0, 1.0),
    }
}

/// Load both inputs, resolving SVG scales against each other.
fn load_pair(args: &Args) -> Result<(Input, Input, f32, f32), String> {
    let a = Input::load(&args.before)?;
    let b = Input::load(&args.after)?;
    let (sa, sb) = svg_scales(&a, &b, args.svg_scale);
    for (input, scale) in [(&a, sa), (&b, sb)] {
        if input.is_svg() {
            eprintln!(
                "shotdiff: rasterising {} at {:.3}x ({:.0}x{:.0} -> {:.0}x{:.0})",
                input.path.display(),
                scale,
                input.size.0,
                input.size.1,
                (input.size.0 * scale).round().max(1.0),
                (input.size.1 * scale).round().max(1.0),
            );
        }
    }
    Ok((a, b, sa, sb))
}

/// Per-pixel max absolute channel difference across RGBA.
#[inline]
fn pixel_delta(a: &[u8; 4], b: &[u8; 4]) -> u8 {
    std::iter::zip(a, b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0)
}

/// Place `src` at the top-left of a `w`x`h` canvas filled with `bg`.
fn pad_to(src: &RgbaImage, w: u32, h: u32, bg: Rgba<u8>) -> RgbaImage {
    if src.width() == w && src.height() == h {
        return src.clone();
    }
    let mut out = RgbaImage::from_pixel(w, h, bg);
    image::imageops::replace(&mut out, src, 0, 0);
    out
}

/// Build the DIFF panel: a dimmed greyscale of `after` as context, with changed
/// pixels painted pink. Returns the panel and the changed-pixel count.
fn build_diff(a: &RgbaImage, b: &RgbaImage, threshold: u8) -> (RgbaImage, u64) {
    let (w, h) = a.dimensions();
    let mut out = RgbaImage::new(w, h);
    let mut changed = 0u64;
    for ((pa, pb), out_px) in a.pixels().zip(b.pixels()).zip(out.pixels_mut()) {
        if pixel_delta(&pa.0, &pb.0) > threshold {
            changed += 1;
            *out_px = PINK;
        } else {
            // Lightened greyscale of the AFTER pixel as quiet context (128..=255).
            let [r, g, bl, _] = pb.0;
            let luma = (0.299 * r as f32 + 0.587 * g as f32 + 0.114 * bl as f32) as u32;
            let v = (128 + luma / 2).min(255) as u8;
            *out_px = Rgba([v, v, v, 0xFF]);
        }
    }
    (out, changed)
}

/// Concatenate panels horizontally with a coloured gap between them.
fn hcat(panels: &[&RgbaImage]) -> RgbaImage {
    let h = panels.iter().map(|p| p.height()).max().unwrap_or(0);
    let total_w: u32 = panels.iter().map(|p| p.width()).sum::<u32>()
        + GAP * (panels.len().saturating_sub(1)) as u32;
    let mut out = RgbaImage::from_pixel(total_w, h, GAP_BG);
    let mut x: i64 = 0;
    for p in panels {
        image::imageops::replace(&mut out, *p, x, 0);
        x += (p.width() + GAP) as i64;
    }
    out
}

struct DiffResult {
    changed: u64,
    total: u64,
}

impl DiffResult {
    fn percent(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.changed as f64 / self.total as f64 * 100.0
        }
    }

    /// Whether the configured gate should fail the run.
    fn fails(&self, args: &Args) -> bool {
        match args.max_diff {
            Some((v, true)) => self.percent() > v,
            Some((v, false)) => self.changed as f64 > v,
            None => args.fail_on_diff && self.changed > 0,
        }
    }
}

fn run_png(args: &Args) -> Result<DiffResult, String> {
    let (in_a, in_b, scale_a, scale_b) = load_pair(args)?;
    let img_a = in_a.to_rgba(scale_a, args.svg_background)?;
    let img_b = in_b.to_rgba(scale_b, args.svg_background)?;

    let w = img_a.width().max(img_b.width());
    let h = img_a.height().max(img_b.height());
    let pa = pad_to(&img_a, w, h, PAD_BG);
    let pb = pad_to(&img_b, w, h, PAD_BG);

    let (diff, changed) = build_diff(&pa, &pb, args.threshold);
    let total = (w as u64) * (h as u64);

    let composite = if args.diff_only {
        diff
    } else {
        hcat(&[&pa, &diff, &pb])
    };

    composite
        .save_with_format(&args.output, image::ImageFormat::Png)
        .map_err(|e| format!("failed to write {}: {e}", args.output.display()))?;

    let res = DiffResult { changed, total };
    eprintln!(
        "shotdiff: {}x{} (before {}x{}, after {}x{})",
        w,
        h,
        img_a.width(),
        img_a.height(),
        img_b.width(),
        img_b.height()
    );
    if img_a.dimensions() != img_b.dimensions() {
        eprintln!("shotdiff: WARNING size mismatch — aligned top-left, padded to fit");
    }
    eprintln!(
        "shotdiff: {} / {} pixels changed ({:.3}%) at threshold {}",
        res.changed,
        res.total,
        res.percent(),
        args.threshold
    );
    eprintln!("shotdiff: wrote {}", args.output.display());
    Ok(res)
}

// ---------- browser mode ----------

fn b64(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(T[(n >> 18 & 63) as usize] as char);
        out.push(T[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn sniff_mime(data: &[u8]) -> &'static str {
    if data.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png"
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if data.starts_with(b"GIF8") {
        "image/gif"
    } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        "image/webp"
    } else if data.starts_with(b"BM") {
        "image/bmp"
    } else {
        "application/octet-stream"
    }
}

/// Raster inputs are embedded byte-for-byte; SVG input is rasterised first so the
/// viewer diffs exactly the pixels PNG mode would.
fn data_uri(input: &Input, scale: f32, bg: Rgba<u8>) -> Result<String, String> {
    if input.is_svg() {
        let img = input.to_rgba(scale, bg)?;
        let mut png = Cursor::new(Vec::new());
        img.write_to(&mut png, image::ImageFormat::Png)
            .map_err(|e| format!("failed to encode {}: {e}", input.path.display()))?;
        return Ok(format!("data:image/png;base64,{}", b64(&png.into_inner())));
    }
    Ok(format!(
        "data:{};base64,{}",
        sniff_mime(&input.bytes),
        b64(&input.bytes)
    ))
}

fn run_browser(args: &Args) -> Result<(), String> {
    let (in_a, in_b, scale_a, scale_b) = load_pair(args)?;
    let uri_a = data_uri(&in_a, scale_a, args.svg_background)?;
    let uri_b = data_uri(&in_b, scale_b, args.svg_background)?;
    let html = build_html(&uri_a, &uri_b, args.threshold);

    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("shotdiff-{pid}.html"));
    std::fs::write(&path, html).map_err(|e| format!("failed to write {}: {e}", path.display()))?;
    eprintln!("shotdiff: wrote {}", path.display());

    open_in_browser(&path)
}

fn open_in_browser(path: &Path) -> Result<(), String> {
    let p = path.to_string_lossy().to_string();
    let candidates: &[&[&str]] = if cfg!(target_os = "macos") {
        &[&["open"]]
    } else if cfg!(target_os = "windows") {
        &[&["cmd", "/C", "start", ""]]
    } else {
        &[&["xdg-open"], &["sensible-browser"], &["www-browser"]]
    };
    for cmd in candidates {
        let mut c = std::process::Command::new(cmd[0]);
        c.args(&cmd[1..]).arg(&p);
        if c.status().map(|s| s.success()).unwrap_or(false) {
            return Ok(());
        }
    }
    eprintln!("shotdiff: could not auto-open a browser; open this file manually:\n  {p}");
    Ok(())
}

fn build_html(uri_a: &str, uri_b: &str, threshold: u8) -> String {
    // Braces in CSS/JS are doubled for format!.
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>shotdiff</title>
<style>
  :root {{ --pink: #FF2D87; }}
  * {{ box-sizing: border-box; }}
  body {{ margin: 0; background: #111; color: #ddd; font: 14px/1.4 -apple-system, system-ui, sans-serif; }}
  header {{ position: sticky; top: 0; z-index: 2; display: flex; gap: 20px; align-items: center;
           flex-wrap: wrap; padding: 12px 16px; background: #1c1c1c; border-bottom: 1px solid #333; }}
  header h1 {{ font-size: 14px; margin: 0; color: var(--pink); letter-spacing: .5px; }}
  label {{ display: inline-flex; align-items: center; gap: 8px; white-space: nowrap; }}
  output {{ min-width: 2.5em; color: #fff; font-variant-numeric: tabular-nums; }}
  #stats {{ margin-left: auto; color: #9aa; font-variant-numeric: tabular-nums; }}
  .row {{ display: flex; gap: 16px; align-items: flex-start; padding: 16px; overflow: auto; }}
  figure {{ margin: 0; flex: 0 0 auto; }}
  figcaption {{ text-align: center; padding: 4px; color: #888; letter-spacing: 1px; font-size: 12px; }}
  canvas {{ display: block; background: #222; image-rendering: pixelated;
            box-shadow: 0 0 0 1px #333; }}
</style>
</head>
<body>
<header>
  <h1>shotdiff</h1>
  <label>threshold
    <input id="th" type="range" min="0" max="255" value="{threshold}">
    <output id="thv">{threshold}</output>
  </label>
  <label>zoom
    <input id="zoom" type="range" min="10" max="200" value="100">
    <output id="zoomv">100%</output>
  </label>
  <label><input id="ctx" type="checkbox" checked> grey context</label>
  <span id="stats">computing…</span>
</header>
<div class="row">
  <figure><figcaption>BEFORE</figcaption><canvas id="ca"></canvas></figure>
  <figure><figcaption>DIFF</figcaption><canvas id="cd"></canvas></figure>
  <figure><figcaption>AFTER</figcaption><canvas id="cb"></canvas></figure>
</div>
<script>
const SRC_A = "{uri_a}";
const SRC_B = "{uri_b}";
const PINK = [255, 45, 135];

function load(src) {{
  return new Promise((res, rej) => {{
    const im = new Image();
    im.onload = () => res(im);
    im.onerror = () => rej(new Error("failed to decode image"));
    im.src = src;
  }});
}}

function padData(im, W, H) {{
  const c = document.createElement("canvas");
  c.width = W; c.height = H;
  const x = c.getContext("2d");
  x.drawImage(im, 0, 0);
  return x.getImageData(0, 0, W, H);
}}

(async () => {{
  let ia, ib;
  try {{ [ia, ib] = await Promise.all([load(SRC_A), load(SRC_B)]); }}
  catch (e) {{ document.getElementById("stats").textContent = "error: " + e.message; return; }}

  const W = Math.max(ia.naturalWidth, ib.naturalWidth);
  const H = Math.max(ia.naturalHeight, ib.naturalHeight);
  const da = padData(ia, W, H);
  const db = padData(ib, W, H);

  const ca = document.getElementById("ca");
  const cb = document.getElementById("cb");
  const cd = document.getElementById("cd");
  for (const c of [ca, cb, cd]) {{ c.width = W; c.height = H; }}
  ca.getContext("2d").putImageData(da, 0, 0);
  cb.getContext("2d").putImageData(db, 0, 0);
  const xd = cd.getContext("2d");
  const out = xd.createImageData(W, H);

  const sizeNote = (ia.naturalWidth !== ib.naturalWidth || ia.naturalHeight !== ib.naturalHeight)
    ? `  ⚠ size mismatch (${{ia.naturalWidth}}×${{ia.naturalHeight}} vs ${{ib.naturalWidth}}×${{ib.naturalHeight}})` : "";

  function compute() {{
    const th = +document.getElementById("th").value;
    const grey = document.getElementById("ctx").checked;
    const A = da.data, B = db.data, O = out.data;
    let changed = 0;
    for (let i = 0; i < A.length; i += 4) {{
      const d = Math.max(
        Math.abs(A[i] - B[i]),
        Math.abs(A[i+1] - B[i+1]),
        Math.abs(A[i+2] - B[i+2]),
        Math.abs(A[i+3] - B[i+3]));
      if (d > th) {{
        changed++;
        O[i] = PINK[0]; O[i+1] = PINK[1]; O[i+2] = PINK[2]; O[i+3] = 255;
      }} else if (grey) {{
        const v = Math.min(255, 128 + ((0.299*B[i] + 0.587*B[i+1] + 0.114*B[i+2]) | 0) / 2) | 0;
        O[i] = O[i+1] = O[i+2] = v; O[i+3] = 255;
      }} else {{
        O[i] = B[i]; O[i+1] = B[i+1]; O[i+2] = B[i+2]; O[i+3] = 255;
      }}
    }}
    xd.putImageData(out, 0, 0);
    const total = W * H;
    const pct = total ? (changed / total * 100) : 0;
    document.getElementById("stats").textContent =
      `${{W}}×${{H}} · ${{changed.toLocaleString()}} / ${{total.toLocaleString()}} px changed (${{pct.toFixed(3)}}%)` + sizeNote;
  }}

  function applyZoom() {{
    const z = +document.getElementById("zoom").value / 100;
    for (const c of [ca, cb, cd]) {{ c.style.width = (W * z) + "px"; c.style.height = (H * z) + "px"; }}
    document.getElementById("zoomv").textContent = Math.round(z * 100) + "%";
  }}

  document.getElementById("th").addEventListener("input", e => {{
    document.getElementById("thv").textContent = e.target.value; compute();
  }});
  document.getElementById("ctx").addEventListener("change", compute);
  document.getElementById("zoom").addEventListener("input", applyZoom);

  applyZoom();
  compute();
}})();
</script>
</body>
</html>
"#
    )
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("shotdiff: {e}\n\n{USAGE}");
            return ExitCode::from(2);
        }
    };

    if args.browser {
        return match run_browser(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("shotdiff: {e}");
                ExitCode::from(2)
            }
        };
    }

    match run_png(&args) {
        Ok(res) => {
            if res.fails(&args) {
                eprintln!("shotdiff: FAIL — change exceeded the configured gate");
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        Err(e) => {
            eprintln!("shotdiff: {e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_is_max_abs_channel() {
        assert_eq!(pixel_delta(&[10, 20, 30, 255], &[10, 20, 30, 255]), 0);
        assert_eq!(pixel_delta(&[0, 0, 0, 255], &[5, 0, 200, 255]), 200);
        assert_eq!(pixel_delta(&[255, 255, 255, 255], &[255, 255, 255, 0]), 255);
    }

    #[test]
    fn max_diff_parses_count_and_percent() {
        assert_eq!(parse_max_diff("1200").unwrap(), (1200.0, false));
        assert_eq!(parse_max_diff("0.5%").unwrap(), (0.5, true));
        assert!(parse_max_diff("nope").is_err());
        assert!(parse_max_diff("x%").is_err());
    }

    #[test]
    fn gate_logic() {
        let base = |max_diff, fail_on_diff| Args {
            before: PathBuf::new(),
            after: PathBuf::new(),
            output: PathBuf::new(),
            threshold: 16,
            diff_only: false,
            browser: false,
            fail_on_diff,
            max_diff,
            svg_scale: None,
            svg_background: TRANSPARENT,
        };
        let res = DiffResult {
            changed: 50,
            total: 10_000,
        }; // 0.5%
        assert!(!res.fails(&base(None, false)), "no gate → never fails");
        assert!(
            res.fails(&base(None, true)),
            "fail-on-diff trips on any change"
        );
        assert!(
            res.fails(&base(Some((0.2, true)), false)),
            "0.5% > 0.2% → fail"
        );
        assert!(
            !res.fails(&base(Some((1.0, true)), false)),
            "0.5% <= 1% → ok"
        );
        assert!(
            res.fails(&base(Some((49.0, false)), false)),
            "50 > 49 → fail"
        );
        assert!(
            !res.fails(&base(Some((50.0, false)), false)),
            "50 <= 50 → ok"
        );

        let clean = DiffResult {
            changed: 0,
            total: 10_000,
        };
        assert!(
            !clean.fails(&base(None, true)),
            "0 change → fail-on-diff ok"
        );
    }

    const SQUARE_SVG: &[u8] =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20" fill="#00f"/></svg>"##;

    #[test]
    fn svg_is_sniffed_from_content_not_extension() {
        assert!(looks_like_svg(SQUARE_SVG));
        assert!(looks_like_svg(
            b"\xEF\xBB\xBF\n  <?xml version=\"1.0\"?><svg/>"
        ));
        assert!(looks_like_svg(&[0x1f, 0x8b, 0x08, 0x00])); // .svgz
        assert!(!looks_like_svg(&[0x89, b'P', b'N', b'G']));
        assert!(!looks_like_svg(b""));
    }

    #[test]
    fn colours_parse() {
        assert_eq!(parse_color("transparent").unwrap(), TRANSPARENT);
        assert_eq!(parse_color("white").unwrap(), Rgba([255, 255, 255, 255]));
        assert_eq!(parse_color("#f00").unwrap(), Rgba([255, 0, 0, 255]));
        assert_eq!(
            parse_color("#12345678").unwrap(),
            Rgba([0x12, 0x34, 0x56, 0x78])
        );
        assert_eq!(parse_color("00FF7F").unwrap(), Rgba([0, 255, 127, 255]));
        assert!(parse_color("#gg0000").is_err());
        assert!(parse_color("#12345").is_err());
    }

    #[test]
    fn svg_scale_parses() {
        assert_eq!(parse_svg_scale("2").unwrap(), 2.0);
        assert_eq!(parse_svg_scale("0.5").unwrap(), 0.5);
        assert!(parse_svg_scale("0").is_err());
        assert!(parse_svg_scale("-1").is_err());
        assert!(parse_svg_scale("big").is_err());
    }

    fn svg_input(bytes: &[u8]) -> Input {
        let opt = usvg::Options::default();
        let tree = usvg::Tree::from_data(bytes, &opt).unwrap();
        let size = (tree.size().width(), tree.size().height());
        Input {
            path: PathBuf::from("mem.svg"),
            bytes: bytes.to_vec(),
            tree: Some(tree),
            size,
        }
    }

    fn raster_input(w: f32, h: f32) -> Input {
        Input {
            path: PathBuf::from("mem.png"),
            bytes: Vec::new(),
            tree: None,
            size: (w, h),
        }
    }

    #[test]
    fn lone_svg_is_scaled_to_the_raster_it_is_compared_with() {
        let svg = svg_input(SQUARE_SVG); // 40x20
        let png = raster_input(120.0, 60.0);

        assert_eq!(svg_scales(&svg, &png, None), (3.0, 1.0));
        assert_eq!(svg_scales(&png, &svg, None), (1.0, 3.0));
        // Explicit --svg-scale wins over the auto fit.
        assert_eq!(svg_scales(&svg, &png, Some(2.0)), (2.0, 2.0));
        // Two SVGs (or two rasters) render at their intrinsic size.
        assert_eq!(svg_scales(&svg, &svg_input(SQUARE_SVG), None), (1.0, 1.0));
        assert_eq!(svg_scales(&png, &raster_input(9.0, 9.0), None), (1.0, 1.0));
    }

    #[test]
    fn svg_rasterises_at_scale_over_the_background() {
        let svg = svg_input(SQUARE_SVG);

        let img = svg.to_rgba(1.0, TRANSPARENT).unwrap();
        assert_eq!(img.dimensions(), (40, 20));
        assert_eq!(*img.get_pixel(20, 10), Rgba([0, 0, 255, 255]));

        let scaled = svg.to_rgba(2.5, TRANSPARENT).unwrap();
        assert_eq!(scaled.dimensions(), (100, 50));

        // The rect covers the whole viewport, so the backdrop only shows outside it.
        let dot =
            svg_input(br#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"></svg>"#);
        assert_eq!(
            *dot.to_rgba(1.0, TRANSPARENT).unwrap().get_pixel(5, 5),
            TRANSPARENT
        );
        assert_eq!(
            *dot.to_rgba(1.0, Rgba([255, 255, 255, 255]))
                .unwrap()
                .get_pixel(5, 5),
            Rgba([255, 255, 255, 255])
        );
    }

    #[test]
    fn absurd_rasterisation_targets_are_refused() {
        let svg = svg_input(SQUARE_SVG);
        assert!(svg.to_rgba(100_000.0, TRANSPARENT).is_err());
    }
}
