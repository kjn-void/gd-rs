#!/usr/bin/env bash
# Run the implementations in RESULTS.md's current seven-column comparison.
# Usage (from the repository root):
#   INDEX_SORT_ROWS=10000 benches/index_sort/run_comparison.sh <new-output-dir>
# Build and verify first, then time each harness sequentially on an idle host.
set -euo pipefail

out_arg=${1:?usage: run_comparison.sh <new-output-dir>}
mkdir -p "$out_arg"
out=$(cd "$out_arg" && pwd)
root=$(pwd)
bench="$root/benches/index_sort"
cpp_build="$root/target/index_sort/cpp"
jobs=${INDEX_SORT_BUILD_JOBS:-2}
export INDEX_SORT_ROWS=${INDEX_SORT_ROWS:-10000}

if [ -e "$out/comparison-environment.txt" ]; then
    echo "Refusing to overwrite an existing comparison: $out" >&2
    exit 1
fi

{
    date -u +%Y-%m-%dT%H:%M:%SZ
    uname -a
    if command -v lscpu >/dev/null; then lscpu; fi
    if [ -r /proc/device-tree/model ]; then
        printf 'board='; tr '\0' '\n' < /proc/device-tree/model
    fi
    if command -v free >/dev/null; then free -h; fi
    printf 'page_size='; getconf PAGESIZE
    if command -v sysctl >/dev/null; then
        sysctl machdep.cpu.brand_string hw.memsize hw.physicalcpu 2>/dev/null || true
    fi
    c++ --version
    cmake --version | head -1
    rustc --version
    java -version 2>&1
    dotnet --info
    git rev-parse HEAD
    git branch --show-current
    git -C external/gd rev-parse HEAD
    printf 'rows=%s; build_jobs=%s; benchmark_threads=1; affinity=none\n' "$INDEX_SORT_ROWS" "$jobs"
    printf 'RUSTFLAGS=%s\nCARGO_ENCODED_RUSTFLAGS=%s\n' "${RUSTFLAGS-<unset>}" "${CARGO_ENCODED_RUSTFLAGS-<unset>}"
    printf '%s\n' 'C++: Release; no architecture override; Google Benchmark 2 processes × 5 repetitions' \
        'C++ SoA: cached key/index sort; direct column gather into retained output/string buffers; no preliminary append' \
        'Rust: default bench profile; gd-rs default-features=false, german-strings=true; Criterion 1 s warm-up / 3 s measurement / 100 samples' \
        'C#: Release; optimized policy; 3 warm-ups / 10 iterations of 250 ms / memory diagnostics' \
        'Kotlin: optimized policy; JMH 3 forks / 5 warm-ups / 5 measurements of 1 s'
} > "$out/comparison-environment.txt"
git diff -- benches/index_sort benches/cpp-reference src Cargo.toml Cargo.lock > "$out/source.diff"
git ls-files -z --cached --others --exclude-standard benches/index_sort benches/cpp-reference src Cargo.toml Cargo.lock \
    | xargs -0 shasum -a 256 > "$out/source-sha256.txt"
cp "$bench/run_comparison.sh" "$out/command.sh"

echo 'Building and verifying C++'
cmake -S "$bench/cpp" -B "$cpp_build" -DCMAKE_BUILD_TYPE=Release > "$out/cpp-configure.txt" 2>&1
cmake --build "$cpp_build" --parallel "$jobs" > "$out/cpp-build.txt" 2>&1
ctest --test-dir "$cpp_build" --output-on-failure > "$out/cpp-tests.txt" 2>&1
cp "$cpp_build/CMakeFiles/sort_methods_benchmark.dir/flags.make" "$out/cpp-flags.txt"

echo 'Building and verifying Rust'
cargo test --manifest-path "$bench/rust/Cargo.toml" --release --locked -j "$jobs" > "$out/rust-tests.txt" 2>&1
cargo build --manifest-path "$bench/rust/Cargo.toml" --release --locked --bench sort_methods -j "$jobs" > "$out/rust-build.txt" 2>&1

