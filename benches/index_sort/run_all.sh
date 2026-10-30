#!/usr/bin/env bash
# Runs every index-sort benchmark in C++, Rust, Kotlin, and C# on this host.
#
#   benches/index_sort/run_all.sh fast <out-dir>       every case at 1,000 and 10^6
#                                                      rows, except GD's 10^6-row
#                                                      selection sorts
#   benches/index_sort/run_all.sh selection <out-dir>  GD's 10^6-row selection sorts:
#                                                      one timed run each, about an
#                                                      hour in total
#   benches/index_sort/run_all.sh gather <out-dir>     Rust, Kotlin, and C# at 10,000
#                                                      rows: index sort with rows
#                                                      swapped in place versus a new
#                                                      table built by gathering
#
# Run from the repository root. Results and toolchain versions go to <out-dir>.
set -euo pipefail

phase=${1:?usage: run_all.sh fast|selection|gather <out-dir>}
out=$(mkdir -p "${2:?usage: run_all.sh fast|selection|gather <out-dir>}" && cd "$2" && pwd)
root=$(pwd)
bench="$root/benches/index_sort"
cpp_build="$root/target/index_sort/cpp"

{
    date -u +%Y-%m-%dT%H:%M:%SZ
    uname -a
    sysctl -n machdep.cpu.brand_string 2>/dev/null || true
    c++ --version | head -1
    rustc --version
    java -version 2>&1 | head -1
    dotnet --version
    git -C "$root" rev-parse HEAD
    git -C "$root/external/gd" rev-parse HEAD
} > "$out/$phase-environment.txt"

cmake -S "$bench/cpp" -B "$cpp_build" -DCMAKE_BUILD_TYPE=Release > /dev/null
cmake --build "$cpp_build" -j > /dev/null
(cd "$bench/rust" && cargo build --release --benches 2> /dev/null)
(cd "$bench/csharp" && dotnet build -c Release > /dev/null)

case "$phase" in
    fast)
        # Earlier runs' reports would otherwise be copied along with this run's.
        rm -rf "$bench/kotlin/build/reports/benchmarks" "$bench/csharp/BenchmarkDotNet.Artifacts"
        uptime > "$out/fast-load.txt"
        "$cpp_build/index_sort_benchmark" \
            --benchmark_filter='/1000(000)?$' \
            --benchmark_repetitions=5 --benchmark_report_aggregates_only=true \
            --benchmark_out="$out/cpp.json" --benchmark_out_format=json
        (cd "$bench/rust" && cargo bench --bench index_sort -- '^1000(000)?/') | tee "$out/rust.txt"
        (cd "$bench/kotlin" && ./gradlew --quiet mainBenchmark) | tee "$out/kotlin.txt"
        cp -R "$bench/kotlin/build/reports/benchmarks/main" "$out/kotlin-reports"
        (cd "$bench/csharp" && dotnet bin/Release/net10.0/IndexSort.dll \
            --filter '*.ThousandRows.*' '*.MillionRows.*' --exporters json) | tee "$out/csharp.txt"
        cp -R "$bench/csharp/BenchmarkDotNet.Artifacts/results" "$out/csharp-reports"
        uptime >> "$out/fast-load.txt"
        ;;
    selection)
        uptime > "$out/selection-load.txt"
        "$cpp_build/index_sort_benchmark" \
            --benchmark_filter='/1000000/iterations:1$' \
            --benchmark_out="$out/cpp-selection.json" --benchmark_out_format=json
        uptime >> "$out/selection-load.txt"
        ;;
    gather)
        rm -rf "$bench/kotlin/build/reports/benchmarks" "$bench/csharp/BenchmarkDotNet.Artifacts"
        uptime > "$out/gather-load.txt"
        (cd "$bench/rust" && cargo bench --bench index_sort -- '^10000/') | tee "$out/rust.txt"
        (cd "$bench/kotlin" && ./gradlew --quiet mainGatherBenchmark) | tee "$out/kotlin.txt"
        cp -R "$bench/kotlin/build/reports/benchmarks/gather" "$out/kotlin-reports"
        (cd "$bench/csharp" && dotnet bin/Release/net10.0/IndexSort.dll \
            --filter '*.TenThousandRows.*' --exporters json) | tee "$out/csharp.txt"
        cp -R "$bench/csharp/BenchmarkDotNet.Artifacts/results" "$out/csharp-reports"
        uptime >> "$out/gather-load.txt"
        ;;
    *)
        echo "unknown phase $phase" >&2
        exit 1
        ;;
esac
