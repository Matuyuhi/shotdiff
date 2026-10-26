## 2024-06-30 - [Luma Calculation Float to Int]
**Learning:** In hot loops processing millions of pixels, calculating Rec. 601 greyscale values using `f32` conversion and multiplication creates measurable overhead (e.g., in `build_diff`).
**Action:** Use integer approximations (`(77 * r + 150 * g + 29 * bl) >> 8`) to avoid expensive floating-point math, and use the knowledge that this won't overflow a `u8` to remove unnecessary bounds checking (`.min(255)`).
