## 2024-07-08 - Integer Luma & Unrolling
**Learning:** Floating-point arithmetic and loop branches in tight pixel processing loops (both Rust and Javascript versions) are severe bottlenecks.
**Action:** Use integer approximations like `(77 * r + 150 * g + 29 * b) >> 8` for luma, and manually unroll small loops (e.g. RGBA channel comparisons) for measurable speedups.
