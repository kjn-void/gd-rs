# Order workflow: recorded measurements

Sources: [Rust application](../../benches/order_workflow/workload.rs), [C++ application](../../benches/cpp-reference/order_workflow/workload.hpp), [measurement runner](../../benches/order_workflow/compare.py). See the [analysis and limitations](order-workflow.md).

Times are median milliseconds across two alternating process rounds, five samples each, after one warmup per process. Ranges show minimum–maximum samples; they are not confidence intervals. C++/Rust is the elapsed-time ratio. Peak RSS is the largest whole-process high-water mark of the two rounds, including setup.

## 10,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp).

| Stage | Workers | Index | Rust ms (range) | C++ ms (range) | C++/Rust | Rust peak MiB | C++ peak MiB |
|---|---:|---|---:|---:|---:|---:|---:|
| import | 1 | native | 1.452 (1.434–1.477) | 2.110 (1.972–2.241) | 1.45× | 5.6 | 5.0 |
| prepare | 1 | native | 1.209 (1.202–1.247) | 2.098 (2.029–2.157) | 1.74× | 8.5 | 8.3 |
| prepare | 1 | sorted | 1.255 (1.237–1.288) | 1.994 (1.893–2.090) | 1.59× | 8.4 | 8.3 |
| variants | 1 | native | 0.552 (0.528–0.588) | 1.606 (1.521–1.651) | 2.91× | 10.5 | 10.0 |
| variants | 2 | native | 0.331 (0.321–0.347) | 1.291 (1.265–1.324) | 3.90× | 10.5 | 10.2 |
| variants | 4 | native | 0.209 (0.195–0.234) | 0.742 (0.715–0.763) | 3.54× | 10.7 | 10.4 |
| variants | 8 | native | 0.203 (0.171–0.271) | 0.719 (0.698–0.778) | 3.54× | 11.0 | 11.5 |
| complete | 1 | native | 3.302 (3.256–3.377) | 5.707 (5.580–5.829) | 1.73× | 10.6 | 10.1 |
| complete | 2 | native | 3.030 (2.935–3.095) | 5.480 (5.397–5.561) | 1.81× | 10.7 | 10.4 |
| complete | 4 | native | 2.891 (2.851–3.009) | 4.888 (4.766–4.931) | 1.69× | 10.8 | 10.4 |
| complete | 8 | native | 2.929 (2.875–3.050) | 4.957 (4.767–5.342) | 1.69× | 11.6 | 11.5 |

## 100,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp).

| Stage | Workers | Index | Rust ms (range) | C++ ms (range) | C++/Rust | Rust peak MiB | C++ peak MiB |
|---|---:|---|---:|---:|---:|---:|---:|
| import | 1 | native | 15.001 (14.907–15.250) | 20.780 (20.509–21.942) | 1.39× | 21.3 | 21.7 |
| prepare | 1 | native | 13.128 (12.998–13.333) | 24.231 (23.570–24.772) | 1.85× | 52.2 | 53.5 |
| prepare | 1 | sorted | 14.067 (14.001–14.165) | 24.365 (24.021–25.295) | 1.73× | 52.2 | 53.5 |
| variants | 1 | native | 5.685 (5.605–5.832) | 16.378 (16.127–17.075) | 2.88× | 69.2 | 72.5 |
| variants | 2 | native | 3.005 (2.985–3.043) | 12.984 (12.694–13.196) | 4.32× | 69.8 | 69.5 |
| variants | 4 | native | 1.849 (1.751–2.442) | 7.245 (7.140–7.332) | 3.92× | 80.6 | 74.0 |
| variants | 8 | native | 1.378 (1.317–1.734) | 6.685 (6.546–6.894) | 4.85× | 80.0 | 75.9 |
| complete | 1 | native | 34.344 (33.911–35.623) | 62.917 (61.664–64.063) | 1.83× | 77.8 | 68.3 |
| complete | 2 | native | 31.504 (30.682–32.116) | 57.158 (56.780–57.850) | 1.81× | 71.5 | 69.5 |
| complete | 4 | native | 29.991 (29.570–30.550) | 52.136 (51.156–52.507) | 1.74× | 71.6 | 71.7 |
| complete | 8 | native | 31.499 (31.380–32.153) | 53.185 (52.558–56.663) | 1.69× | 82.2 | 76.8 |

## 1,000,000 order lines

Sources: [Rust](../../benches/order_workflow/workload.rs), [C++](../../benches/cpp-reference/order_workflow/workload.hpp).

