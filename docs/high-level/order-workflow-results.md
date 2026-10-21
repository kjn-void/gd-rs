# Order workflow: recorded measurements

Sources: [Rust application](../../benches/order_workflow/workload.rs), [C++ application](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp), [measurement runner](../../benches/order_workflow/compare.py). See the [analysis and limitations](order-workflow.md).

Times are median milliseconds across 3 process rounds, 5 samples each, after one warmup per process. Ranges show minimum–maximum samples; they are not confidence intervals. Ratios divide elapsed times. Peak RSS is the largest whole-process high-water mark across rounds, including setup.

gd-rs (SoA) uses checked nullable typed column slices and a reused SQLite row buffer. GD AoS uses `table_column_buffer`. GD AoSoA uses `simd::table_8_8`, the unmodified `gd_table_simd.cpp`, and the counted adapter. The native-index rows compare all three implementations; sorted changes only the Rust join index. See [adapter details and limits](order-workflow.md#gd-simd-variant).

## 10,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp).

| Stage | Workers | Index | gd-rs (SoA) ms (range) | GD (AoS) ms (range) | GD (AoSoA) ms (range) | AoS/gd-rs | AoSoA/gd-rs | AoSoA/AoS |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| import | 1 | native | 0.802 (0.787–0.836) | 1.306 (1.290–1.331) | 1.253 (1.237–1.298) | 1.63× | 1.56× | 0.96× |
| prepare | 1 | native | 0.750 (0.746–0.769) | 1.105 (1.090–1.151) | 1.020 (0.998–1.068) | 1.47× | 1.36× | 0.92× |
| prepare | 1 | sorted | 0.750 (0.742–0.766) | 1.103 (1.093–1.144) | 1.025 (1.001–1.067) | 1.47× | 1.37× | 0.93× |
| variants | 1 | native | 0.227 (0.205–0.277) | 0.947 (0.899–0.973) | 0.711 (0.693–0.758) | 4.18× | 3.14× | 0.75× |
| variants | 2 | native | 0.160 (0.142–0.183) | 0.630 (0.617–0.691) | 0.523 (0.513–0.563) | 3.93× | 3.27× | 0.83× |
| variants | 4 | native | 0.127 (0.102–0.186) | 0.489 (0.478–0.511) | 0.420 (0.404–0.441) | 3.86× | 3.31× | 0.86× |
| variants | 8 | native | 0.139 (0.088–0.164) | 0.465 (0.426–0.488) | 0.399 (0.361–0.448) | 3.34× | 2.87× | 0.86× |
| complete | 1 | native | 1.803 (1.767–1.875) | 3.394 (3.288–3.481) | 3.039 (2.979–3.132) | 1.88× | 1.69× | 0.90× |
| complete | 2 | native | 1.742 (1.696–1.785) | 3.099 (3.043–3.206) | 2.831 (2.794–2.917) | 1.78× | 1.63× | 0.91× |
| complete | 4 | native | 1.694 (1.667–1.720) | 2.931 (2.875–3.002) | 2.722 (2.693–2.790) | 1.73× | 1.61× | 0.93× |
| complete | 8 | native | 1.734 (1.697–1.780) | 2.914 (2.856–2.983) | 2.689 (2.652–2.770) | 1.68× | 1.55× | 0.92× |

| Stage | Workers | Index | gd-rs (SoA) peak MiB | GD (AoS) peak MiB | GD (AoSoA) peak MiB |
|---|---:|---|---:|---:|---:|
| import | 1 | native | 4.8 | 5.0 | 4.6 |
| prepare | 1 | native | 6.9 | 7.5 | 7.1 |
| prepare | 1 | sorted | 6.9 | 7.5 | 7.1 |
| variants | 1 | native | 8.3 | 9.0 | 8.8 |
| variants | 2 | native | 8.3 | 9.0 | 8.9 |
| variants | 4 | native | 8.4 | 9.1 | 9.0 |
| variants | 8 | native | 8.7 | 9.2 | 9.5 |
| complete | 1 | native | 8.4 | 8.9 | 8.9 |
| complete | 2 | native | 8.4 | 9.1 | 9.0 |
| complete | 4 | native | 8.5 | 9.3 | 9.2 |
| complete | 8 | native | 8.7 | 9.5 | 9.2 |

## 100,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp).

| Stage | Workers | Index | gd-rs (SoA) ms (range) | GD (AoS) ms (range) | GD (AoSoA) ms (range) | AoS/gd-rs | AoSoA/gd-rs | AoSoA/AoS |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| import | 1 | native | 8.266 (8.027–8.364) | 13.207 (12.915–13.569) | 12.771 (12.687–12.958) | 1.60× | 1.55× | 0.97× |
| prepare | 1 | native | 8.730 (8.666–8.958) | 13.171 (12.930–13.306) | 12.591 (12.395–12.795) | 1.51× | 1.44× | 0.96× |
| prepare | 1 | sorted | 9.179 (9.081–9.372) | 13.148 (12.939–13.392) | 12.664 (12.358–12.988) | 1.43× | 1.38× | 0.96× |
| variants | 1 | native | 2.515 (2.481–2.569) | 9.717 (9.564–10.012) | 7.595 (7.351–7.875) | 3.86× | 3.02× | 0.78× |
| variants | 2 | native | 1.395 (1.359–1.478) | 6.184 (6.043–6.286) | 5.162 (4.991–5.356) | 4.43× | 3.70× | 0.83× |
| variants | 4 | native | 0.907 (0.801–0.962) | 4.233 (4.182–4.526) | 3.642 (3.431–3.851) | 4.67× | 4.01× | 0.86× |
| variants | 8 | native | 0.644 (0.515–0.749) | 3.901 (3.813–4.034) | 3.414 (3.154–3.641) | 6.06× | 5.30× | 0.88× |
| complete | 1 | native | 19.615 (19.554–19.781) | 36.434 (36.033–36.715) | 33.566 (33.127–33.903) | 1.86× | 1.71× | 0.92× |
| complete | 2 | native | 18.486 (18.353–18.786) | 32.872 (32.482–33.465) | 30.982 (30.819–31.156) | 1.78× | 1.68× | 0.94× |
| complete | 4 | native | 18.028 (17.810–18.122) | 30.779 (30.373–31.200) | 29.306 (29.105–29.595) | 1.71× | 1.63× | 0.95× |
| complete | 8 | native | 17.778 (17.708–17.946) | 30.509 (30.232–31.398) | 29.052 (28.822–29.630) | 1.72× | 1.63× | 0.95× |

| Stage | Workers | Index | gd-rs (SoA) peak MiB | GD (AoS) peak MiB | GD (AoSoA) peak MiB |
|---|---:|---|---:|---:|---:|
| import | 1 | native | 13.3 | 21.6 | 16.9 |
| prepare | 1 | native | 38.0 | 46.5 | 42.2 |
| prepare | 1 | sorted | 38.0 | 46.4 | 42.3 |
| variants | 1 | native | 45.8 | 59.4 | 57.8 |
| variants | 2 | native | 47.4 | 59.7 | 58.4 |
| variants | 4 | native | 47.6 | 61.2 | 60.4 |
| variants | 8 | native | 48.1 | 66.4 | 61.2 |
| complete | 1 | native | 46.0 | 59.2 | 57.8 |
| complete | 2 | native | 47.6 | 59.7 | 58.5 |
| complete | 4 | native | 47.8 | 60.3 | 60.5 |
| complete | 8 | native | 47.9 | 61.9 | 61.1 |

## 1,000,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp).

| Stage | Workers | Index | gd-rs (SoA) ms (range) | GD (AoS) ms (range) | GD (AoSoA) ms (range) | AoS/gd-rs | AoSoA/gd-rs | AoSoA/AoS |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| import | 1 | native | 80.171 (79.782–81.023) | 128.471 (128.125–129.064) | 124.588 (123.190–125.343) | 1.60× | 1.55× | 0.97× |
| prepare | 1 | native | 97.805 (96.394–98.699) | 161.273 (154.248–172.083) | 159.045 (154.603–170.510) | 1.65× | 1.63× | 0.99× |
| prepare | 1 | sorted | 106.699 (106.074–110.152) | 163.374 (155.668–176.977) | 158.458 (155.428–171.779) | 1.53× | 1.49× | 0.97× |
| variants | 1 | native | 26.972 (26.833–27.208) | 105.571 (98.318–114.845) | 96.235 (92.003–105.979) | 3.91× | 3.57× | 0.91× |
| variants | 2 | native | 15.042 (14.678–15.227) | 64.972 (61.465–69.264) | 62.309 (61.099–68.617) | 4.32× | 4.14× | 0.96× |
| variants | 4 | native | 10.172 (9.641–10.504) | 44.703 (42.523–51.015) | 45.644 (41.432–49.870) | 4.39× | 4.49× | 1.02× |
| variants | 8 | native | 7.432 (7.127–8.026) | 41.925 (38.239–46.452) | 35.069 (33.907–41.870) | 5.64× | 4.72× | 0.84× |
| complete | 1 | native | 207.085 (204.831–210.286) | 395.378 (388.176–417.191) | 382.667 (374.514–406.920) | 1.91× | 1.85× | 0.97× |
| complete | 2 | native | 195.747 (193.613–202.598) | 360.115 (353.543–385.032) | 344.520 (340.203–366.500) | 1.84× | 1.76× | 0.96× |
| complete | 4 | native | 190.379 (188.689–192.908) | 344.197 (331.391–364.109) | 329.249 (324.242–349.841) | 1.81× | 1.73× | 0.96× |
| complete | 8 | native | 188.329 (185.916–190.316) | 332.734 (329.573–353.720) | 321.724 (317.422–342.066) | 1.77× | 1.71× | 0.97× |

| Stage | Workers | Index | gd-rs (SoA) peak MiB | GD (AoS) peak MiB | GD (AoSoA) peak MiB |
|---|---:|---|---:|---:|---:|
| import | 1 | native | 151.5 | 166.3 | 125.5 |
| prepare | 1 | native | 296.6 | 403.4 | 372.8 |
| prepare | 1 | sorted | 287.9 | 403.5 | 372.8 |
| variants | 1 | native | 411.6 | 567.4 | 546.2 |
| variants | 2 | native | 411.8 | 569.4 | 580.8 |
| variants | 4 | native | 411.6 | 596.7 | 657.7 |
| variants | 8 | native | 414.3 | 589.6 | 568.5 |
| complete | 1 | native | 469.4 | 567.5 | 550.5 |
| complete | 2 | native | 469.4 | 568.9 | 554.1 |
| complete | 4 | native | 478.0 | 576.5 | 559.0 |
| complete | 8 | native | 479.1 | 588.7 | 573.9 |

## Source and executable sizes

Sources: [Rust application](../../benches/order_workflow/workload.rs), [Rust selection APIs](../../src/table/selection.rs), [C++ application and adapters](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp), [size measurement code](../../benches/order_workflow/compare.py).

| Component | Physical lines | Nonblank lines | Bytes |
|---|---:|---:|---:|
| rust_application | 284 | 247 | 10,448 |
| rust_selection_library | 175 | 167 | 7,045 |
| rust_column_views_library | 903 | 814 | 32,020 |
| rust_sqlite_library | 805 | 756 | 31,802 |
| rust_driver | 286 | 237 | 9,412 |
| cpp_application_and_adapters | 270 | 237 | 11,304 |
| cpp_simd_adapter | 157 | 136 | 6,159 |
| cpp_driver_and_pool | 385 | 351 | 15,059 |
| shared_fixture_and_runner | 370 | 342 | 20,172 |
| rust_api_tests | 128 | 124 | 4,625 |

Lines include comments; formatting differs between languages. Rust selection, column-view, and SQLite modules are counted separately; their full reusable APIs exceed what this application calls. Other library and dependency source is excluded.

| Standalone program | Unstripped bytes | Stripped bytes |
|---|---:|---:|
| rust | 2,674,816 | 2,401,872 |
| cpp | 1,401,952 | 1,325,104 |
| cpp_simd | 1,402,328 | 1,324,960 |

Executables include the application, timing/correctness driver, retained library code, and SQLite. Rust also uses Rayon and serde_json; C++ uses the counted pool and a small JSON emitter. Both use system dynamic libraries, listed in the raw JSON. These are program footprints, not intrinsic table-library sizes.

## Verification and environment

Timing invocations check background CPU usage before and during warmup and samples. The controller waits while an unrelated process exceeds 60% of one CPU and repeats any invocation with observed activity. Excluded samples, observations, and the controller source are retained in the raw JSON; only accepted timings enter the tables. Source fingerprints identify the measured current worktree; the recorded Git revision is its base revision.

Sources: [fixture and SQL oracle](../../benches/order_workflow/fixture.py), [Rust verifier](../../benches/order_workflow/driver.rs), [C++ verifier](../../benches/cpp-reference/order_workflow/driver.cpp).

52 verification invocations completed: the hand fixture and each measured size, every implementation at every worker count, plus the Rust sorted-index diagnostic. Every cell was compared with independent SQL; all output counts and digests matched across languages, index algorithms, and worker counts.

- 11 input lines → variant counts `[5, 4, 0, 0, 0, 0, 1, 3]`.
- 10,000 input lines → variant counts `[7000, 1695, 412, 255, 939, 1001, 5365, 167]`.
- 100,000 input lines → variant counts `[70174, 17099, 4093, 2614, 8689, 11217, 54149, 1641]`.
- 1,000,000 input lines → variant counts `[701831, 171703, 41146, 26177, 86358, 113317, 541638, 16323]`.

Environment details follow. The [raw measurement JSON](measurements/order-workflow-m6.json) additionally records every source hash, command, runtime library, rejection diagnostic, and process-load snapshot.

```text
utc: 2026-10-21T06:24:04.777596+00:00
platform: macOS-27.0.1-arm64-arm-64bit
cpu: Apple M6
cpu_topology: hw.physicalcpu: 12
hw.logicalcpu: 12
hw.perflevel0.physicalcpu: 2
hw.perflevel1.physicalcpu: 4
logical_cpus: 12
rustc: rustc 1.99.1 (58cf8f9c3 2026-10-13)
binary: rustc
commit-hash: 58cf8f9c35b355199727627d07ae34a4e2846d67
commit-date: 2026-10-13
host: aarch64-apple-darwin
release: 1.99.1
LLVM version: 23.1.1
cxx: Apple clang version 21.0.0 (clang-2100.3.34.2)
Target: arm64-apple-darwin27.0.0
Thread model: posix
InstalledDir: /Library/Developer/CommandLineTools/usr/bin
gd_revision: 1bba194909d997891c05fe5a4ce1d3d3a888e0cf
gd_rs_revision: 28a509b24ddca56b29410350f110bcfa2a4255f3
rust_flags: release: opt-level=3, codegen-units=1, lto=thin; target-cpu=native; features sqlite,rayon; CFLAGS=-march=native
cpp_flags: Release: -O3 -DNDEBUG -march=native; CMAKE_INTERPROCEDURAL_OPTIMIZATION=ON; sanitizers OFF
affinity: OS scheduling, no affinity; persistent pools; one process at a time
seed: 7640891576956012809
samples: 5
rounds: 3
implementations: {'rust': 'gd::Table', 'cpp': 'gd::table::table_column_buffer', 'cpp_simd': 'gd::table::simd::table_8_8 with benchmark adapter'}
rust_column_access: generic checked nullable fixed-width slices for preparation, filtering, and amount mutation; one reused SQLite row buffer
process_order: rotate starting implementation each round; reverse every three rounds
simd_adapter: unmodified gd_table_simd.cpp; syntax-placeholder-only generated header; packed null-bitmap column; packed cell/reference getters; owned pointer/schema; geometric import reservation; application projected gather and packed filtering
sqlite_version: 3.53.2
fixture_sqlite: 3.54.0
invocation: ['target/order-workflow/rerun-guarded.py', '--samples', '5', '--rounds', '3', '--output', 'target/order-workflow/results-m6.json']
gd_source_unchanged: True
power_settings: AC Power:
 Sleep On Power Button 1
 autorestartatconnect 0
 lowpowermode         0
 standby              0
 ttyskeepawake        1
 powernap             1
 displaysleep         10
 womp                 1
 networkoversleep     0
 sleep                1
 tcpkeepalive         1
 autorestart          0
 disksleep            10
thermal_state_before: Note: No thermal warning level has been recorded
Note: No performance warning level has been recorded
Note: No CPU power status has been recorded
thermal_state_after: Note: No thermal warning level has been recorded
Note: No performance warning level has been recorded
Note: No CPU power status has been recorded
```
