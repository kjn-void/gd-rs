# Order workflow: recorded measurements

Sources: [Rust application](../../benches/order_workflow/workload.rs), [C++ application](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp), [measurement runner](../../benches/order_workflow/compare.py). See the [analysis and limitations](order-workflow.md).

Times are median milliseconds across 3 process rounds, 5 samples each, after one warmup per process. Ranges show minimum–maximum samples; they are not confidence intervals. Ratios divide elapsed times. Peak RSS is the largest whole-process high-water mark across rounds, including setup.

Rust uses checked nullable typed column slices and a reused SQLite row buffer. GD DTO uses `table_column_buffer`. GD SIMD uses `simd::table_8_8`, the unmodified `gd_table_simd.cpp`, and the counted adapter. The native-index rows compare all three implementations; sorted changes only the Rust join index. See [adapter details and limits](order-workflow.md#gd-simd-variant).

## 10,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp).

| Stage | Workers | Index | Rust ms (range) | GD DTO ms (range) | GD SIMD ms (range) | DTO/Rust | SIMD/Rust | SIMD/DTO |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| import | 1 | native | 1.375 (1.319–1.469) | 2.333 (2.183–2.711) | 2.061 (1.937–2.392) | 1.70× | 1.50× | 0.88× |
| prepare | 1 | native | 1.187 (1.133–1.299) | 2.328 (1.930–2.548) | 1.738 (1.581–2.135) | 1.96× | 1.46× | 0.75× |
| prepare | 1 | sorted | 1.181 (1.134–1.381) | 2.334 (1.921–2.681) | 1.638 (1.581–1.902) | 1.98× | 1.39× | 0.70× |
| variants | 1 | native | 0.413 (0.379–0.469) | 1.779 (1.711–2.168) | 1.211 (1.117–1.411) | 4.31× | 2.93× | 0.68× |
| variants | 2 | native | 0.261 (0.234–0.309) | 1.291 (1.192–1.532) | 0.884 (0.821–1.143) | 4.94× | 3.38× | 0.68× |
| variants | 4 | native | 0.207 (0.157–0.255) | 0.886 (0.818–0.969) | 0.660 (0.614–0.818) | 4.28× | 3.19× | 0.74× |
| variants | 8 | native | 0.227 (0.189–0.305) | 0.884 (0.841–0.992) | 0.695 (0.643–0.832) | 3.90× | 3.07× | 0.79× |
| complete | 1 | native | 2.893 (2.817–3.625) | 6.044 (5.859–6.754) | 4.928 (4.731–5.615) | 2.09× | 1.70× | 0.82× |
| complete | 2 | native | 2.721 (2.642–2.953) | 5.684 (5.314–6.137) | 4.401 (4.325–4.544) | 2.09× | 1.62× | 0.77× |
| complete | 4 | native | 2.865 (2.634–3.480) | 5.443 (5.156–5.971) | 4.406 (4.195–5.142) | 1.90× | 1.54× | 0.81× |
| complete | 8 | native | 2.930 (2.763–3.245) | 5.564 (5.229–5.895) | 4.647 (4.440–5.209) | 1.90× | 1.59× | 0.84× |

| Stage | Workers | Index | Rust peak MiB | GD DTO peak MiB | GD SIMD peak MiB |
|---|---:|---|---:|---:|---:|
| import | 1 | native | 5.4 | 4.9 | 4.5 |
| prepare | 1 | native | 8.3 | 8.2 | 7.1 |
| prepare | 1 | sorted | 8.3 | 8.2 | 7.1 |
| variants | 1 | native | 10.3 | 9.8 | 8.7 |
| variants | 2 | native | 10.3 | 10.0 | 8.8 |
| variants | 4 | native | 10.5 | 10.2 | 9.1 |
| variants | 8 | native | 10.7 | 10.9 | 9.3 |
| complete | 1 | native | 10.4 | 9.9 | 8.8 |
| complete | 2 | native | 10.5 | 10.3 | 8.9 |
| complete | 4 | native | 10.6 | 10.3 | 9.1 |
| complete | 8 | native | 10.9 | 10.9 | 9.4 |

## 100,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp).

| Stage | Workers | Index | Rust ms (range) | GD DTO ms (range) | GD SIMD ms (range) | DTO/Rust | SIMD/Rust | SIMD/DTO |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| import | 1 | native | 12.691 (12.610–12.815) | 21.635 (21.418–22.597) | 19.620 (19.378–32.457) | 1.70× | 1.55× | 0.91× |
| prepare | 1 | native | 12.851 (12.705–17.122) | 26.248 (23.924–29.480) | 21.105 (20.611–28.516) | 2.04× | 1.64× | 0.80× |
| prepare | 1 | sorted | 13.673 (13.278–13.944) | 26.059 (25.615–28.039) | 20.802 (20.471–21.347) | 1.91× | 1.52× | 0.80× |
| variants | 1 | native | 3.844 (3.746–4.119) | 17.504 (17.287–20.399) | 12.580 (12.233–13.095) | 4.55× | 3.27× | 0.72× |
| variants | 2 | native | 2.116 (2.019–2.197) | 11.616 (11.400–12.599) | 8.370 (8.252–8.555) | 5.49× | 3.95× | 0.72× |
| variants | 4 | native | 1.371 (1.241–1.467) | 8.001 (7.893–8.174) | 5.984 (5.876–6.768) | 5.84× | 4.36× | 0.75× |
| variants | 8 | native | 1.323 (1.256–1.511) | 7.764 (7.496–8.467) | 5.878 (5.772–6.120) | 5.87× | 4.44× | 0.76× |
| complete | 1 | native | 30.039 (29.689–31.377) | 65.582 (64.862–70.754) | 53.470 (52.885–57.586) | 2.18× | 1.78× | 0.82× |
| complete | 2 | native | 28.096 (27.835–29.071) | 59.991 (58.092–62.202) | 49.074 (48.831–49.446) | 2.14× | 1.75× | 0.82× |
| complete | 4 | native | 29.308 (28.149–32.651) | 58.857 (56.470–60.638) | 51.313 (48.645–63.124) | 2.01× | 1.75× | 0.87× |
| complete | 8 | native | 27.873 (27.408–29.781) | 58.325 (56.589–61.878) | 47.580 (47.095–49.886) | 2.09× | 1.71× | 0.82× |

| Stage | Workers | Index | Rust peak MiB | GD DTO peak MiB | GD SIMD peak MiB |
|---|---:|---|---:|---:|---:|
| import | 1 | native | 21.3 | 19.1 | 15.8 |
| prepare | 1 | native | 52.2 | 47.5 | 39.7 |
| prepare | 1 | sorted | 52.1 | 47.5 | 39.7 |
| variants | 1 | native | 66.8 | 62.0 | 55.1 |
| variants | 2 | native | 67.5 | 69.5 | 55.9 |
| variants | 4 | native | 67.8 | 70.7 | 60.3 |
| variants | 8 | native | 67.8 | 81.1 | 60.1 |
| complete | 1 | native | 71.3 | 62.0 | 55.2 |
| complete | 2 | native | 71.5 | 69.4 | 56.0 |
| complete | 4 | native | 71.7 | 70.0 | 60.3 |
| complete | 8 | native | 71.8 | 80.8 | 61.6 |

## 1,000,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp).

| Stage | Workers | Index | Rust ms (range) | GD DTO ms (range) | GD SIMD ms (range) | DTO/Rust | SIMD/Rust | SIMD/DTO |
|---|---:|---|---:|---:|---:|---:|---:|---:|
| import | 1 | native | 122.650 (121.946–125.077) | 214.808 (210.892–219.274) | 193.050 (191.984–196.123) | 1.75× | 1.57× | 0.90× |
| prepare | 1 | native | 149.752 (148.052–152.214) | 289.751 (285.844–297.545) | 243.110 (240.498–246.243) | 1.93× | 1.62× | 0.84× |
| prepare | 1 | sorted | 180.355 (162.075–197.550) | 287.984 (278.014–339.956) | 244.494 (242.170–266.692) | 1.60× | 1.36× | 0.85× |
| variants | 1 | native | 44.981 (43.100–46.395) | 184.785 (175.516–191.464) | 172.116 (170.721–175.907) | 4.11× | 3.83× | 0.93× |
| variants | 2 | native | 24.547 (23.956–25.045) | 118.926 (117.853–122.274) | 109.228 (107.516–111.603) | 4.84× | 4.45× | 0.92× |
| variants | 4 | native | 16.549 (15.141–17.850) | 81.181 (79.609–84.910) | 75.379 (73.446–79.775) | 4.91× | 4.55× | 0.93× |
| variants | 8 | native | 13.837 (13.235–14.229) | 78.915 (77.477–81.756) | 66.713 (63.067–69.653) | 5.70× | 4.82× | 0.85× |
| complete | 1 | native | 346.007 (331.933–380.258) | 727.403 (704.445–755.199) | 634.601 (629.579–660.559) | 2.10× | 1.83× | 0.87× |
| complete | 2 | native | 330.883 (315.560–347.842) | 658.766 (638.002–689.698) | 568.360 (564.130–585.577) | 1.99× | 1.72× | 0.86× |
| complete | 4 | native | 314.239 (304.649–363.200) | 622.980 (611.757–646.503) | 538.990 (529.533–554.388) | 1.98× | 1.72× | 0.87× |
| complete | 8 | native | 307.439 (302.362–333.618) | 625.669 (604.259–638.777) | 531.271 (521.049–568.838) | 2.04× | 1.73× | 0.85× |

| Stage | Workers | Index | Rust peak MiB | GD DTO peak MiB | GD SIMD peak MiB |
|---|---:|---|---:|---:|---:|
| import | 1 | native | 92.7 | 109.4 | 91.1 |
| prepare | 1 | native | 433.2 | 418.4 | 353.4 |
| prepare | 1 | sorted | 433.2 | 418.4 | 353.4 |
| variants | 1 | native | 590.9 | 560.7 | 518.2 |
| variants | 2 | native | 698.8 | 639.9 | 550.5 |
| variants | 4 | native | 649.2 | 662.2 | 554.2 |
| variants | 8 | native | 669.4 | 725.8 | 591.0 |
| complete | 1 | native | 591.8 | 561.1 | 518.3 |
| complete | 2 | native | 644.2 | 588.9 | 528.5 |
| complete | 4 | native | 592.1 | 610.4 | 524.0 |
| complete | 8 | native | 670.5 | 686.4 | 537.8 |

## Source and executable sizes

Sources: [Rust application](../../benches/order_workflow/workload.rs), [Rust selection APIs](../../src/table/selection.rs), [C++ application and adapters](../../benches/cpp-reference/order_workflow/workload.hpp), [GD SIMD adapter](../../benches/cpp-reference/order_workflow/simd_table.hpp), [size measurement code](../../benches/order_workflow/compare.py).

| Component | Physical lines | Nonblank lines | Bytes |
|---|---:|---:|---:|
| rust_application | 213 | 203 | 6,987 |
| rust_selection_library | 175 | 167 | 7,045 |
| rust_column_views_library | 469 | 423 | 17,406 |
| rust_sqlite_library | 800 | 751 | 31,519 |
| rust_driver | 220 | 214 | 7,675 |
| cpp_application_and_adapters | 154 | 153 | 7,541 |
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

The million-row timings use additional CPU checks before and during each invocation.
The controller waits while an unrelated process exceeds 60% of one CPU and repeats
any invocation with observed activity. Excluded samples, CPU observations, and the
controller source are retained in the raw JSON; only accepted timings enter the tables.
Source fingerprints identify the measured current worktree; the recorded Git revision
is its base revision.

Sources: [fixture and SQL oracle](../../benches/order_workflow/fixture.py), [Rust verifier](../../benches/order_workflow/driver.rs), [C++ verifier](../../benches/cpp-reference/order_workflow/driver.cpp).

52 verification invocations completed: the hand fixture and each measured size, every implementation at every worker count, plus the Rust sorted-index diagnostic. Every cell was compared with independent SQL; all output counts and digests matched across languages, index algorithms, and worker counts.

- 11 input lines → variant counts `[5, 4, 0, 0, 0, 0, 1, 3]`.
- 10,000 input lines → variant counts `[7000, 1695, 412, 255, 939, 1001, 5365, 167]`.
- 100,000 input lines → variant counts `[70174, 17099, 4093, 2614, 8689, 11217, 54149, 1641]`.
- 1,000,000 input lines → variant counts `[701831, 171703, 41146, 26177, 86358, 113317, 541638, 16323]`.

Environment details follow. The [raw measurement JSON](measurements/order-workflow-m3max.json) additionally records every source hash, command, runtime library, rejection diagnostic, and process-load snapshot.

```text
utc: 2026-10-06T07:36:10.886832+00:00
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
gd_rs_revision: 1b413a5eed7884a06a4866315e8d29870b44aed3
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
invocation: ['/Users/kjn/repos/gd-rs/benches/order_workflow/compare.py', '--skip-build', '--samples', '5', '--rounds', '3', '--output', 'target/order-workflow/current-results.json']
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
