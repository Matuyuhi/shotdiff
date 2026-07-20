## 2026-06-23 - [Optimizing image processing loops]
**Learning:** The `image` crate's `.pixels()` iterator has significant overhead and floating-point math is a bottleneck in tight loops.
**Action:** Use chunked iteration over raw byte slices (`.as_raw().chunks_exact(4)`) and integer approximations for color math (like luma) in pixel processing loops.