echo 'Building and verifying C#'
(cd "$bench/csharp" && dotnet build -c Release --disable-build-servers && dotnet bin/Release/net10.0/IndexSort.dll verify) > "$out/csharp-tests.txt" 2>&1

echo 'Building and verifying Kotlin'
(cd "$bench/kotlin" && ./gradlew --no-daemon --max-workers="$jobs" test mainBenchmarkJar) > "$out/kotlin-tests.txt" 2>&1
(cd "$bench/kotlin" && ./gradlew --stop) > "$out/gradle-stop.txt" 2>&1

# Multi-core compilation can heat a small host beyond its steady single-core
# benchmark temperature. Let it cool before the first timed process starts.
if [ -r /sys/class/thermal/thermal_zone0/temp ]; then
    echo 'Waiting for CPU temperature below 75 C before timing'
    while [ "$(cat /sys/class/thermal/thermal_zone0/temp)" -gt 75000 ]; do
        date -u +%Y-%m-%dT%H:%M:%SZ >> "$out/cooldown.txt"
        cat /sys/class/thermal/thermal_zone0/temp >> "$out/cooldown.txt"
        sleep 5
    done
fi

snapshot() {
    date -u +%Y-%m-%dT%H:%M:%SZ
    uptime
    if [ "$(uname -s)" = Linux ]; then
        ps -eo pid,pcpu,comm --sort=-pcpu | head -15 || true
        for zone in /sys/class/thermal/thermal_zone*; do
            if [ -r "$zone/temp" ]; then
                printf '%s temperature_millidegrees_c=' "$(cat "$zone/type")"; cat "$zone/temp"
            fi
        done
        for policy in /sys/devices/system/cpu/cpufreq/policy*; do
            if [ -r "$policy/scaling_cur_freq" ]; then
                printf '%s khz=' "$policy"; cat "$policy/scaling_cur_freq"
                printf 'governor='; cat "$policy/scaling_governor"
            fi
        done
    else
        ps -Ao pid,pcpu,comm -r | head -15 || true
    fi
}
monitor_pid=
cleanup() { if [ -n "$monitor_pid" ]; then kill "$monitor_pid" 2>/dev/null || true; fi; }
trap cleanup EXIT
(while true; do snapshot; sleep 15; done) > "$out/load.txt" 2>&1 &
monitor_pid=$!

for name in cpp cpp-repeat; do
    echo "Benchmarking C++: $name"
    "$cpp_build/sort_methods_benchmark" --benchmark_filter='^(key|std_sort|soa_rows)/' \
        --benchmark_repetitions=5 --benchmark_report_aggregates_only=true \
        --benchmark_out="$out/$name.json" --benchmark_out_format=json > "$out/$name.txt" 2>&1
done

echo 'Benchmarking Rust key sort and both gd-rs workflows'
CRITERION_HOME="$out/rust-criterion" cargo bench --manifest-path "$bench/rust/Cargo.toml" --bench sort_methods \
    -- '^(sort_methods/key/|gd_rs/(order_view|view_materialize)/)' > "$out/rust.txt" 2>&1

echo 'Benchmarking C# optimized policy'
(cd "$bench/csharp" && dotnet bin/Release/net10.0/IndexSort.dll \
    --filter '*SortMethods*optimized*' --warmupCount 3 --iterationCount 10 --iterationTime 250 \
    --memory --exporters json --artifacts "$out/csharp-artifacts") > "$out/csharp.txt" 2>&1
cp -R "$out/csharp-artifacts/results" "$out/csharp-reports"

echo 'Benchmarking Kotlin optimized policy'
mkdir -p "$out/kotlin-reports/comparison"
java -jar "$bench/kotlin/build/benchmarks/main/jars/index-sort-bench-main-jmh-JMH.jar" \
    'indexsort.SortMethods.index' -p variant=optimized -f 3 -wi 5 -i 5 -w 1s -r 1s -t 1 \
    -rf json -rff "$out/kotlin-reports/comparison/main.json" > "$out/kotlin.txt" 2>&1
snapshot >> "$out/load.txt"
python3 "$bench/summarize.py" "$out" | tee "$out/summary.txt"
echo "Comparison complete: $out"
