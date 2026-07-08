## 2024-07-08 - Avoid floating-point math and unroll hot pixel processing loops
**Learning:** Floating-point arithmetic in tight pixel processing loops is a severe bottleneck, slowing down diff processing and adding overhead for f32 conversions.
**Action:** Using integer approximations for color equations (e.g., `(77 * r + 150 * g + 29 * b) >> 8` for luma calculations) and manually unrolling small fixed-length loops (like RGB pixel channel max differences) yields significant, measurable speedups by avoiding f32 conversion penalties on the hot path.
