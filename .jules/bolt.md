## 2024-05-24 - Optimizing pixel processing arithmetic
**Learning:** Floating-point arithmetic in tight pixel processing loops is a severe bottleneck in both Rust and the JS output. Using integer approximations (e.g., `(77 * r + 150 * g + 29 * b) >> 8` for luma) and manually unrolling small loops (like pixel channel comparisons) yields significant, measurable speedups without noticeable loss in accuracy.
**Action:** Use integer approximations for color conversions and unroll small constant-sized loops when processing image buffers.
