#include <benchmark/benchmark.h>

#include <cstdint>

#include "gd_table_index.h"

namespace {

gd::table::index_int64 MakeIndex(std::int64_t count)
{
   gd::table::index_int64 index(static_cast<std::size_t>(count));
   for(std::int64_t value = 0; value < count; ++value)
   {
      index.add(gd::variant_view{value * 2}, static_cast<std::uint64_t>(value));
   }
   index.sort();
   return index;
}

void BuildIntegerIndex(benchmark::State& state)
{
   for(auto _ : state)
   {
      auto index = MakeIndex(state.range(0));
      benchmark::DoNotOptimize(index);
   }
   state.SetItemsProcessed(state.iterations() * state.range(0));
}
BENCHMARK(BuildIntegerIndex)->RangeMultiplier(4)->Range(16, 1 << 20)->Complexity();

void FindIntegerIndexHit(benchmark::State& state)
{
   const auto index = MakeIndex(state.range(0));
   for(auto _ : state)
   {
      benchmark::DoNotOptimize(index.find((state.range(0) - 1) * 2));
   }
}
BENCHMARK(FindIntegerIndexHit)->RangeMultiplier(4)->Range(16, 1 << 20)->Complexity();

void FindIntegerIndexMiss(benchmark::State& state)
{
   const auto index = MakeIndex(state.range(0));
   for(auto _ : state)
   {
      benchmark::DoNotOptimize(index.find((state.range(0) - 1) * 2 - 1));
   }
}
BENCHMARK(FindIntegerIndexMiss)->RangeMultiplier(4)->Range(16, 1 << 20)->Complexity();

} // namespace
