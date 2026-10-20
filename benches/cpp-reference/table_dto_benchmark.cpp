// GD's "[benchmark] table dto" test converted to Google Benchmark.
//
// The table setup and every loop body are unchanged from the upstream test; only the
// gd::benchmark::run_s harness is replaced. Each iteration's total is compared with the
// checksum computed during setup. The Rust counterpart is benches/table_dto.rs.

#include <benchmark/benchmark.h>

#include <cstdint>

#include "gd_table_column-buffer.h"

namespace {

using gd::table::dto::table;

constexpr uint64_t kRows = 10'000;

struct DtoFixture {
    table tableDto{(table::eTableFlagNull32 | table::eTableFlagRowStatus),
                   {{"uint64", "C1"}, {"uint64", "C2"}, {"uint64", "C3"}, {"uint64", "SUM"}},
                   gd::table::tag_prepare{}};
    uint64_t uCheckSum = 0;

    DtoFixture() {
        for(uint64_t u = 0; u < kRows; ++u) {
            auto uRow = tableDto.row_add_one();
            tableDto.cell_set(uRow, 0, u);
            tableDto.cell_set(uRow, 1, u * 2);
            tableDto.cell_set(uRow, 2, u * 3);
        }
        for(auto itRow : tableDto) {
            for(unsigned uCol = 0; uCol < 3; ++uCol) {
                uCheckSum += itRow.cell_get_variant_view(uCol).get_uint64();
            }
        }
    }
};

template<class Body>
void RunDto(benchmark::State& state, Body body) {
    DtoFixture fixture;
    for(auto _ : state) {
        const uint64_t uTotalSum = body(fixture.tableDto);
        benchmark::DoNotOptimize(uTotalSum);
        if(uTotalSum != fixture.uCheckSum) {
            state.SkipWithError("checksum mismatch");
            break;
        }
    }
    state.SetItemsProcessed(static_cast<int64_t>(state.iterations()) * static_cast<int64_t>(kRows));
}

void Sum01_RowIterator(benchmark::State& state) {
    RunDto(state, [](table& tableDto) {
        uint64_t uTotalSum = 0;
        for(auto itRow = tableDto.begin(); itRow != tableDto.end(); ++itRow) {
            uint64_t uSum = 0;
            for(unsigned uCol = 0; uCol < 3; ++uCol) {
                auto value = itRow.cell_get_variant_view(uCol);
                if(value.is_null() == false) uSum += value.get_uint64();
            }
            itRow.cell_set(3, uSum);
            uTotalSum += uSum;
        }
        return uTotalSum;
    });
}

void Sum02_IndexedNullCheck(benchmark::State& state) {
    RunDto(state, [](table& tableDto) {
        uint64_t uTotalSum = 0;
        for(uint64_t uRow = 0; uRow < tableDto.size(); ++uRow) {
            uint64_t uSum = 0;
            for(unsigned uCol = 0; uCol < 3; ++uCol) {
                auto value = tableDto.cell_get_variant_view(uRow, uCol);
                if(value.is_null() == false) uSum += value.get_uint64();
            }
            tableDto.cell_set(uRow, 3, uSum);
            uTotalSum += uSum;
        }
        return uTotalSum;
    });
}

void Sum03_IndexedNoNullCheck(benchmark::State& state) {
    RunDto(state, [](table& tableDto) {
        uint64_t uTotalSum = 0;
        for(uint64_t uRow = 0; uRow < tableDto.size(); ++uRow) {
            uint64_t uSum = 0;
            for(unsigned uCol = 0; uCol < 3; ++uCol) {
                auto value = tableDto.cell_get_variant_view(uRow, uCol);
                uSum += value.get_uint64();
            }
            tableDto.cell_set(uRow, 3, uSum);
            uTotalSum += uSum;
        }
        return uTotalSum;
    });
}

void SumSpecial_CastNoNullCheck(benchmark::State& state) {
    RunDto(state, [](table& tableDto) {
        uint64_t uTotalSum = 0;
        for(uint64_t uRow = 0; uRow < tableDto.size(); ++uRow) {
            uint64_t uSum = 0;
            for(unsigned uCol = 0; uCol < 3; ++uCol) {
                auto value = tableDto.cell_get_variant_view(uRow, uCol);
                uSum += (uint64_t)value;
            }
            tableDto.cell_set(uRow, 3, uSum);
            uTotalSum += uSum;
        }
        return uTotalSum;
    });
}

void Sum04_StructThreeValues(benchmark::State& state) {
    RunDto(state, [](table& tableDto) {
        struct row {
            uint64_t uC1;
            uint64_t uC2;
            uint64_t uC3;
        };
        uint64_t uTotalSum = 0;
        for(uint64_t uRow = 0; uRow < tableDto.size(); ++uRow) {
            uint64_t uSum = 0;
            auto* prow = (row*)tableDto.cell_get(uRow, 0);
            uSum = prow->uC1 + prow->uC2 + prow->uC3;
            uTotalSum += uSum;
            tableDto.cell_set(uRow, 3, uSum);
        }
        return uTotalSum;
    });
}

void Sum05_StructFourValues(benchmark::State& state) {
    RunDto(state, [](table& tableDto) {
        struct row {
            uint64_t uC1;
            uint64_t uC2;
            uint64_t uC3;
            uint64_t uSum;
        };
        uint64_t uTotalSum = 0;
        for(uint64_t uRow = 0; uRow < tableDto.size(); ++uRow) {
            uint64_t uSum = 0;
            auto* prow = (row*)tableDto.cell_get(uRow, 0);
            uSum = prow->uC1 + prow->uC2 + prow->uC3;
            uTotalSum += uSum;
            prow->uSum = uSum;
        }
        return uTotalSum;
    });
}

} // namespace

BENCHMARK(Sum01_RowIterator)->Name("TableDto/Sum01_RowIterator");
BENCHMARK(Sum02_IndexedNullCheck)->Name("TableDto/Sum02_IndexedNullCheck");
BENCHMARK(Sum03_IndexedNoNullCheck)->Name("TableDto/Sum03_IndexedNoNullCheck");
BENCHMARK(SumSpecial_CastNoNullCheck)->Name("TableDto/SumSpecial_CastNoNullCheck");
BENCHMARK(Sum04_StructThreeValues)->Name("TableDto/Sum04_StructThreeValues");
BENCHMARK(Sum05_StructFourValues)->Name("TableDto/Sum05_StructFourValues");
