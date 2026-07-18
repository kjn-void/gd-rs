#include <benchmark/benchmark.h>

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <cstdlib>
#include <string_view>
#include <tuple>
#include <vector>

#include "gd_table_column-buffer.h"

namespace {

constexpr std::size_t kRows = 10'000'000;
using Column = std::tuple<std::string_view, unsigned, std::string_view>;

struct alignas(8) PriceRow
{
   double price;
   double tax;
   std::uint32_t qty;
};

static_assert(alignof(PriceRow) == 8);
static_assert(sizeof(PriceRow) == 24);
static_assert(offsetof(PriceRow, price) == 0);
static_assert(offsetof(PriceRow, tax) == 8);
static_assert(offsetof(PriceRow, qty) == 16);

gd::table::table_column_buffer MakeTable()
{
   gd::table::table_column_buffer table(static_cast<unsigned>(kRows));
   table.column_add(std::vector<Column>{{"double", 0, "price"},
                                        {"double", 0, "tax"},
                                        {"uint32", 0, "qty"},
                                        {"uint32", 0, "_padding"}},
                    gd::table::tag_type_name{});
   const auto prepared = table.prepare();
   if(!prepared.first || table.size_row() != sizeof(PriceRow) ||
      table.column_get(0).position() != offsetof(PriceRow, price) ||
      table.column_get(1).position() != offsetof(PriceRow, tax) ||
      table.column_get(2).position() != offsetof(PriceRow, qty))
      std::abort();
   for(std::size_t row = 0; row < kRows; ++row)
   {
      table.row_add({1.0 + static_cast<double>(row % 10'000) * 0.01,
                     static_cast<double>(row % 26),
                     static_cast<std::uint32_t>(row % 100 + 1),
                     std::uint32_t{0}});
   }
   return table;
}

#if defined(_MSC_VER)
__declspec(noinline)
#else
__attribute__((noinline))
#endif
void CalculateTotalCosts(const gd::table::table_column_buffer& table,
                         std::vector<double>& totals)
{
   if(table.get_row_count() != totals.size()) std::abort();
   for(std::size_t row = 0; row < totals.size(); ++row)
   {
      PriceRow value;
      std::memcpy(&value, table.row_get(row), sizeof(value));
      totals[row] = static_cast<double>(value.qty) * value.price * (1.0 + value.tax / 100.0);
   }
}

#if defined(_MSC_VER)
__declspec(noinline)
#else
__attribute__((noinline))
#endif
void CalculateTotalCostsUnrolled(const gd::table::table_column_buffer& table,
                                 std::vector<double>& totals)
{
   if(table.get_row_count() != totals.size()) std::abort();
#if defined(__clang__)
#pragma clang loop unroll_count(16)
#elif defined(__GNUC__)
#pragma GCC unroll 16
#endif
   for(std::size_t row = 0; row < totals.size(); ++row)
   {
      PriceRow value;
      std::memcpy(&value, table.row_get(row), sizeof(value));
      totals[row] = static_cast<double>(value.qty) * value.price * (1.0 + value.tax / 100.0);
   }
}

using Calculate = void (*)(const gd::table::table_column_buffer&, std::vector<double>&);

void PriceTotalAoS10M(benchmark::State& state, Calculate calculate)
{
   const auto table = MakeTable();
   std::vector<double> totals(kRows);
   calculate(table, totals);
   for(const auto row : {std::size_t{0}, std::size_t{1}, std::size_t{25},
                         std::size_t{9'999}, kRows - 1})
   {
      PriceRow value;
      std::memcpy(&value, table.row_get(row), sizeof(value));
      const auto expected = static_cast<double>(value.qty) * value.price *
                            (1.0 + value.tax / 100.0);
      if(totals[row] != expected) std::abort();
   }

   for(auto _ : state)
   {
      calculate(table, totals);
      benchmark::DoNotOptimize(totals.data());
      benchmark::ClobberMemory();
   }
   state.SetItemsProcessed(state.iterations() * static_cast<std::int64_t>(kRows));
   state.counters["input_mib"] =
      benchmark::Counter(static_cast<double>(table.size_reserved_total()) / (1024.0 * 1024.0));
   state.counters["output_mib"] =
      benchmark::Counter(static_cast<double>(kRows * sizeof(double)) / (1024.0 * 1024.0));
}
BENCHMARK_CAPTURE(PriceTotalAoS10M, compiler, CalculateTotalCosts)
   ->Name("PriceTotal/10000000/AoSCompiler")
   ->Unit(benchmark::kMillisecond);
BENCHMARK_CAPTURE(PriceTotalAoS10M, unrolled16, CalculateTotalCostsUnrolled)
   ->Name("PriceTotal/10000000/AoSUnrolled16")
   ->Unit(benchmark::kMillisecond);

} // namespace
