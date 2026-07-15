#include <benchmark/benchmark.h>

#include <cstdint>
#include <string>
#include <vector>

#include "gd_binary.h"

namespace {

void HexEncode(benchmark::State& state)
{
   const auto size = static_cast<std::size_t>(state.range(0));
   const std::vector<std::uint8_t> bytes(size, 0xab);
   for(auto _ : state)
   {
      benchmark::DoNotOptimize(gd::binary_to_hex_g(bytes.data(), bytes.size()));
   }
   state.SetBytesProcessed(state.iterations() * state.range(0));
}
BENCHMARK(HexEncode)->RangeMultiplier(16)->Range(16, 65536);

void HexDecode(benchmark::State& state)
{
   const auto size = static_cast<std::size_t>(state.range(0));
   const std::vector<std::uint8_t> source(size, 0xab);
   const std::string encoded = gd::binary_to_hex_g(source.data(), source.size());
   for(auto _ : state)
   {
      std::vector<std::uint8_t> bytes(size);
      gd::binary_copy_hex_g(bytes.data(), encoded);
      benchmark::DoNotOptimize(bytes);
   }
   state.SetBytesProcessed(state.iterations() * state.range(0));
}
BENCHMARK(HexDecode)->RangeMultiplier(16)->Range(16, 65536);

void WriteU64BE(benchmark::State& state)
{
   std::vector<std::uint8_t> bytes(4096 * sizeof(std::uint64_t));
   for(auto _ : state)
   {
      gd::binary::write_be writer(bytes);
      for(std::uint64_t value = 0; value < 4096; ++value) writer << value;
      benchmark::DoNotOptimize(writer.position());
   }
   state.SetItemsProcessed(state.iterations() * 4096);
}
BENCHMARK(WriteU64BE);

void ReadU64BE(benchmark::State& state)
{
   std::vector<std::uint8_t> bytes(4096 * sizeof(std::uint64_t));
   gd::binary::write_be writer(bytes);
   for(std::uint64_t value = 0; value < 4096; ++value) writer << value;
   for(auto _ : state)
   {
      gd::binary::read_be reader(bytes);
      std::uint64_t sum = 0;
      while(!reader.eof()) sum += reader.read<std::uint64_t>();
      benchmark::DoNotOptimize(sum);
   }
   state.SetItemsProcessed(state.iterations() * 4096);
}
BENCHMARK(ReadU64BE);

void FindLast(benchmark::State& state)
{
   std::vector<std::uint8_t> bytes(65536, static_cast<std::uint8_t>('a'));
   constexpr std::string_view needle = "needle";
   bytes.insert(bytes.end(), needle.begin(), needle.end());
   for(auto _ : state)
   {
      benchmark::DoNotOptimize(gd::buffer_find_g(bytes.data(),
                                                  bytes.size(),
                                                  reinterpret_cast<const std::uint8_t*>(needle.data()),
                                                  needle.size()));
   }
   state.SetBytesProcessed(state.iterations() * static_cast<std::int64_t>(bytes.size()));
}
BENCHMARK(FindLast);

} // namespace
