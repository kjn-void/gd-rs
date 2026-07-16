#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <limits>
#include <stdexcept>
#include <string_view>
#include <tuple>
#include <vector>

#include <omp.h>

#include "gd_table_column-buffer.h"

namespace {

using Column = std::tuple<std::string_view, unsigned, std::string_view>;

constexpr std::size_t kDefaultRows = 100'000'000;
constexpr std::uint32_t kDefaultMaxArg = 23;

#if defined(_MSC_VER)
__declspec(noinline)
#else
__attribute__((noinline))
#endif
std::uint32_t RecursiveFibonacci(std::uint32_t value)
{
   return value < 2 ? value : RecursiveFibonacci(value - 1) + RecursiveFibonacci(value - 2);
}

std::uint32_t ExpectedFibonacci(std::uint32_t value)
{
   std::uint32_t previous = 0;
   std::uint32_t current = 1;
   for(std::uint32_t index = 0; index < value; ++index)
   {
      const auto next = previous + current;
      previous = current;
      current = next;
   }
   return previous;
}

template<typename Type>
Type Setting(const char* name, Type defaultValue)
{
   const auto* text = std::getenv(name);
   if(text == nullptr) return defaultValue;
   char* end = nullptr;
   const auto parsed = std::strtoull(text, &end, 10);
   if(*text == '\0' || *end != '\0' || parsed > std::numeric_limits<Type>::max())
      throw std::runtime_error(std::string("invalid ") + name + "=" + text);
   return static_cast<Type>(parsed);
}

gd::table::table_column_buffer MakeTable(std::size_t rows, std::uint32_t maxArg)
{
   gd::table::table_column_buffer table(static_cast<unsigned>(rows));
   table.column_add(std::vector<Column>{{"uint32", 0, "arg"}, {"uint32", 0, "result"}},
                    gd::table::tag_type_name{});
   const auto prepared = table.prepare();
   if(!prepared.first) throw std::runtime_error(prepared.second);
   for(std::size_t row = 0; row < rows; ++row)
   {
      const auto arg = static_cast<std::uint32_t>(row % maxArg) + 1U;
      table.row_add({gd::variant_view(arg), gd::variant_view(std::uint32_t{0})});
   }
   return table;
}

} // namespace

int main()
{
   try
   {
      const auto rows = Setting<std::size_t>("GD_PAR_ROWS", kDefaultRows);
      const auto maxArg = Setting<std::uint32_t>("GD_PAR_MAX_ARG", kDefaultMaxArg);
      if(rows == 0) throw std::runtime_error("GD_PAR_ROWS must be greater than zero");
      if(maxArg == 0 || maxArg > 23)
         throw std::runtime_error("GD_PAR_MAX_ARG must be in 1..=23");
      if(rows > std::numeric_limits<unsigned>::max())
         throw std::runtime_error("GD_PAR_ROWS exceeds the C++ table row limit");

      const auto buildStarted = std::chrono::steady_clock::now();
      auto table = MakeTable(rows, maxArg);
      const auto buildSeconds = std::chrono::duration<double>(
         std::chrono::steady_clock::now() - buildStarted).count();

      const auto transformStarted = std::chrono::steady_clock::now();
#pragma omp parallel for schedule(dynamic, 4096)
      for(std::int64_t row = 0; row < static_cast<std::int64_t>(rows); ++row)
      {
         std::uint32_t arg;
         std::memcpy(&arg, table.cell_get(static_cast<std::uint64_t>(row), 0), sizeof(arg));
         const auto result = RecursiveFibonacci(arg);
         std::memcpy(table.cell_get(static_cast<std::uint64_t>(row), 1),
                     &result,
                     sizeof(result));
      }
      const auto transformSeconds = std::chrono::duration<double>(
         std::chrono::steady_clock::now() - transformStarted).count();

      bool valid = true;
#pragma omp parallel for schedule(dynamic, 4096) reduction(&& : valid)
      for(std::int64_t row = 0; row < static_cast<std::int64_t>(rows); ++row)
      {
         std::uint32_t arg;
         std::uint32_t result;
         std::memcpy(&arg, table.cell_get(static_cast<std::uint64_t>(row), 0), sizeof(arg));
         std::memcpy(&result, table.cell_get(static_cast<std::uint64_t>(row), 1), sizeof(result));
         valid = valid && result == ExpectedFibonacci(arg);
      }
      if(!valid) throw std::runtime_error("result validation failed");

      std::printf("implementation=C++/OpenMP\n");
      std::printf("rows=%zu\n", rows);
      std::printf("arg_range=1..=%u\n", maxArg);
      std::printf("threads=%d\n", omp_get_max_threads());
      std::printf("row_storage_bytes=%llu\n",
                  static_cast<unsigned long long>(table.size_reserved_total()));
      std::printf("build_seconds=%.6f\n", buildSeconds);
      std::printf("transform_seconds=%.6f\n", transformSeconds);
      std::printf("transform_rows_per_second=%.3f\n",
                  static_cast<double>(rows) / transformSeconds);
      std::printf("validation=ok\n");
      return EXIT_SUCCESS;
   }
   catch(const std::exception& error)
   {
      std::fprintf(stderr, "error: %s\n", error.what());
      return EXIT_FAILURE;
   }
}
