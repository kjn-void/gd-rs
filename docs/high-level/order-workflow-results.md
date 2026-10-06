# Order workflow: recorded measurements

Sources: [Rust application](../../benches/order_workflow/workload.rs), [C++ application](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp), [measurement runner](../../benches/order_workflow/compare.py). See the [analysis and limitations](order-workflow.md).

Times are median milliseconds across 3 process rounds, 5 samples each, after one warmup per process. Ranges show minimum–maximum samples; they are not confidence intervals. Ratios divide elapsed times. Peak RSS is the largest whole-process high-water mark across rounds, including setup.

gd-rs (SoA) uses checked nullable typed column slices and a reused SQLite row buffer. GD AoS uses `table_column_buffer`. GD AoSoA uses `simd::table_8_8`, the unmodified `gd_table_simd.cpp`, and the counted adapter. The native-index rows compare all three implementations; sorted changes only the Rust join index. See [adapter details and limits](order-workflow.md#gd-simd-variant).

## 10,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp).

| Stage | Workers | Index | gd-rs (SoA) ms (range) | GD (AoS) ms (range) | GD (AoSoA) ms (range) | AoS/gd-rs | AoSoA/gd-rs | AoSoA/AoS |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| import | 1 | native | 1.377 (1.312–1.484) | 2.177 (2.099–2.222) | 2.044 (1.979–2.154) | 1.58× | 1.48× | 0.94× |
| prepare | 1 | native | 1.177 (1.161–1.277) | 2.207 (2.023–2.411) | 1.684 (1.627–1.859) | 1.87× | 1.43× | 0.76× |
| prepare | 1 | sorted | 1.224 (1.173–1.303) | 2.246 (2.120–2.377) | 1.650 (1.580–1.716) | 1.83× | 1.35× | 0.73× |
| variants | 1 | native | 0.391 (0.359–0.441) | 1.727 (1.665–1.805) | 1.233 (1.177–1.314) | 4.42× | 3.16× | 0.71× |
| variants | 2 | native | 0.261 (0.245–0.303) | 1.154 (1.130–1.193) | 0.900 (0.849–1.580) | 4.42× | 3.45× | 0.78× |
| variants | 4 | native | 0.237 (0.212–0.299) | 0.833 (0.791–0.913) | 0.670 (0.622–0.752) | 3.52× | 2.83× | 0.80× |
| variants | 8 | native | 0.241 (0.205–0.347) | 0.875 (0.809–1.001) | 0.692 (0.632–0.804) | 3.63× | 2.87× | 0.79× |
| complete | 1 | native | 3.082 (2.937–3.362) | 6.242 (5.951–6.467) | 5.048 (4.879–5.152) | 2.03× | 1.64× | 0.81× |
| complete | 2 | native | 2.924 (2.752–3.047) | 5.634 (5.427–5.855) | 4.708 (4.508–4.877) | 1.93× | 1.61× | 0.84× |
| complete | 4 | native | 2.921 (2.763–3.421) | 5.347 (5.215–5.616) | 4.513 (4.347–4.726) | 1.83× | 1.55× | 0.84× |
| complete | 8 | native | 3.040 (2.849–3.601) | 5.720 (5.581–5.857) | 4.737 (4.192–5.094) | 1.88× | 1.56× | 0.83× |

| Stage | Workers | Index | gd-rs (SoA) peak MiB | GD (AoS) peak MiB | GD (AoSoA) peak MiB |
|---|---:|---|---:|---:|---:|
| import | 1 | native | 5.4 | 4.9 | 4.5 |
| prepare | 1 | native | 8.3 | 7.4 | 7.1 |
| prepare | 1 | sorted | 8.3 | 7.4 | 7.1 |
| variants | 1 | native | 10.3 | 8.8 | 8.7 |
| variants | 2 | native | 10.3 | 8.8 | 8.7 |
| variants | 4 | native | 10.5 | 9.1 | 9.0 |
| variants | 8 | native | 10.7 | 9.4 | 9.1 |
| complete | 1 | native | 10.4 | 8.9 | 8.7 |
| complete | 2 | native | 10.5 | 9.0 | 8.8 |
| complete | 4 | native | 10.6 | 9.2 | 9.2 |
| complete | 8 | native | 10.8 | 9.5 | 9.5 |

## 100,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp).

| Stage | Workers | Index | gd-rs (SoA) ms (range) | GD (AoS) ms (range) | GD (AoSoA) ms (range) | AoS/gd-rs | AoSoA/gd-rs | AoSoA/AoS |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| import | 1 | native | 13.346 (12.866–13.841) | 21.585 (20.972–22.543) | 20.180 (19.890–20.626) | 1.62× | 1.51× | 0.93× |
| prepare | 1 | native | 14.161 (13.310–16.930) | 26.528 (25.769–28.157) | 23.184 (21.854–24.734) | 1.87× | 1.64× | 0.87× |
| prepare | 1 | sorted | 14.292 (14.033–17.202) | 27.701 (26.474–29.868) | 23.805 (21.738–25.256) | 1.94× | 1.67× | 0.86× |
| variants | 1 | native | 4.475 (3.997–5.122) | 17.944 (17.597–18.407) | 14.101 (13.168–16.258) | 4.01× | 3.15× | 0.79× |
| variants | 2 | native | 2.441 (2.271–2.986) | 12.208 (11.842–13.521) | 8.858 (8.488–9.499) | 5.00× | 3.63× | 0.73× |
| variants | 4 | native | 1.712 (1.503–1.919) | 7.972 (7.821–8.149) | 6.266 (6.130–6.437) | 4.66× | 3.66× | 0.79× |
| variants | 8 | native | 1.483 (1.369–1.604) | 8.137 (7.545–9.284) | 6.620 (5.977–7.002) | 5.48× | 4.46× | 0.81× |
| complete | 1 | native | 30.716 (30.511–31.851) | 65.715 (63.192–70.337) | 55.320 (54.995–57.502) | 2.14× | 1.80× | 0.84× |
| complete | 2 | native | 29.018 (28.767–29.529) | 60.023 (58.727–61.832) | 50.738 (50.171–51.876) | 2.07× | 1.75× | 0.85× |
| complete | 4 | native | 28.596 (28.134–30.580) | 57.164 (56.072–58.611) | 49.487 (48.303–50.537) | 2.00× | 1.73× | 0.87× |
| complete | 8 | native | 30.827 (30.070–31.462) | 57.492 (56.686–60.867) | 49.890 (49.521–51.496) | 1.87× | 1.62× | 0.87× |

| Stage | Workers | Index | gd-rs (SoA) peak MiB | GD (AoS) peak MiB | GD (AoSoA) peak MiB |
|---|---:|---|---:|---:|---:|
| import | 1 | native | 21.3 | 19.0 | 15.8 |
| prepare | 1 | native | 52.2 | 40.5 | 39.6 |
| prepare | 1 | sorted | 52.2 | 40.6 | 39.6 |
| variants | 1 | native | 66.9 | 56.4 | 55.1 |
| variants | 2 | native | 67.5 | 60.2 | 60.0 |
| variants | 4 | native | 67.8 | 61.5 | 63.9 |
| variants | 8 | native | 67.7 | 61.9 | 60.7 |
| complete | 1 | native | 71.3 | 56.4 | 55.2 |
| complete | 2 | native | 71.5 | 60.2 | 56.0 |
| complete | 4 | native | 71.7 | 61.2 | 60.2 |
| complete | 8 | native | 71.6 | 61.0 | 59.7 |

## 1,000,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp).

| Stage | Workers | Index | gd-rs (SoA) ms (range) | GD (AoS) ms (range) | GD (AoSoA) ms (range) | AoS/gd-rs | AoSoA/gd-rs | AoSoA/AoS |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| import | 1 | native | 126.256 (124.438–129.676) | 212.133 (208.208–220.175) | 196.503 (194.957–198.229) | 1.68× | 1.56× | 0.93× |
| prepare | 1 | native | 177.687 (173.180–183.878) | 321.749 (309.938–327.416) | 278.294 (272.538–287.488) | 1.81× | 1.57× | 0.86× |
| prepare | 1 | sorted | 195.757 (191.298–212.272) | 323.720 (316.862–337.397) | 279.043 (272.758–304.648) | 1.65× | 1.43× | 0.86× |
| variants | 1 | native | 46.340 (45.422–48.576) | 185.078 (182.928–188.973) | 176.387 (173.920–183.850) | 3.99× | 3.81× | 0.95× |
| variants | 2 | native | 25.297 (24.569–25.629) | 118.312 (116.022–121.267) | 109.667 (108.050–113.209) | 4.68× | 4.34× | 0.93× |
| variants | 4 | native | 16.952 (16.058–17.722) | 82.923 (79.326–87.362) | 77.146 (74.081–80.367) | 4.89× | 4.55× | 0.93× |
| variants | 8 | native | 13.826 (13.401–14.754) | 77.171 (72.299–83.885) | 67.443 (63.373–70.339) | 5.58× | 4.88× | 0.87× |
| complete | 1 | native | 354.259 (347.885–372.229) | 721.873 (713.331–746.285) | 655.262 (643.181–665.693) | 2.04× | 1.85× | 0.91× |
| complete | 2 | native | 333.693 (328.834–353.642) | 652.265 (643.798–670.782) | 584.938 (577.696–616.576) | 1.95× | 1.75× | 0.90× |
| complete | 4 | native | 324.546 (317.894–336.786) | 618.415 (607.511–643.284) | 560.743 (549.386–601.750) | 1.91× | 1.73× | 0.91× |
| complete | 8 | native | 326.479 (318.483–332.035) | 619.105 (607.491–641.410) | 539.952 (532.148–556.923) | 1.90× | 1.65× | 0.87× |

| Stage | Workers | Index | gd-rs (SoA) peak MiB | GD (AoS) peak MiB | GD (AoSoA) peak MiB |
|---|---:|---|---:|---:|---:|
| import | 1 | native | 92.7 | 109.4 | 91.1 |
| prepare | 1 | native | 433.2 | 354.2 | 353.3 |
| prepare | 1 | sorted | 433.2 | 354.1 | 353.4 |
| variants | 1 | native | 590.9 | 520.5 | 518.2 |
| variants | 2 | native | 643.1 | 553.9 | 546.3 |
| variants | 4 | native | 647.4 | 554.1 | 552.4 |
| variants | 8 | native | 737.3 | 567.3 | 571.0 |
| complete | 1 | native | 591.9 | 521.6 | 518.2 |
| complete | 2 | native | 587.9 | 522.8 | 521.5 |
| complete | 4 | native | 592.1 | 530.1 | 524.2 |
| complete | 8 | native | 601.6 | 536.2 | 536.8 |

## Source and executable sizes

Sources: [Rust application](../../benches/order_workflow/workload.rs), [Rust selection APIs](../../src/table/selection.rs), [C++ application and adapters](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp), [size measurement code](../../benches/order_workflow/compare.py).

| Component | Physical lines | Nonblank lines | Bytes |
|---|---:|---:|---:|
| rust_application | 213 | 203 | 6,987 |
| rust_selection_library | 175 | 167 | 7,045 |
| rust_column_views_library | 469 | 423 | 17,406 |
| rust_sqlite_library | 800 | 751 | 31,519 |
| rust_driver | 220 | 214 | 7,675 |
| cpp_application_and_adapters | 165 | 164 | 8,017 |
| cpp_simd_adapter | 106 | 105 | 5,443 |
| cpp_driver_and_pool | 215 | 212 | 11,097 |
| shared_fixture_and_runner | 370 | 342 | 20,172 |
| rust_api_tests | 128 | 124 | 4,603 |

Lines include comments; formatting differs between languages. Rust selection, column-view, and SQLite modules are counted separately; their full reusable APIs exceed what this application calls. Other library and dependency source is excluded.

| Standalone program | Unstripped bytes | Stripped bytes |
|---|---:|---:|
| rust | 2,649,632 | 2,384,304 |
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

Environment details follow. The [raw measurement JSON](measurements/order-workflow-m3max.json) additionally records every source hash, command, runtime library, rejection diagnostic, and process-load snapshot.

```text
utc: 2026-10-06T16:03:49.425438+00:00
platform: macOS-27.0.1-arm64-arm-64bit-Mach-O
cpu: Apple M3 Max
cpu_topology: hw.physicalcpu: 16
hw.logicalcpu: 16
hw.perflevel0.physicalcpu: 12
hw.perflevel1.physicalcpu: 4
logical_cpus: 16
rustc: rustc 1.98.1 (48a229cea 2026-09-01)
binary: rustc
commit-hash: 48a229ceaefd4985c50990b14116b6d856af0985
commit-date: 2026-09-01
host: aarch64-apple-darwin
release: 1.98.1
LLVM version: 22.1.8
cxx: Apple clang version 21.0.0 (clang-2100.3.34.2)
Target: arm64-apple-darwin27.0.0
Thread model: posix
InstalledDir: /Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin
gd_revision: cb11cff90d05260a88d59a30c9da421cd4e19c34
gd_rs_revision: c07f905a6cbd0faed538b697a4c2b51e40743bdd
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
fixture_sqlite: 3.53.4
invocation: ['target/order-workflow/rerun-guarded.py', '--skip-build', '--samples', '5', '--rounds', '3', '--output', 'target/order-workflow/rerun-layout-results.json']
gd_source_unchanged: True
power_settings: Battery Power:
 Sleep On Power Button 1
 powermode            0
 standby              1
 ttyskeepawake        1
 hibernatemode        3
 powernap             1
 hibernatefile        /var/vm/sleepimage
 displaysleep         2
 womp                 0
 networkoversleep     0
 sleep                1
 lessbright           1
 tcpkeepalive         1
 disksleep            10
AC Power:
 Sleep On Power Button 1
 powermode            0
 standby              1
 ttyskeepawake        1
 hibernatemode        3
 powernap             1
 hibernatefile        /var/vm/sleepimage
 displaysleep         10
 womp                 1
 networkoversleep     0
 sleep                0
 tcpkeepalive         1
 disksleep            10
thermal_state_before: Note: No thermal warning level has been recorded
Note: No performance warning level has been recorded
Note: No CPU power status has been recorded
thermal_state_after: Note: No thermal warning level has been recorded
Note: No performance warning level has been recorded
Note: No CPU power status has been recorded
```
