## 2024-05-24 - [Avoid f32 arithmetic in tight image processing loops]
**Learning:** [Floating point calculations for simple color conversions in Rust (like luma calculation) have a measurable performance penalty in hot paths over images with millions of pixels. Replacing `(0.299*r + ...)` with `(299*r + ...) / 1000` is safer and faster.]
**Action:** [Next time working with pixel-by-pixel image transformations, prefer integer arithmetic scaled up and then down instead of floating point math.]
