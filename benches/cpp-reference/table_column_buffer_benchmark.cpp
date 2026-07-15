#include <benchmark/benchmark.h>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstring>
#include <cstdlib>
#include <cstdio>
#include <cstdint>
#include <limits>
#include <string>
#include <string_view>
#include <tuple>
#include <type_traits>
#include <vector>

#include "gd_table_column-buffer.h"
#include "gd_table_arguments.h"
#include "gd_table_io.h"

namespace {

using Column = std::tuple<std::string_view, unsigned, std::string_view>;

constexpr std::size_t kMixedNumericRows = 10'000'000;

struct NumericSummary
{
   long double average;
   long double minimum;
   long double maximum;
   long double median;
};

constexpr std::array<NumericSummary, 6> kExpectedMixedNumeric{{
   {124.999272L, 0, 250, 125},
   {-0.25L, -2'500'000, 2'499'999.5L, -0.25L},
   {32709.5755948L, 0, 65520, 32679},
   {4'999'999'500.0L, 0, 9'999'999'000.0L, 4'999'999'500.0L},
   {-0.5L, -5'000'000, 4'999'999, -0.5L},
   {-0.5L, -5'000'000, 4'999'999, -0.5L},
}};

bool Close(long double left, long double right)
{
   return std::fabs(left - right) <= 0.000001L;
}

gd::table::table_column_buffer MakeTable(std::size_t rows)
{
   gd::table::table_column_buffer table(static_cast<unsigned>(rows));
   table.column_add(std::vector<Column>{{"uint64", 0, "id"},
                                        {"string", 16, "group"},
                                        {"int64", 0, "value"}},
                    gd::table::tag_type_name{});
   const auto prepared = table.prepare();
   if(!prepared.first) std::abort();
   for(std::size_t row = 0; row < rows; ++row)
   {
      const std::string group = "group-" + std::to_string(row % 16);
      table.row_add({static_cast<std::uint64_t>(row),
                     std::string_view(group),
                     static_cast<std::int64_t>(row)});
   }
   return table;
}

gd::table::table_column_buffer MakeTablePrepared(std::size_t rows)
{
   std::vector<std::string> groups;
   groups.reserve(16);
   for(std::size_t group = 0; group < 16; ++group)
   {
      groups.push_back("group-" + std::to_string(group));
   }

   gd::table::table_column_buffer table(static_cast<unsigned>(rows));
   table.column_add(std::vector<Column>{{"uint64", 0, "id"},
                                        {"string", 16, "group"},
                                        {"int64", 0, "value"}},
                    gd::table::tag_type_name{});
   const auto prepared = table.prepare();
   if(!prepared.first) std::abort();
   for(std::size_t row = 0; row < rows; ++row)
   {
      table.row_add({static_cast<std::uint64_t>(row),
                     std::string_view(groups[row % groups.size()]),
                     static_cast<std::int64_t>(row)});
   }
   return table;
}

gd::table::table_column_buffer MakeSortTable(std::size_t rows)
{
   gd::table::table_column_buffer table(static_cast<unsigned>(rows));
   table.column_add(std::vector<Column>{{"uint64", 0, "key"}},
                    gd::table::tag_type_name{});
   const auto prepared = table.prepare();
   if(!prepared.first) std::abort();
   for(std::size_t row = 0; row < rows; ++row)
   {
      const auto key = (static_cast<std::uint64_t>(row) * 48271U) % rows;
      table.row_add(std::vector<gd::variant_view>{key});
   }
   return table;
}

gd::table::arguments::table MakeOpenTable(std::size_t rows)
{
   gd::table::arguments::table table(static_cast<unsigned>(rows), gd::table::tag_full_meta{});
   table.column_prepare();
   table.column_add("uint64", 0, "id");
   table.prepare();
   for(std::size_t row = 0; row < rows; ++row)
   {
      const auto row_index = table.row_add_one();
      table.cell_set(row_index, 0U, gd::variant_view(static_cast<std::uint64_t>(row)));
      table.cell_set(row_index,
                     "custom_category",
                     gd::variant_view(row % 2 == 0 ? "binary" : "text"));
      table.cell_set(row_index,
                     "custom_region",
                     gd::variant_view(row % 3 == 0 ? "north" : "south"));
   }
   return table;
}

std::vector<std::string> MakeWideFieldNames()
{
   std::vector<std::string> names;
   names.reserve(1000);
   for(std::size_t field = 0; field < 1000; ++field)
   {
      char name[16];
      std::snprintf(name, sizeof(name), "field_%04zu", field);
      names.emplace_back(name);
   }
   return names;
}

gd::table::arguments::table MakeWideOpenTable(const std::vector<std::string>& names, bool appendOnly)
{
   gd::table::arguments::table table(1000U, gd::table::tag_full_meta{});
   table.column_prepare();
   table.column_add("uint64", 0, "id");
   table.prepare();
   for(std::size_t row = 0; row < 1000; ++row)
   {
      const auto row_index = table.row_add_one();
      table.cell_set(row_index, 0U, gd::variant_view(static_cast<std::uint64_t>(row)));
      for(std::size_t field = 0; field < names.size(); ++field)
      {
         const auto value = gd::variant_view(static_cast<std::uint64_t>(row + field));
         if(appendOnly)
            table.cell_add_argument(row_index, names[field], value);
         else
            table.cell_set(row_index, names[field], value);
      }
   }
   return table;
}

gd::table::table_column_buffer MakeMixedNumericTable(std::size_t rows)
{
   gd::table::table_column_buffer table(static_cast<unsigned>(rows));
   table.column_add(std::vector<Column>{{"uint8", 0, "u8_value"},
                                        {"double", 0, "f64_value"},
                                        {"uint16", 0, "u16_value"},
                                        {"uint64", 0, "u64_value"},
                                        {"float", 0, "f32_value"},
                                        {"int32", 0, "i32_value"}},
                    gd::table::tag_type_name{});
   const auto prepared = table.prepare();
   if(!prepared.first) std::abort();

   for(std::size_t row = 0; row < rows; ++row)
   {
      const auto value = (static_cast<std::uint64_t>(row) * 48271U) % rows;
      const auto centered = static_cast<std::int64_t>(value) - 5'000'000;
      table.row_add({gd::variant_view(static_cast<std::uint8_t>(value % 251U)),
                     gd::variant_view(static_cast<double>(value) * 0.5 - 2'500'000.0),
                     gd::variant_view(static_cast<std::uint16_t>(value % 65521U)),
                     gd::variant_view(value * 1000U),
                     gd::variant_view(static_cast<float>(centered)),
                     gd::variant_view(static_cast<std::int32_t>(centered))});
   }

   return table;
}

template<typename Type>
Type ReadCell(const gd::table::table_column_buffer& table, std::uint64_t row, unsigned column)
{
   Type value;
   std::memcpy(&value, table.cell_get(row, column), sizeof(value));
   return value;
}

template<typename Type>
NumericSummary SummarizeColumn(const gd::table::table_column_buffer& table, unsigned column)
{
   std::vector<Type> values;
   values.reserve(table.get_row_count());

   Type minimum = std::numeric_limits<Type>::max();
   Type maximum = std::numeric_limits<Type>::lowest();
   __int128 integer_sum = 0;
   long double floating_sum = 0;
   for(std::uint64_t row = 0; row < table.get_row_count(); ++row)
   {
      const auto value = ReadCell<Type>(table, row, column);
      minimum = std::min(minimum, value);
      maximum = std::max(maximum, value);
      if constexpr(std::is_integral_v<Type>)
         integer_sum += static_cast<__int128>(value);
      else
         floating_sum += static_cast<long double>(value);
      values.push_back(value);
   }

   const auto middle = values.begin() + static_cast<std::ptrdiff_t>(values.size() / 2);
   std::nth_element(values.begin(), middle, values.end());
   const auto upper_middle = *middle;
   const auto lower_middle = *std::max_element(values.begin(), middle);
   const auto sum = std::is_integral_v<Type> ? static_cast<long double>(integer_sum) : floating_sum;
   return {sum / static_cast<long double>(values.size()),
           static_cast<long double>(minimum),
           static_cast<long double>(maximum),
           (static_cast<long double>(lower_middle) + static_cast<long double>(upper_middle)) / 2};
}

template<typename Type>
long double AverageColumn(const gd::table::table_column_buffer& table, unsigned column)
{
   if constexpr(std::is_integral_v<Type>)
   {
      using Sum = std::conditional_t<std::is_signed_v<Type>, std::int64_t, std::uint64_t>;
      Sum sum = 0;
      for(std::uint64_t row = 0; row < table.get_row_count(); ++row)
         sum += static_cast<Sum>(ReadCell<Type>(table, row, column));
      return static_cast<long double>(sum) / static_cast<long double>(table.get_row_count());
   }
   else
   {
      long double sum = 0;
      for(std::uint64_t row = 0; row < table.get_row_count(); ++row)
         sum += static_cast<long double>(ReadCell<Type>(table, row, column));
      return sum / static_cast<long double>(table.get_row_count());
   }
}

template<typename Type>
Type MaximumColumn(const gd::table::table_column_buffer& table, unsigned column)
{
   Type maximum = std::numeric_limits<Type>::lowest();
   for(std::uint64_t row = 0; row < table.get_row_count(); ++row)
      maximum = std::max(maximum, ReadCell<Type>(table, row, column));
   return maximum;
}

template<typename Type>
Type MaximumContiguous(const std::vector<Type>& values)
{
   Type maximum = std::numeric_limits<Type>::lowest();
   for(const auto value : values)
      maximum = std::max(maximum, value);
   return maximum;
}

template<typename Type>
long double MedianColumn(const gd::table::table_column_buffer& table, unsigned column)
{
   std::vector<Type> values;
   values.reserve(table.get_row_count());
   for(std::uint64_t row = 0; row < table.get_row_count(); ++row)
      values.push_back(ReadCell<Type>(table, row, column));

   const auto middle = values.begin() + static_cast<std::ptrdiff_t>(values.size() / 2);
   std::nth_element(values.begin(), middle, values.end());
   const auto upper_middle = *middle;
   const auto lower_middle = *std::max_element(values.begin(), middle);
   return (static_cast<long double>(lower_middle) + static_cast<long double>(upper_middle)) / 2;
}

std::array<NumericSummary, 6> SummarizeMixedNumericTable(
   const gd::table::table_column_buffer& table)
{
   return {SummarizeColumn<std::uint8_t>(table, 0),
           SummarizeColumn<double>(table, 1),
           SummarizeColumn<std::uint16_t>(table, 2),
           SummarizeColumn<std::uint64_t>(table, 3),
           SummarizeColumn<float>(table, 4),
           SummarizeColumn<std::int32_t>(table, 5)};
}

void VerifyMixedNumericSummary(const std::array<NumericSummary, 6>& summaries)
{
   for(std::size_t index = 0; index < summaries.size(); ++index)
   {
      if(!Close(summaries[index].average, kExpectedMixedNumeric[index].average) ||
         !Close(summaries[index].minimum, kExpectedMixedNumeric[index].minimum) ||
         !Close(summaries[index].maximum, kExpectedMixedNumeric[index].maximum) ||
         !Close(summaries[index].median, kExpectedMixedNumeric[index].median))
         std::abort();
   }
}

void AppendRows(benchmark::State& state)
{
   const auto rows = static_cast<std::size_t>(state.range(0));
   for(auto _ : state)
   {
      auto table = MakeTable(rows);
      benchmark::DoNotOptimize(table);
   }
   state.SetItemsProcessed(state.iterations() * state.range(0));
}
BENCHMARK(AppendRows)->RangeMultiplier(10)->Range(10, 10000);

void AppendRowsPrepared(benchmark::State& state)
{
   for(auto _ : state)
   {
      auto table = MakeTablePrepared(10000);
      benchmark::DoNotOptimize(table);
   }
   state.SetItemsProcessed(state.iterations() * 10000);
}
BENCHMARK(AppendRowsPrepared);

void ColumnScan(benchmark::State& state)
{
   const auto table = MakeTable(100000);
   for(auto _ : state)
   {
      std::int64_t sum = 0;
      for(std::uint64_t row = 0; row < table.get_row_count(); ++row)
      {
         sum += table.cell_get_variant_view(row, 2U).as_int64();
      }
      benchmark::DoNotOptimize(sum);
   }
   state.SetItemsProcessed(state.iterations() * 100000);
}
BENCHMARK(ColumnScan);

void NamedCellScan(benchmark::State& state)
{
   const auto table = MakeTable(100000);
   for(auto _ : state)
   {
      std::int64_t sum = 0;
      for(std::uint64_t row = 0; row < table.get_row_count(); ++row)
      {
         sum += table.cell_get_variant_view(row, "value").as_int64();
      }
      benchmark::DoNotOptimize(sum);
   }
   state.SetItemsProcessed(state.iterations() * 100000);
}
BENCHMARK(NamedCellScan);

void OpenSchemaAppendTwoFields(benchmark::State& state)
{
   const auto rows = static_cast<std::size_t>(state.range(0));
   for(auto _ : state)
   {
      auto table = MakeOpenTable(rows);
      benchmark::DoNotOptimize(table);
   }
   state.SetItemsProcessed(state.iterations() * state.range(0));
}
BENCHMARK(OpenSchemaAppendTwoFields)->Arg(100)->Arg(1000)->Arg(10000);

void OpenSchemaLookupTwoFields(benchmark::State& state)
{
   const auto rows = static_cast<std::size_t>(state.range(0));
   const auto table = MakeOpenTable(rows);
   for(auto _ : state)
   {
      std::size_t total_length = 0;
      for(std::uint64_t row = 0; row < table.get_row_count(); ++row)
      {
         total_length += table.cell_get_variant_view(row, "custom_category").as_string_view().size();
         total_length += table.cell_get_variant_view(row, "custom_region").as_string_view().size();
      }
      benchmark::DoNotOptimize(total_length);
   }
   state.SetItemsProcessed(state.iterations() * state.range(0));
}
BENCHMARK(OpenSchemaLookupTwoFields)->Arg(100)->Arg(1000)->Arg(10000)->Arg(100000);

void OpenSchemaWideBuildSet(benchmark::State& state)
{
   const auto names = MakeWideFieldNames();
   for(auto _ : state)
   {
      auto table = MakeWideOpenTable(names, false);
      benchmark::DoNotOptimize(table);
   }
   state.SetItemsProcessed(state.iterations() * 1000 * 1000);
}
BENCHMARK(OpenSchemaWideBuildSet);

void OpenSchemaWideBuildAdd(benchmark::State& state)
{
   const auto names = MakeWideFieldNames();
   for(auto _ : state)
   {
      auto table = MakeWideOpenTable(names, true);
      benchmark::DoNotOptimize(table);
   }
   state.SetItemsProcessed(state.iterations() * 1000 * 1000);
}
BENCHMARK(OpenSchemaWideBuildAdd);

void OpenSchemaWideLookupAll(benchmark::State& state)
{
   const auto names = MakeWideFieldNames();
   const auto table = MakeWideOpenTable(names, true);
   for(auto _ : state)
   {
      std::uint64_t sum = 0;
      for(std::uint64_t row = 0; row < table.get_row_count(); ++row)
      {
         for(const auto& name : names)
            sum += table.cell_get_variant_view(row, name).as_uint64();
      }
      benchmark::DoNotOptimize(sum);
   }
   state.SetItemsProcessed(state.iterations() * 1000 * 1000);
}
BENCHMARK(OpenSchemaWideLookupAll);

void MixedNumericBuild10M(benchmark::State& state)
{
   for(auto _ : state)
   {
      auto table = MakeMixedNumericTable(kMixedNumericRows);
      if(table.size_reserved_total() != kMixedNumericRows * 32U) std::abort();
      benchmark::DoNotOptimize(table);
   }
   state.SetItemsProcessed(state.iterations() * kMixedNumericRows);
   state.counters["table_mib"] =
      benchmark::Counter(static_cast<double>(kMixedNumericRows * 32U) / (1024.0 * 1024.0));
}
BENCHMARK(MixedNumericBuild10M)->Unit(benchmark::kMillisecond);

template<typename Type, unsigned ColumnIndex>
void MixedNumericAverage10M(benchmark::State& state)
{
   const auto table = MakeMixedNumericTable(kMixedNumericRows);
   if(!Close(AverageColumn<Type>(table, ColumnIndex),
             kExpectedMixedNumeric[ColumnIndex].average))
      std::abort();
   for(auto _ : state)
   {
      auto average = AverageColumn<Type>(table, ColumnIndex);
      benchmark::DoNotOptimize(average);
   }
   state.SetItemsProcessed(state.iterations() * kMixedNumericRows);
}

template<typename Type, unsigned ColumnIndex>
void MixedNumericMaximum10M(benchmark::State& state)
{
   const auto table = MakeMixedNumericTable(kMixedNumericRows);
   if(!Close(static_cast<long double>(MaximumColumn<Type>(table, ColumnIndex)),
             kExpectedMixedNumeric[ColumnIndex].maximum))
      std::abort();
   for(auto _ : state)
   {
      auto maximum = MaximumColumn<Type>(table, ColumnIndex);
      benchmark::DoNotOptimize(maximum);
   }
   state.SetItemsProcessed(state.iterations() * kMixedNumericRows);
}

template<typename Type, unsigned ColumnIndex>
void MixedNumericMedian10M(benchmark::State& state)
{
   const auto table = MakeMixedNumericTable(kMixedNumericRows);
   if(!Close(MedianColumn<Type>(table, ColumnIndex),
             kExpectedMixedNumeric[ColumnIndex].median))
      std::abort();
   for(auto _ : state)
   {
      auto median = MedianColumn<Type>(table, ColumnIndex);
      benchmark::DoNotOptimize(median);
   }
   state.SetItemsProcessed(state.iterations() * kMixedNumericRows);
}

#define GD_REGISTER_MIXED_NUMERIC_COLUMN(Type, ColumnIndex, Label)                                \
   BENCHMARK_TEMPLATE(MixedNumericAverage10M, Type, ColumnIndex)                                  \
      ->Name("MixedNumeric/Average/" Label)                                                       \
      ->Unit(benchmark::kMillisecond);                                                            \
   BENCHMARK_TEMPLATE(MixedNumericMaximum10M, Type, ColumnIndex)                                  \
      ->Name("MixedNumeric/Maximum/" Label)                                                       \
      ->Unit(benchmark::kMillisecond);                                                            \
   BENCHMARK_TEMPLATE(MixedNumericMedian10M, Type, ColumnIndex)                                   \
      ->Name("MixedNumeric/Median/" Label)                                                        \
      ->Unit(benchmark::kMillisecond)

GD_REGISTER_MIXED_NUMERIC_COLUMN(std::uint8_t, 0, "u8");
GD_REGISTER_MIXED_NUMERIC_COLUMN(double, 1, "f64");
GD_REGISTER_MIXED_NUMERIC_COLUMN(std::uint16_t, 2, "u16");
GD_REGISTER_MIXED_NUMERIC_COLUMN(std::uint64_t, 3, "u64");
GD_REGISTER_MIXED_NUMERIC_COLUMN(float, 4, "f32");
GD_REGISTER_MIXED_NUMERIC_COLUMN(std::int32_t, 5, "i32");

#undef GD_REGISTER_MIXED_NUMERIC_COLUMN

// Report the materialization and reuse phases separately. There is also a correctness
// blocker in this exact fixture: harvest calls cell_get_variant_view, whose fixed
// 8-byte path does *(uint64_t*)puRowValue. The f64 starts at offset 4, so that is an
// unaligned typed dereference and C++ undefined behavior. Keep this experiment on the
// safely aligned u8 column until that path performs a source-level-defined load.
void MixedNumericHarvestCostU8_10M(benchmark::State& state)
{
   const auto table = MakeMixedNumericTable(kMixedNumericRows);
   for(auto _ : state)
   {
      auto values = table.harvest<std::uint8_t>(0);
      benchmark::DoNotOptimize(values);
   }
   state.SetItemsProcessed(state.iterations() * kMixedNumericRows);
}
BENCHMARK(MixedNumericHarvestCostU8_10M)
   ->Name("MixedNumeric/HarvestCost/u8")
   ->Unit(benchmark::kMillisecond);

void MixedNumericMaximumReusedHarvestU8_10M(benchmark::State& state)
{
   const auto table = MakeMixedNumericTable(kMixedNumericRows);
   const auto values = table.harvest<std::uint8_t>(0);
   if(MaximumContiguous(values) != static_cast<std::uint8_t>(250)) std::abort();

   for(auto _ : state)
   {
      benchmark::DoNotOptimize(values.data());
      auto maximum = MaximumContiguous(values);
      benchmark::DoNotOptimize(maximum);
   }
   state.SetItemsProcessed(state.iterations() * kMixedNumericRows);
}
BENCHMARK(MixedNumericMaximumReusedHarvestU8_10M)
   ->Name("MixedNumeric/MaximumReusedHarvest/u8")
   ->Unit(benchmark::kMillisecond);

void RowSortSelection(benchmark::State& state)
{
   const auto rows = static_cast<std::size_t>(state.range(0));
   for(auto _ : state)
   {
      state.PauseTiming();
      auto table = MakeSortTable(rows);
      state.ResumeTiming();
      table.sort(0, true, gd::table::tag_sort_selection{});
      benchmark::DoNotOptimize(table);
   }
   state.SetItemsProcessed(state.iterations() * state.range(0));
   state.SetComplexityN(state.range(0));
}
BENCHMARK(RowSortSelection)->Arg(100)->Arg(1000)->Arg(5000)->Complexity();

void FormatJson(benchmark::State& state)
{
   const auto rows = static_cast<std::size_t>(state.range(0));
   const auto table = MakeTablePrepared(rows);
   for(auto _ : state)
   {
      std::string output;
      gd::table::to_string(table,
                           0,
                           table.get_row_count(),
                           {},
                           nullptr,
                           output,
                           gd::table::tag_io_json{},
                           gd::table::tag_io_name{});
      benchmark::DoNotOptimize(output);
   }
   state.SetItemsProcessed(state.iterations() * state.range(0));
}
BENCHMARK(FormatJson)->Arg(100)->Arg(1000)->Arg(10000);

void FormatCsv(benchmark::State& state)
{
   const auto rows = static_cast<std::size_t>(state.range(0));
   const auto table = MakeTablePrepared(rows);
   for(auto _ : state)
      benchmark::DoNotOptimize(
         gd::table::to_string(table, gd::table::tag_io_header{}, gd::table::tag_io_csv{}));
   state.SetItemsProcessed(state.iterations() * state.range(0));
}
BENCHMARK(FormatCsv)->Arg(100)->Arg(1000)->Arg(10000);

} // namespace
