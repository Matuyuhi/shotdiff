## 2024-07-08 - Floating-Point Bottlenecks in Tight Pixel Loops
**Learning:** Floating-point arithmetic and conversions (e.g., `f32` to `u32`) in tight pixel processing loops create a severe, measurable performance bottleneck in rust image processing code.
**Action:** Always prefer using integer approximations (e.g., `(77 * r + 150 * g + 29 * b) >> 8` for luma calculations) and manually unroll small loop comparisons when operating inside high-frequency, per-pixel iterators.
