// Physical (selection) sort versus index sort of a GD table, after the six
// "sort table" cases in the upstream GD benchmark described at
// https://www.sweclockers.com/forum/post/21314377.
//
// Each iteration clears the target table, appends the 1000 source rows, and
// sorts on one column, exactly like the upstream lambdas.

#include <benchmark/benchmark.h>

#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <random>
#include <string>
#include <string_view>

#include "gd_table_column-buffer.h"

namespace {

using gd::table::dto::table;

constexpr std::uint64_t kRowCount = 1000;

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
const table& SourceTable() {
    static const table tableSource = [] {
        std::mt19937_64 generatorRandom(42);
        std::uniform_int_distribution<std::uint64_t> distributionInteger(0, 1'000'000);
        std::uniform_real_distribution<double> distributionDouble(0.0, 1'000.0);
        std::uniform_int_distribution<int> distributionCharacter('a', 'z');

        table tableNew = MakeEmptyTable();
        for (std::uint64_t u = 0; u < kRowCount; ++u) {
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
    }();
    return tableSource;
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

template <bool kIndexSort>
void SortTable(benchmark::State& state) {
    const auto uColumn = static_cast<unsigned>(state.range(0));
    const table& tableSource = SourceTable();
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
    state.SetItemsProcessed(state.iterations() * static_cast<std::int64_t>(kRowCount));
}

// Not an upstream case: the clear + append that every case pays before sorting.
void AppendOnly(benchmark::State& state) {
    const table& tableSource = SourceTable();
    table tableTarget = MakeEmptyTable();
    tableTarget.reserve(tableSource.size());

    for (auto _ : state) {
        tableTarget.row_clear();
        tableTarget.append(tableSource);
        benchmark::DoNotOptimize(tableTarget.size());
        benchmark::ClobberMemory();
    }
    state.SetItemsProcessed(state.iterations() * static_cast<std::int64_t>(kRowCount));
}

BENCHMARK(AppendOnly)->Name("sort_00_append_only")->Unit(benchmark::kMicrosecond);

// Cases 01-03: default sort (selection sort, swaps whole rows).
BENCHMARK(SortTable<false>)
    ->Name("sort_01_physical_uint64")->Arg(0)->Unit(benchmark::kMicrosecond);
BENCHMARK(SortTable<false>)
    ->Name("sort_02_physical_string8")->Arg(1)->Unit(benchmark::kMicrosecond);
BENCHMARK(SortTable<false>)
    ->Name("sort_03_physical_string50")->Arg(6)->Unit(benchmark::kMicrosecond);
// Cases 04-06: tag_sort_index (stable_sort of row ids, then in-place cycle swaps).
BENCHMARK(SortTable<true>)
    ->Name("sort_04_index_uint64")->Arg(0)->Unit(benchmark::kMicrosecond);
BENCHMARK(SortTable<true>)
    ->Name("sort_05_index_string8")->Arg(1)->Unit(benchmark::kMicrosecond);
BENCHMARK(SortTable<true>)
    ->Name("sort_06_index_string50")->Arg(6)->Unit(benchmark::kMicrosecond);

}  // namespace

BENCHMARK_MAIN();
