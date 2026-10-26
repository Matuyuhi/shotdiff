## 2024-07-01 - Pixel Iteration Optimization
**Learning:** Unrolling loop for pixel channels (`a[0].abs_diff(b[0]).max(...)`) instead of an iterator/loop on small fixed arrays significantly speeds up tight inner loops in image processing.
**Action:** Unroll fixed 4-channel loops inside hot pixel processing routines.
