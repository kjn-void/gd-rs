#include <benchmark/benchmark.h>

#include <cstdint>
#include <string>

#include "gd_variant.h"
#include "gd_variant_view.h"

namespace {

void ConstructInteger(benchmark::State& state)
{
   for(auto _ : state)
   {
      gd::variant value{std::int64_t{42}};
      benchmark::DoNotOptimize(value);
   }
}
BENCHMARK(ConstructInteger);

void ConstructString(benchmark::State& state)
{
   const std::string source(static_cast<std::size_t>(state.range(0)), 'x');
   for(auto _ : state)
   {
      gd::variant value{source};
      benchmark::DoNotOptimize(value);
   }
   state.SetBytesProcessed(state.iterations() * state.range(0));
}
BENCHMARK(ConstructString)->RangeMultiplier(8)->Range(8, 32 * 1024);

void BorrowString(benchmark::State& state)
{
   const std::string source(static_cast<std::size_t>(state.range(0)), 'x');
   for(auto _ : state)
   {
      gd::variant_view value{source};
      benchmark::DoNotOptimize(value);
   }
   state.SetBytesProcessed(state.iterations() * state.range(0));
}
BENCHMARK(BorrowString)->RangeMultiplier(8)->Range(8, 32 * 1024);

} // namespace
