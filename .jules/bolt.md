## 2024-05-24 - Optimizing tight pixel loops in image processing
**Learning:** Floating-point arithmetic in tight pixel processing loops is a severe bottleneck in this codebase.
**Action:** Use integer approximations (e.g., `(77 * r + 150 * g + 29 * b) >> 8` for luma) and manually unroll small loops (like pixel channel comparisons) to yield significant, measurable speedups when writing pixel manipulation code.
