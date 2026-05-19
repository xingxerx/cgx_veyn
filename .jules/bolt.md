
## 2024-05-19 - Removed full sort from hot-path Z-score calculation
**Learning:** The system was running `compute_stats` across all events for computing Z-scores, and `compute_stats` was using a full array sort `sorted.sort_by(...)` to calculate 10th and 90th percentiles that were *never used* by the Z-score path. This resulted in O(n log n) operations where O(n) would suffice.
**Action:** Always separate summary statistics needed for periodic display (like percentiles) from simple descriptive statistics needed for per-event hot-path continuous evaluation (like mean/stddev).
