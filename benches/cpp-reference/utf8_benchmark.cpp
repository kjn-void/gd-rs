#include <benchmark/benchmark.h>

#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

#include "gd_utf8.h"

namespace {

std::string MakeFixture(std::size_t target)
{
   constexpr std::string_view unit = "alpha & <beta> caf\xC3\xA9 \xF0\x9F\x98\x80 / path? value=42\n";
   std::string text;
   text.reserve(target + unit.size());
   while(text.size() < target) text.append(unit);
   while(target > 0 &&
         (static_cast<unsigned char>(text[target]) & 0xC0U) == 0x80U)
      --target;
   text.resize(target);
   return text;
}

void JsonEncode(benchmark::State& state)
{
   const auto size = static_cast<std::size_t>(state.range(0));
   const std::string text = MakeFixture(size);
   for(auto _ : state)
   {
      std::string literal{"\""};
      gd::utf8::json::convert_utf8_to_json(text, literal);
      literal.push_back('"');
      benchmark::DoNotOptimize(literal);
   }
   state.SetBytesProcessed(state.iterations() * static_cast<std::int64_t>(text.size()));
}
BENCHMARK(JsonEncode)->RangeMultiplier(64)->Range(64, 65536);

void UriEncode(benchmark::State& state)
{
   const auto size = static_cast<std::size_t>(state.range(0));
   const std::string text = MakeFixture(size);
   for(auto _ : state)
   {
      std::string encoded;
      gd::utf8::uri::convert_utf8_to_uri(text, encoded);
      benchmark::DoNotOptimize(encoded);
   }
   state.SetBytesProcessed(state.iterations() * static_cast<std::int64_t>(text.size()));
}
BENCHMARK(UriEncode)->RangeMultiplier(64)->Range(64, 65536);

void UriDecode(benchmark::State& state)
{
   const auto size = static_cast<std::size_t>(state.range(0));
   const std::string text = MakeFixture(size);
   std::string encoded;
   gd::utf8::uri::convert_utf8_to_uri(text, encoded);
   std::vector<std::uint8_t> decoded(text.size());
   for(auto _ : state)
   {
      const auto result = gd::utf8::uri::convert_uri_to_uf8(
         reinterpret_cast<const std::uint8_t*>(encoded.data()),
         reinterpret_cast<const std::uint8_t*>(encoded.data() + encoded.size()),
         decoded.data());
      auto produced = result.second - decoded.data();
      benchmark::DoNotOptimize(produced);
   }
   state.SetBytesProcessed(state.iterations() * static_cast<std::int64_t>(text.size()));
}
BENCHMARK(UriDecode)->RangeMultiplier(64)->Range(64, 65536);

void XmlEscape(benchmark::State& state)
{
   const auto size = static_cast<std::size_t>(state.range(0));
   const std::string text = MakeFixture(size);
   for(auto _ : state)
      benchmark::DoNotOptimize(gd::utf8::xml::convert_utf8_to_xml(text));
   state.SetBytesProcessed(state.iterations() * static_cast<std::int64_t>(text.size()));
}
BENCHMARK(XmlEscape)->RangeMultiplier(64)->Range(64, 65536);

} // namespace