| Stage | Workers | Index | Rust ms (range) | C++ ms (range) | C++/Rust | Rust peak MiB | C++ peak MiB |
|---|---:|---|---:|---:|---:|---:|---:|
| import | 1 | native | 147.392 (146.734–149.987) | 202.315 (198.970–203.485) | 1.37× | 237.3 | 171.9 |
| prepare | 1 | native | 160.330 (154.193–167.336) | 286.535 (282.679–312.164) | 1.79× | 476.2 | 473.1 |
| prepare | 1 | sorted | 183.468 (178.622–190.128) | 281.811 (272.982–303.386) | 1.54× | 468.9 | 473.1 |
| variants | 1 | native | 59.314 (58.461–59.988) | 170.076 (167.060–183.006) | 2.87× | 657.1 | 644.0 |
| variants | 2 | native | 31.368 (30.812–31.688) | 128.037 (123.610–138.254) | 4.08× | 653.1 | 721.1 |
| variants | 4 | native | 19.000 (18.221–19.326) | 70.832 (69.089–79.321) | 3.73× | 670.0 | 696.0 |
| variants | 8 | native | 14.085 (13.680–14.565) | 71.745 (66.187–78.786) | 5.09× | 682.4 | 888.5 |
| complete | 1 | native | 374.406 (369.245–383.823) | 670.622 (645.327–698.872) | 1.79× | 829.1 | 670.2 |
| complete | 2 | native | 345.166 (339.786–351.664) | 646.154 (626.235–657.697) | 1.87× | 819.7 | 687.2 |
| complete | 4 | native | 332.683 (328.615–340.270) | 573.952 (561.360–626.941) | 1.73× | 827.6 | 687.8 |
| complete | 8 | native | 325.798 (323.762–336.699) | 570.669 (539.101–604.349) | 1.75× | 833.3 | 803.1 |

## Source and executable sizes

Sources: [Rust application](../../benches/order_workflow/workload.rs), [new Rust APIs](../../src/table/selection.rs), [C++ application and adapters](../../benches/cpp-reference/order_workflow/workload.hpp), [size measurement code](../../benches/order_workflow/compare.py).

| Component | Physical lines | Nonblank lines | Bytes |
|---|---:|---:|---:|
| rust_application | 175 | 164 | 5,862 |
| rust_library_addition | 155 | 148 | 6,125 |
| rust_driver | 219 | 213 | 7,637 |
| cpp_application_and_adapters | 133 | 132 | 6,578 |
| cpp_driver_and_pool | 214 | 211 | 10,995 |
| shared_fixture_and_runner | 339 | 311 | 17,883 |
| rust_api_tests | 128 | 124 | 4,603 |

Lines include comments; formatting differs between languages. The new Rust APIs also require a module declaration and one visibility change in existing files. Existing library and dependency source is excluded.

| Standalone program | Unstripped bytes | Stripped bytes |
|---|---:|---:|
| rust | 2,614,720 | 2,351,088 |
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
utc: 2026-10-04T12:37:41.965155+00:00
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
gd_rs_revision: cab6dd670f7b6955cd221987db5c09f4b5cafbbf
gd_source_sha256: c5ea0e43c4e31dd345c2311f82fdc8d0af5d53e9ed28fee8897f89dcbddfb2fd
seed: 7640891576956012809
rust_flags: release: opt-level=3, codegen-units=1, lto=thin; target-cpu=native; features sqlite,rayon; CFLAGS=-march=native
cpp_flags: Release: -O3 -DNDEBUG -march=native; CMAKE_INTERPROCEDURAL_OPTIMIZATION=ON; sanitizers OFF
affinity: OS scheduling, no affinity; persistent pools; one process at a time
invocation: ['/Users/kjn/repos/gd-rs/benches/order_workflow/compare.py']
fixture_sqlite: 3.53.4
samples: 5
rounds: 2
source_sha256: {'benches/order_workflow/driver.rs': '929e1dd7b8fcac426633fa9ce16a1814cc07eb8d44164c50573a9453bfeaec6d', 'benches/order_workflow/workload.rs': '42f76ae094e995e4ac07a9d047ab4605d3d1b7d7b78d3aee58270c7295e44801', 'benches/order_workflow/check_safety.py': 'edcb50df415c894a02e42e16dabb4388d6508dedb76eb168950016b29425d241', 'benches/order_workflow/compare.py': 'd08caea00b1e606e313ea19437ccbbcb6f41b644d944d6ab99e9f9619b376a4d', 'benches/order_workflow/fixture.py': 'c5d25592d2d5962e8747dab197a3fc6961601127cbac7ce8cb78d358b905ef50', 'benches/order_workflow/summarize.py': 'bc37e7b3b18282ce4c5c517760e162703fa4ebbd7c4c1b267fa64ea31f4ba336', 'benches/cpp-reference/order_workflow/driver.cpp': '1324296470a16a59c017bf7d41210cbbb070d7211134069e715e48006ac7cd74', 'benches/cpp-reference/order_workflow/probes.cpp': '49fdca83f5e0674cb029b9b72f24c5e5516261bf6b7dd60e8e0246e9e20a6326', 'benches/cpp-reference/order_workflow/pool.hpp': 'ce5c916ee51ac6228de37b3b05ffb7fa60d3c255cf2f69c7624518bddc0c79b3', 'benches/cpp-reference/order_workflow/workload.hpp': '8c3d2b7ecfab02af22df441fb7dfe64d26ab5a2f222d0e2db43f9e8308e44bb2', 'src/table/selection.rs': 'a559a62f8e3d91fd9328dd09d7526ec185551c35d9379023f2711fe5ea2f1138', 'Cargo.toml': 'fb48c158ad7b5b67adae210278838e872c55bf76dbe762a5babd028ffa998153', 'Cargo.lock': '4933cde8b8440649c2447025ba93ce15a7742500e11169e9f58928217ae7cc42'}
gd_source_unchanged: True
```
