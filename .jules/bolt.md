## 2024-06-25 - Luma math optimization
**Learning:** Hot-path floating point math (`0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32`) in pixel loops inside `image` processing applications incurs a significant penalty due to casting and f32 arithmetic.
**Action:** Replace floating point combinations with pre-scaled integer coefficients and bitshifts (e.g., `(77*r + 150*g + 29*b) >> 9`) to avoid the overhead of floats when determining integer contexts like luma.
