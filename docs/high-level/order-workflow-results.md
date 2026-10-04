# Order workflow: recorded measurements

Sources: [Rust application](../../benches/order_workflow/workload.rs), [C++ application](../../benches/cpp-reference/order_workflow/workload.hpp), [measurement runner](../../benches/order_workflow/compare.py). See the [analysis and limitations](order-workflow.md), including the separately
recorded [preparation repeat](order-workflow.md#preparation-timing-repeat).

Times are median milliseconds across two alternating process rounds, five samples each, after one warmup per process. Ranges show minimum–maximum samples; they are not confidence intervals. C++/Rust is the elapsed-time ratio. Peak RSS is the largest whole-process high-water mark of the two rounds, including setup.

## 10,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp).

| Stage | Workers | Index | Rust ms (range) | C++ ms (range) | C++/Rust | Rust peak MiB | C++ peak MiB |
|---|---:|---|---:|---:|---:|---:|---:|
| import | 1 | native | 1.560 (1.542–2.611) | 2.234 (2.141–2.813) | 1.43× | 6.6 | 6.5 |
| prepare | 1 | native | 1.309 (1.290–1.447) | 2.248 (2.073–2.349) | 1.72× | 11.8 | 11.1 |
| prepare | 1 | sorted | 1.343 (1.309–1.481) | 2.229 (2.159–2.327) | 1.66× | 11.7 | 11.0 |
| variants | 1 | native | 0.617 (0.587–0.725) | 1.641 (1.486–1.787) | 2.66× | 11.9 | 11.7 |
| variants | 2 | native | 0.331 (0.311–0.390) | 1.385 (1.193–1.454) | 4.19× | 11.3 | 11.3 |
| variants | 4 | native | 0.213 (0.206–0.244) | 0.796 (0.759–0.953) | 3.73× | 11.4 | 11.2 |
| variants | 8 | native | 0.214 (0.192–0.261) | 0.780 (0.723–0.897) | 3.64× | 12.2 | 11.5 |
| complete | 1 | native | 6.160 (3.664–13.263) | 11.174 (7.649–19.448) | 1.81× | 17.7 | 15.0 |
| complete | 2 | native | 3.256 (3.201–3.581) | 5.758 (5.606–5.919) | 1.77× | 16.2 | 10.2 |
| complete | 4 | native | 3.094 (3.004–3.231) | 5.354 (5.223–5.668) | 1.73× | 11.2 | 12.3 |
| complete | 8 | native | 3.178 (3.093–3.564) | 5.287 (5.031–5.797) | 1.66× | 14.8 | 14.0 |

## 100,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp).

| Stage | Workers | Index | Rust ms (range) | C++ ms (range) | C++/Rust | Rust peak MiB | C++ peak MiB |
|---|---:|---|---:|---:|---:|---:|---:|
| import | 1 | native | 14.713 (14.651–14.892) | 21.078 (20.396–22.131) | 1.43× | 21.3 | 21.7 |
| prepare | 1 | native | 12.957 (12.686–13.290) | 25.044 (24.662–25.176) | 1.93× | 52.2 | 53.5 |
| prepare | 1 | sorted | 14.352 (14.091–14.747) | 24.344 (24.083–25.288) | 1.70× | 52.2 | 53.5 |
| variants | 1 | native | 5.825 (5.606–6.028) | 16.820 (16.527–17.307) | 2.89× | 69.1 | 68.1 |
| variants | 2 | native | 3.019 (2.964–3.149) | 13.170 (13.056–13.606) | 4.36× | 69.7 | 69.4 |
| variants | 4 | native | 1.887 (1.804–2.010) | 7.303 (7.156–7.532) | 3.87× | 78.3 | 75.7 |
| variants | 8 | native | 1.340 (1.304–1.942) | 7.338 (7.001–10.507) | 5.48× | 83.7 | 79.8 |
| complete | 1 | native | 34.317 (33.959–34.763) | 60.864 (58.515–63.648) | 1.77× | 71.2 | 68.1 |
| complete | 2 | native | 31.231 (30.650–31.603) | 59.850 (58.845–60.482) | 1.92× | 71.5 | 69.4 |
| complete | 4 | native | 30.225 (29.654–31.896) | 55.711 (54.283–58.432) | 1.84× | 72.0 | 82.6 |
| complete | 8 | native | 29.854 (29.307–30.907) | 53.830 (52.115–55.471) | 1.80× | 75.3 | 76.7 |

## 1,000,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp).

| Stage | Workers | Index | Rust ms (range) | C++ ms (range) | C++/Rust | Rust peak MiB | C++ peak MiB |
|---|---:|---|---:|---:|---:|---:|---:|
| import | 1 | native | 149.757 (147.823–153.167) | 210.068 (203.358–218.408) | 1.40× | 253.2 | 166.3 |
| prepare | 1 | native | 181.594 (157.768–222.868) | 310.762 (290.754–328.415) | 1.71× | 476.1 | 473.1 |
| prepare | 1 | sorted | 209.954 (180.739–325.766) | 296.253 (279.023–312.187) | 1.41× | 470.9 | 473.1 |
| variants | 1 | native | 63.714 (61.188–66.914) | 185.729 (172.803–206.798) | 2.92× | 690.2 | 649.7 |
| variants | 2 | native | 34.069 (31.991–43.392) | 135.745 (130.876–141.717) | 3.98× | 681.6 | 670.9 |
| variants | 4 | native | 19.524 (18.905–19.935) | 83.368 (74.490–96.165) | 4.27× | 682.1 | 775.7 |
| variants | 8 | native | 14.971 (13.786–18.961) | 70.743 (68.228–79.748) | 4.73× | 682.0 | 722.8 |
| complete | 1 | native | 373.798 (370.257–396.402) | 678.296 (660.787–743.817) | 1.81× | 823.7 | 670.2 |
| complete | 2 | native | 347.624 (344.558–392.517) | 651.967 (609.915–713.815) | 1.88× | 813.8 | 686.8 |
| complete | 4 | native | 354.633 (330.264–358.840) | 584.944 (558.914–629.665) | 1.65× | 850.1 | 688.2 |
| complete | 8 | native | 327.818 (322.461–331.463) | 578.533 (557.497–616.726) | 1.76× | 837.2 | 720.2 |

## Source and executable sizes

Sources: [Rust application](../../benches/order_workflow/workload.rs), [Rust selection APIs](../../src/table/selection.rs), [C++ application and adapters](../../benches/cpp-reference/order_workflow/workload.hpp), [size measurement code](../../benches/order_workflow/compare.py).

| Component | Physical lines | Nonblank lines | Bytes |
|---|---:|---:|---:|
| rust_application | 175 | 164 | 5,862 |
| rust_selection_library | 155 | 148 | 6,125 |
| rust_driver | 220 | 214 | 7,675 |
| cpp_application_and_adapters | 133 | 132 | 6,578 |
| cpp_driver_and_pool | 215 | 212 | 11,055 |
| shared_fixture_and_runner | 347 | 319 | 18,416 |
| rust_api_tests | 128 | 124 | 4,603 |

Lines include comments; formatting differs between languages. The Rust selection module is counted separately; other library and dependency source is excluded.

| Standalone program | Unstripped bytes | Stripped bytes |
|---|---:|---:|
| rust | 2,649,392 | 2,384,272 |
| cpp | 1,401,952 | 1,325,104 |

Executables include the application, timing/correctness driver, retained library code, and SQLite. Rust also uses Rayon and serde_json; C++ uses the counted pool and a small JSON emitter. Both use system dynamic libraries, listed in the raw JSON. These are program footprints, not intrinsic table-library sizes.

## Verification and environment

Sources: [fixture and SQL oracle](../../benches/order_workflow/fixture.py), [Rust verifier](../../benches/order_workflow/driver.rs), [C++ verifier](../../benches/cpp-reference/order_workflow/driver.cpp).

36 verification invocations completed: the hand fixture and each measured size, both languages at every worker count, plus the Rust sorted-index diagnostic. Every cell was compared with independent SQL; all output counts and digests matched across languages, index algorithms, and worker counts.

- 11 input lines → variant counts `[5, 4, 0, 0, 0, 0, 1, 3]`.
- 10,000 input lines → variant counts `[7000, 1695, 412, 255, 939, 1001, 5365, 167]`.
- 100,000 input lines → variant counts `[70174, 17099, 4093, 2614, 8689, 11217, 54149, 1641]`.
- 1,000,000 input lines → variant counts `[701831, 171703, 41146, 26177, 86358, 113317, 541638, 16323]`.

```text
utc: 2026-10-04T13:12:11.618920+00:00
platform: macOS-27.0.1-arm64-arm-64bit-Mach-O
logical_cpus: 16
cpu: Apple M3 Max
cpu_topology: hw.physicalcpu: 16
hw.logicalcpu: 16
hw.perflevel0.physicalcpu: 12
hw.perflevel1.physicalcpu: 4
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
gd_rs_revision: 9bd95cd9be1c15ae7a759a75202a16ec41160c0a
gd_source_sha256: a2261ae9166c0f372d98fd698d481dc76014bef2b7d9a6314d1e6d3282abaef6
seed: 7640891576956012809
rust_flags: release: opt-level=3, codegen-units=1, lto=thin; target-cpu=native; features sqlite,rayon; CFLAGS=-march=native
cpp_flags: Release: -O3 -DNDEBUG -march=native; CMAKE_INTERPROCEDURAL_OPTIMIZATION=ON; sanitizers OFF
affinity: OS scheduling, no affinity; persistent pools; one process at a time
invocation: ['/Users/kjn/repos/gd-rs/benches/order_workflow/compare.py', '--skip-build', '--output', 'target/order-workflow/submodule-results.json']
fixture_sqlite: 3.53.4
samples: 5
rounds: 2
sqlite_version: 3.53.2
source_sha256: {'benches/order_workflow/driver.rs': 'f14adc2937b60a19e6154b88b881fbbe9722600297e9802b9c6c526b2352ceee', 'benches/order_workflow/workload.rs': '42f76ae094e995e4ac07a9d047ab4605d3d1b7d7b78d3aee58270c7295e44801', 'benches/order_workflow/check_safety.py': 'b7c37e33da6fb40e8b9ba86ca0d7a1211f0d023b48905645cc2381dd46b6daa2', 'benches/order_workflow/compare.py': '1b2742f87843c26c1a79010ceb0a5b3c8c60d0bb82d9c18386f8b0d801115a81', 'benches/order_workflow/fixture.py': 'c5d25592d2d5962e8747dab197a3fc6961601127cbac7ce8cb78d358b905ef50', 'benches/order_workflow/summarize.py': 'e16e6ac979ffdda9435efcc4c18c246d5df440de365196c6f7234541ecbe9c38', 'benches/cpp-reference/order_workflow/driver.cpp': '7e13146e56beb53a211d2d8acd1c684fef07d71db787292d11ff8d23a57ff5cd', 'benches/cpp-reference/order_workflow/probes.cpp': '49fdca83f5e0674cb029b9b72f24c5e5516261bf6b7dd60e8e0246e9e20a6326', 'benches/cpp-reference/order_workflow/pool.hpp': 'ce5c916ee51ac6228de37b3b05ffb7fa60d3c255cf2f69c7624518bddc0c79b3', 'benches/cpp-reference/order_workflow/workload.hpp': '8c3d2b7ecfab02af22df441fb7dfe64d26ab5a2f222d0e2db43f9e8308e44bb2', 'src/table/selection.rs': 'a559a62f8e3d91fd9328dd09d7526ec185551c35d9379023f2711fe5ea2f1138', 'Cargo.toml': 'c309db897991a0e949393de568cc0d8f17713190025f0be87d10de6243842167', 'Cargo.lock': 'b9da6e1deb720058fa9f399811aa1c9fb28206c7b2370197b400473b56e7a6fb', 'benches/cpp-reference/CMakeLists.txt': '1627977c3d2c4677f11205392312ca7d45241c0ba25831f388379caa259aab27', 'benches/cpp-reference/cmake/GdCore.cmake': 'ed1a10df89727379366cc9aa45449b37c13c6ed7b2cbc725c6f4616fd48e29b8'}
gd_source_unchanged: True
```
