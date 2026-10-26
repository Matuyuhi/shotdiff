## 2024-07-06 - Floating Point Arithmetic Bottleneck
**Learning:** Floating-point arithmetic in tight pixel processing loops is a severe bottleneck in shotdiff. Using integer approximations (e.g., `(77 * r + 150 * g + 29 * b) >> 8` for luma) and manually unrolling small loops (like pixel channel comparisons) yields significant, measurable speedups.
**Action:** Avoid floating-point math in per-pixel processing paths. Favor integer approximations and unrolled loops for small, fixed-size iterations like RGBA channels.
