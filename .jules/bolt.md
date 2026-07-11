## 2024-07-11 - Optimizing pixel processing bottlenecks
**Learning:** Floating-point arithmetic in tight pixel processing loops is a severe bottleneck in 'shotdiff' (and likely similar apps).
**Action:** Use integer approximations (e.g., `(77 * r + 150 * g + 29 * b) >> 8` for luma) and manually unroll small loops (like pixel channel comparisons) for significant, measurable speedups.
