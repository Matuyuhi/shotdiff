## 2024-07-13 - Optimize pixel loops with integer math and unrolling
**Learning:** Floating-point arithmetic and short loop overhead in tight pixel processing loops (like `build_diff`'s per-pixel delta and luma calculations) are a severe bottleneck in this Rust/WASM architecture.
**Action:** Use integer approximations for luma (e.g., `(77 * r + 150 * g + 29 * b) >> 8`) and manually unroll small loops (e.g., `.max()` chaining for RGBA channels) in tight performance paths.
