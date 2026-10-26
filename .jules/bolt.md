## 2025-01-20 - Unroll short loops in hot paths
**Learning:** In a very hot path processing per-pixel elements across a tight, fixed boundary (like 4 RGBA channels), loop control overhead and data dependencies on a single accumulator can restrict performance. Manually unrolling small loops allows for better instruction pipelining and reduced branching.
**Action:** Unroll fixed-length, short iterations (like processing RGBA arrays) in performance-critical sections to eliminate the `for` loop overhead and improve throughput.
