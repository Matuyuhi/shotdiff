//! shotdiff — side-by-side screenshot diff.
//!
//! Takes two image paths (BEFORE, AFTER) and produces a `BEFORE | DIFF | AFTER`
//! comparison where changed pixels are highlighted in pink. Two output modes:
//!   * PNG mode (default): writes a composite PNG. No browser/GUI — works in CI.
//!   * Browser mode (`--browser`): writes a self-contained HTML viewer with an
//!     interactive threshold slider and opens it in the default browser.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use image::{Rgba, RgbaImage};

/// Pink used to paint changed pixels (#FF2D87).
const PINK: Rgba<u8> = Rgba([0xFF, 0x2D, 0x87, 0xFF]);
/// Gap (px) between panels in the composite, and its colour.
const GAP: u32 = 16;
const GAP_BG: Rgba<u8> = Rgba([0x1a, 0x1a, 0x1a, 0xFF]);
/// Fill colour for padded regions when the two inputs differ in size.
const PAD_BG: Rgba<u8> = Rgba([0x00, 0x00, 0x00, 0x00]);

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
}

const USAGE: &str = "\
shotdiff — side-by-side screenshot diff (BEFORE | DIFF | AFTER, changes in pink)

USAGE:
    shotdiff <BEFORE> <AFTER> [OPTIONS]

ARGS:
    <BEFORE>    Path to the first/old image
    <AFTER>     Path to the second/new image

OPTIONS:
    -o, --output <PATH>   Output PNG path (default: shotdiff-out.png)
    -t, --threshold <N>   Per-channel diff threshold 0-255 (default: 16)
        --diff-only       Output only the centre DIFF panel
        --browser         Build an interactive HTML viewer and open it
        --fail-on-diff    Exit 1 if any pixel changed (visual-regression gate)
        --max-diff <V>    Fail only if change exceeds V; e.g. 1200 or 0.5%
    -h, --help            Show this help

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

/// Per-pixel max absolute channel difference across RGBA.
#[inline]
fn pixel_delta(a: &[u8; 4], b: &[u8; 4]) -> u8 {
    let d0 = a[0].abs_diff(b[0]);
    let d1 = a[1].abs_diff(b[1]);
    let d2 = a[2].abs_diff(b[2]);
    let d3 = a[3].abs_diff(b[3]);
    d0.max(d1).max(d2).max(d3)
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
            // Use integer arithmetic with bit shifts instead of floats for ~10% speedup.
            // 77/256 ≈ 0.300, 150/256 ≈ 0.586, 29/256 ≈ 0.113
            let luma = (r as u32 * 77 + g as u32 * 150 + bl as u32 * 29) >> 8;
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
    let img_a = image::open(&args.before)
        .map_err(|e| format!("failed to read {}: {e}", args.before.display()))?
        .to_rgba8();
    let img_b = image::open(&args.after)
        .map_err(|e| format!("failed to read {}: {e}", args.after.display()))?
        .to_rgba8();

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

fn data_uri(path: &Path) -> Result<String, String> {
    let bytes =
        std::fs::read(path).map_err(|e| format!("failed to read {}: {e}", path.display()))?;
    Ok(format!(
        "data:{};base64,{}",
        sniff_mime(&bytes),
        b64(&bytes)
    ))
}

fn run_browser(args: &Args) -> Result<(), String> {
    let uri_a = data_uri(&args.before)?;
    let uri_b = data_uri(&args.after)?;
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
}
