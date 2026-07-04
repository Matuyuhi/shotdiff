## 2024-07-04 - Floating Point in Pixel Loops
**Learning:** Floating-point arithmetic in tight pixel processing loops (like luma calculation) is a severe bottleneck in this architecture.
**Action:** Use integer approximations like `(77 * r + 150 * g + 29 * b) >> 8` for luma and manually unroll small loops (like `pixel_delta` comparisons) for measurable speedups.
