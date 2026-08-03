# M12 performance status

The provisional 512×512 targets remain the spec's non-blocking research SLOs:
Quality p95 ≤10 s, Fast p95 ≤1 s, peak working set ≤1 GiB on a named reference
CPU.

No honest isolated named-CPU court exists after the M7 operator waiver, so M12
does not relabel cached or parallel telemetry as that proof. The latest
committed M8 calibration records runtime p95 3771 ms, but it is not a substitute
for the missing isolated two-preset court.

What is release-enforced now:

- core elapsed, render, hypothesis, candidate and estimated-memory budgets;
- decode allocation limits;
- M10 spatial indexing plus explicit intersection/sample work limits;
- M11 8-million-pixel working-set limit, bounded center sampling, unique
  candidate inventory and total proposal-work limit;
- WASM encoded-input limit;
- legacy timeout and combined output/log limit.

Therefore runtime growth is bounded and diagnosable, while the 1 s/10 s p95
numbers remain unclaimed. Reclassifying them as release-blocking requires a
separate named-CPU measurement artifact, not a code or documentation edit.
