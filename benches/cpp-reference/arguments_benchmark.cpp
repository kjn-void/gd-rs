#include <benchmark/benchmark.h>

#include <cstdint>
#include <string>

#include "gd_arguments.h"
#include "gd_arguments_index.h"
#include "gd_arguments_io.h"

namespace {

gd::argument::arguments MakeArguments(std::size_t count)
{
   gd::argument::arguments values;
   for(std::size_t index = 0; index < count; ++index)
   {
      values.append("key-" + std::to_string(index), static_cast<std::uint64_t>(index));
   }
   return values;
}

gd::argument::arguments MakeUriArguments()
{
   gd::argument::arguments values;
   values.append("scheme", "https");
   values.append("host", "example.com");
   values.append("port", std::int32_t{443});
   values.append("path", "/api/users");
   values.append("query", "limit=10&offset=20");
   values.append("fragment", "section1");
   values.append("user", "admin");
   values.append("password", "secret123");
   values.append("secure", true);
   values.append("timeout", std::int32_t{5000});
   values.append("retry_count", std::int32_t{3});
   return values;
}

void AppendNamed(benchmark::State& state)
{
   const auto count = static_cast<std::size_t>(state.range(0));
   for(auto _ : state)
   {
      gd::argument::arguments values;
      for(std::size_t index = 0; index < count; ++index)
      {
         values.append("key-" + std::to_string(index), static_cast<std::uint64_t>(index));
      }
      benchmark::DoNotOptimize(values.buffer_data());
   }
   state.SetItemsProcessed(state.iterations() * state.range(0));
}
BENCHMARK(AppendNamed)->RangeMultiplier(4)->Range(1, 4096);

void LookupNamed(benchmark::State& state)
{
   const auto count = static_cast<std::size_t>(state.range(0));
   const auto values = MakeArguments(count);
   const std::string key = "key-" + std::to_string(count - 1);
   for(auto _ : state)
   {
      benchmark::DoNotOptimize(values.find(key));
   }
}
BENCHMARK(LookupNamed)->RangeMultiplier(4)->Range(1, 4096)->Complexity();

void LookupMissing(benchmark::State& state)
{
   const auto values = MakeArguments(static_cast<std::size_t>(state.range(0)));
   for(auto _ : state)
   {
      benchmark::DoNotOptimize(values.find("missing"));
   }
}
BENCHMARK(LookupMissing)->RangeMultiplier(4)->Range(1, 4096)->Complexity();

void ReadUriByName(benchmark::State& state)
{
   const auto values = MakeUriArguments();
   for(auto _ : state)
   {
      std::uint64_t checksum = 0;
      checksum += values["scheme"].as_string_view().size();
      checksum += values["host"].as_string_view().size();
      checksum += static_cast<std::uint64_t>(values["port"].as_int());
      checksum += values["path"].as_string_view().size();
      checksum += values["query"].as_string_view().size();
      checksum += values["fragment"].as_string_view().size();
      checksum += values["user"].as_string_view().size();
      checksum += values["password"].as_string_view().size();
      checksum += values["secure"].as_bool() ? 1U : 0U;
      checksum += static_cast<std::uint64_t>(values["timeout"].as_int());
      checksum += static_cast<std::uint64_t>(values["retry_count"].as_int());
      benchmark::DoNotOptimize(checksum);
   }
   state.counters["buffer_used_bytes"] = static_cast<double>(values.buffer_size());
}
BENCHMARK(ReadUriByName);

void ReadUriByIndex(benchmark::State& state)
{
   const auto values = MakeUriArguments();
   const gd::argument::arguments_index_t index(values);
   for(auto _ : state)
   {
      std::uint64_t checksum = 0;
      for(std::size_t slot = 0; slot < index.size(); ++slot)
      {
         const auto value = index.get_argument(values, slot);
         if(value.is_text()) checksum += value.as_string_view().size();
         else if(value.is_bool()) checksum += value.as_bool() ? 1U : 0U;
         else checksum += static_cast<std::uint64_t>(value.as_int64());
      }
      benchmark::DoNotOptimize(checksum);
   }
   state.counters["buffer_used_bytes"] = static_cast<double>(values.buffer_size());
   state.counters["index_slots"] = static_cast<double>(index.size());
}
BENCHMARK(ReadUriByIndex);

void BuildUriIndex(benchmark::State& state)
{
   const auto values = MakeUriArguments();
   for(auto _ : state)
   {
      gd::argument::arguments_index_t index(values);
      benchmark::DoNotOptimize(index);
   }
}
BENCHMARK(BuildUriIndex);

void FormatUriJson(benchmark::State& state)
{
   const auto values = MakeUriArguments();
   for(auto _ : state)
   {
      std::string json;
      std::string uri;
      gd::argument::to_string(values, json, gd::argument::tag_io_json{});
      gd::argument::to_string(values, uri, gd::argument::tag_io_uri{});
      benchmark::DoNotOptimize(json);
      benchmark::DoNotOptimize(uri);
   }
}
BENCHMARK(FormatUriJson);

} // namespace
