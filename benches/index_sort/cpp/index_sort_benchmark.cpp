// Physical (selection) sort versus index sort of a GD table, after the six
// "sort table" cases in the upstream GD benchmark described at
// https://www.sweclockers.com/forum/post/21314377.
//
// Each iteration clears the target table, appends the source rows, and sorts on
// one column, exactly like the upstream lambdas. The upstream test uses 1000
// rows; every case also runs with 10^6 rows. Selection sort over 10^6 rows takes
// minutes, so those cases run a single iteration. Cases 01s-03s are not upstream:
// they replace GD's selection sort with std::sort over the table's rows.

#include <benchmark/benchmark.h>

#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <map>
#include <random>
#include <string>
#include <string_view>
#include <utility>

#include "gd_table_column-buffer.h"
#include "gd_table_row_iterator.h"

namespace {

using gd::table::dto::table;

constexpr std::int64_t kRowCount = 1000;
constexpr std::int64_t kMillionRows = 1'000'000;

table MakeEmptyTable() {
    return table(
        (table::eTableFlagNull32 | table::eTableFlagRowStatus),
        {{"uint64", 0, "C1"},
         {"string", 8, "C2"},
         {"double", 0, "C3"},
         {"double", 0, "C4"},
         {"int32", 0, "C5"},
         {"uint32", 0, "C6"},
         {"string", 50, "C7"}},
        gd::table::tag_prepare{});
}

// Same generator, distributions, and draw order as the upstream test.
table MakeSourceTable(std::int64_t iRowCount) {
    std::mt19937_64 generatorRandom(42);
    std::uniform_int_distribution<std::uint64_t> distributionInteger(0, 1'000'000);
    std::uniform_real_distribution<double> distributionDouble(0.0, 1'000.0);
    std::uniform_int_distribution<int> distributionCharacter('a', 'z');

    table tableNew = MakeEmptyTable();
    tableNew.reserve(static_cast<std::uint64_t>(iRowCount));
    for (std::int64_t i = 0; i < iRowCount; ++i) {
        auto uRow = tableNew.row_add_one();

        std::string stringValue(8, ' ');
        std::string stringValue2(25, ' ');
        for (auto& iCharacter : stringValue) {
            iCharacter = static_cast<char>(distributionCharacter(generatorRandom));
        }
        for (auto& iCharacter : stringValue2) {
            iCharacter = static_cast<char>(distributionCharacter(generatorRandom));
        }

        std::string stringValue6 = "01234567890123456789";
        stringValue6 += stringValue2;

        tableNew.cell_set(uRow, 0u, distributionInteger(generatorRandom));
        tableNew.cell_set(uRow, 1u, std::string_view(stringValue));
        tableNew.cell_set(uRow, 2u, distributionDouble(generatorRandom));
        tableNew.cell_set(uRow, 6u, std::string_view(stringValue6));
    }
    return tableNew;
}

const table& SourceTable(std::int64_t iRowCount) {
    static std::map<std::int64_t, table> mapSource;
    auto it = mapSource.find(iRowCount);
    if (it == mapSource.end()) {
        it = mapSource.emplace(iRowCount, MakeSourceTable(iRowCount)).first;
    }
    return it->second;
}

void RequireSorted(const table& tableSorted, unsigned uColumn) {
    for (std::uint64_t uRow = 1; uRow < tableSorted.size(); ++uRow) {
        auto valuePrev = tableSorted.cell_get_variant_view(uRow - 1, uColumn);
        auto valueCurr = tableSorted.cell_get_variant_view(uRow, uColumn);
        if (valueCurr < valuePrev) {
            std::cerr << "column " << uColumn << " is not sorted at row " << uRow << '\n';
            std::abort();
        }
    }
}

// Not an upstream case: physical sort with std::sort over the table's rows
// (../../cpp-reference/gd_table_row_iterator.h) instead of GD's selection sort.
void StdSortTable(benchmark::State& state) {
    const auto uColumn = static_cast<unsigned>(state.range(0));
    const table& tableSource = SourceTable(state.range(1));
    table tableTarget = MakeEmptyTable();
    tableTarget.reserve(tableSource.size());

    for (auto _ : state) {
        tableTarget.row_clear();
        tableTarget.append(tableSource);
        table_rows::Sort(tableTarget, uColumn);
        benchmark::DoNotOptimize(tableTarget.size());
        benchmark::ClobberMemory();
    }

    RequireSorted(tableTarget, uColumn);
    state.SetItemsProcessed(state.iterations() * state.range(1));
}

// Not an upstream case: the clear + append that every case pays before sorting.
void AppendOnly(benchmark::State& state) {
    const table& tableSource = SourceTable(state.range(0));
    table tableTarget = MakeEmptyTable();
    tableTarget.reserve(tableSource.size());

    for (auto _ : state) {
        tableTarget.row_clear();
        tableTarget.append(tableSource);
        benchmark::DoNotOptimize(tableTarget.size());
        benchmark::ClobberMemory();
    }
    state.SetItemsProcessed(state.iterations() * state.range(0));
}

// Arguments: sort column, row count.
template <bool kIndexSort>
void SortTable(benchmark::State& state) {
    const auto uColumn = static_cast<unsigned>(state.range(0));
    const table& tableSource = SourceTable(state.range(1));
    table tableTarget = MakeEmptyTable();
    tableTarget.reserve(tableSource.size());

    for (auto _ : state) {
        tableTarget.row_clear();
        tableTarget.append(tableSource);
        if constexpr (kIndexSort) {
            tableTarget.sort(uColumn, true, gd::table::tag_sort_index{});
        } else {
            tableTarget.sort(uColumn, true);
        }
        benchmark::DoNotOptimize(tableTarget.size());
        benchmark::ClobberMemory();
    }

    RequireSorted(tableTarget, uColumn);
    state.counters["row_bytes"] = tableTarget.size_row();
    state.counters["row_meta_bytes"] = tableTarget.size_row_meta();
    state.SetItemsProcessed(state.iterations() * state.range(1));
}

template <bool kIndexSort>
void Register(const char* pbszName, std::int64_t iColumn) {
    benchmark::RegisterBenchmark(pbszName, SortTable<kIndexSort>)
        ->Args({iColumn, kRowCount})
        ->Unit(benchmark::kMicrosecond);
    auto* pbenchmarkMillion = benchmark::RegisterBenchmark(pbszName, SortTable<kIndexSort>)
                                  ->Args({iColumn, kMillionRows})
                                  ->Unit(benchmark::kMillisecond);
    if constexpr (!kIndexSort) {
        // About n^2/2 = 5*10^11 comparisons: one iteration takes minutes.
        pbenchmarkMillion->Iterations(1);
    }
}

const bool bRegistered = [] {
    benchmark::RegisterBenchmark("sort_00_append_only", AppendOnly)
        ->Arg(kRowCount)
        ->Arg(kMillionRows)
        ->Unit(benchmark::kMicrosecond);
    // Cases 01-03: default sort (selection sort, swaps whole rows).
    Register<false>("sort_01_physical_uint64", 0);
    Register<false>("sort_02_physical_string8", 1);
    Register<false>("sort_03_physical_string50", 6);
    // Cases 04-06: tag_sort_index (stable_sort of row ids, then in-place cycle swaps).
    Register<true>("sort_04_index_uint64", 0);
    Register<true>("sort_05_index_string8", 1);
    Register<true>("sort_06_index_string50", 6);
    // Not upstream: cases 01-03 with std::sort on the rows instead of selection sort.
    for (auto [pbszName, iColumn] : {std::pair{"sort_01s_std_sort_uint64", 0},
                                     std::pair{"sort_02s_std_sort_string8", 1},
                                     std::pair{"sort_03s_std_sort_string50", 6}}) {
        benchmark::RegisterBenchmark(pbszName, StdSortTable)
            ->Args({iColumn, kRowCount})
            ->Unit(benchmark::kMicrosecond);
        benchmark::RegisterBenchmark(pbszName, StdSortTable)
            ->Args({iColumn, kMillionRows})
            ->Unit(benchmark::kMillisecond);
    }
    return true;
}();

}  // namespace

BENCHMARK_MAIN();
