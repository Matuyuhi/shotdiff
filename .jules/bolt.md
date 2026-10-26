## 2026-07-07 - Floating-point bottleneck in pixel loop
**Learning:** Floating-point arithmetic in tight pixel processing loops is a severe bottleneck.
**Action:** Use integer approximations for operations like luminance calculation (e.g., `(77 * r + 150 * g + 29 * b) >> 8` for luma) and manually unroll small loops (like pixel channel comparisons) to yield significant, measurable speedups.