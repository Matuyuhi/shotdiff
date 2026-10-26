## 2026-07-12 - Floating-point bottlenecks in tight loops
**Learning:** Floating-point arithmetic in tight pixel processing loops is a severe bottleneck. Using integer approximations (e.g., `(77 * r + 150 * g + 29 * b) >> 8` for luma) and manually unrolling small loops (like pixel channel comparisons) yields significant, measurable speedups.
**Action:** When processing images or doing operations per-pixel, replace floating point math with integer arithmetic and bit shifts. Unroll small loops manually.
