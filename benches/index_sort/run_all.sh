#!/usr/bin/env bash
# Runs every index-sort benchmark in C++, Rust, Kotlin, and C# on this host.
#
#   benches/index_sort/run_all.sh fast <out-dir>       C++ GD's six cases at 1,000 and
#                                                      10^6 rows, except the 10^6-row
#                                                      selection sorts
#   benches/index_sort/run_all.sh selection <out-dir>  GD's 10^6-row selection sorts:
#                                                      one timed run each, about an
#                                                      hour in total
#   benches/index_sort/run_all.sh methods <out-dir>    every column of a 10,000-row
#                                                      table: GD's physical, index, and
#                                                      key sorts, and each port's sort
#
#   benches/index_sort/run_all.sh ports <out-dir>      Rust/C#/Kotlin optimization
#                                                      variants and gd-rs order views
#
# INDEX_SORT_ROWS sets the methods phase's row count. Above 100,000 rows GD's
# physical sort is left out, and each harness runs one operation per iteration.
#
# Run from the repository root. Results and toolchain versions go to <out-dir>.
set -euo pipefail

phase=${1:?usage: run_all.sh fast|selection|methods|ports <out-dir>}
out=$(mkdir -p "${2:?usage: run_all.sh fast|selection|methods|ports <out-dir>}" && cd "$2" && pwd)
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
git diff -- benches/index_sort benches/cpp-reference src Cargo.toml Cargo.lock > "$out/source.diff"
git ls-files -z --cached --others --exclude-standard benches/index_sort benches/cpp-reference src Cargo.toml Cargo.lock \
    | xargs -0 shasum -a 256 > "$out/source-sha256.txt"

if [ "$phase" != ports ]; then
    cmake -S "$bench/cpp" -B "$cpp_build" -DCMAKE_BUILD_TYPE=Release > /dev/null
    cmake --build "$cpp_build" -j > /dev/null
    ctest --test-dir "$cpp_build" --output-on-failure
fi
(cd "$bench/rust" && cargo build --release --benches 2> /dev/null)
(cd "$bench/csharp" && dotnet build -c Release > /dev/null)

case "$phase" in
    fast)
        uptime > "$out/fast-load.txt"
        "$cpp_build/index_sort_benchmark" \
            --benchmark_filter='/1000(000)?$' \
            --benchmark_repetitions=5 --benchmark_report_aggregates_only=true \
            --benchmark_out="$out/cpp.json" --benchmark_out_format=json
        uptime >> "$out/fast-load.txt"
        ;;
    selection)
        uptime > "$out/selection-load.txt"
        "$cpp_build/index_sort_benchmark" \
            --benchmark_filter='/1000000/iterations:1$' \
            --benchmark_out="$out/cpp-selection.json" --benchmark_out_format=json
        uptime >> "$out/selection-load.txt"
        ;;
    methods|ports)
        echo "rows ${INDEX_SORT_ROWS:-10000}" > "$out/methods-load.txt"
        uptime >> "$out/methods-load.txt"
        if [ "$phase" = methods ]; then
            "$cpp_build/sort_methods_benchmark" \
                --benchmark_repetitions=5 --benchmark_report_aggregates_only=true \
                --benchmark_out="$out/cpp.json" --benchmark_out_format=json
        fi
        (cd "$bench/rust" && CRITERION_HOME="$out/rust-criterion" cargo bench --bench sort_methods) | tee "$out/rust.txt"
        (cd "$bench/kotlin" && ./gradlew --quiet mainBenchmark) | tee "$out/kotlin.txt"
        python3 - "$bench/kotlin/build/reports/benchmarks/main" "$out/kotlin-reports" <<'PYREPORT'
from pathlib import Path
import shutil, sys
source = max(Path(sys.argv[1]).iterdir(), key=lambda p: p.name)
shutil.copytree(source, Path(sys.argv[2]) / source.name)
PYREPORT
        csharp_job=()
        if [ "$phase" = ports ]; then
            csharp_job=(--warmupCount 3 --iterationCount 10 --iterationTime 250 --memory)
        fi
        if [ "${INDEX_SORT_ROWS:-10000}" -gt 100000 ]; then
            csharp_job=(--invocationCount 1 --unrollFactor 1 --warmupCount 3 --iterationCount 10)
        fi
        (cd "$bench/csharp" && dotnet bin/Release/net10.0/IndexSort.dll \
            --filter '*.SortMethods.*' --exporters json --artifacts "$out/csharp-artifacts" ${csharp_job[@]+"${csharp_job[@]}"}) | tee "$out/csharp.txt"
        cp -R "$out/csharp-artifacts/results" "$out/csharp-reports"
        uptime >> "$out/methods-load.txt"
        ;;
    *)
        echo "unknown phase $phase" >&2
        exit 1
        ;;
esac
