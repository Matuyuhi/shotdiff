## 2024-07-15 - Optimize pixel diff and luma calculations
**Learning:** Floating-point arithmetic in tight pixel processing loops is a severe bottleneck in this Rust codebase. Using integer approximations for luma and avoiding loop overhead per pixel yields measurable speedups without sacrificing readable correctness.
**Action:** Replace floating-point operations in hot pixel loops with integer approximations (e.g., `(77 * r + 150 * g + 29 * b) >> 8`) and unroll small loops (e.g., `max` over RGBA channels) where feasible.
