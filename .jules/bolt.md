## 2024-07-03 - Luma calculation with float math

**Learning:** Calculating float operations inside tight pixel processing loops is very expensive. Float to int conversions take a big toll.
**Action:** Always favor fixed-point math and bitwise operations instead of floating-point arithmetic for graphics.
