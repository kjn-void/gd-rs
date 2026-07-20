#include <chrono>
#include <charconv>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <string_view>
#include <vector>

namespace {

constexpr std::size_t kDefaultRows = 500'000;
constexpr std::size_t kMinimumRows = 2'000;
constexpr std::size_t kWarmupLogicalRows = 8'000'000;
constexpr std::size_t kTimingLogicalRows = 256'000'000;
constexpr std::size_t kPerfLogicalRows = 2'048'000'000;
constexpr int kTimingSamples = 9;

struct alignas(8) PriceRow
{
   double price;
   double tax;
   std::uint32_t quantity;
};

static_assert(sizeof(PriceRow) == 24);

#if defined(_MSC_VER)
#define GD_NOINLINE __declspec(noinline)
#define GD_RESTRICT __restrict
#else
#define GD_NOINLINE __attribute__((noinline))
#define GD_RESTRICT __restrict
#endif

GD_NOINLINE void CalculateUnrestricted(const PriceRow* data, double* results,
                                       std::size_t row_count)
{
   constexpr std::size_t kBlockSize = 16;
   const std::size_t block_count = row_count / kBlockSize;
   const std::size_t tail_start = block_count * kBlockSize;

   for(std::size_t block = 0; block < block_count; ++block)
   {
      const PriceRow* block_data = &data[block * kBlockSize];
      double* block_results = &results[block * kBlockSize];
#if defined(__clang__)
#pragma clang loop unroll_count(16)
#elif defined(__GNUC__)
#pragma GCC unroll 16
#endif
      for(std::size_t index = 0; index < kBlockSize; ++index)
      {
         block_results[index] =
            (block_data[index].price * block_data[index].quantity) + block_data[index].tax;
      }
   }

   for(std::size_t index = tail_start; index < row_count; ++index)
      results[index] = (data[index].price * data[index].quantity) + data[index].tax;
}

GD_NOINLINE void CalculateRestricted(const PriceRow* GD_RESTRICT data,
                                     double* GD_RESTRICT results,
                                     std::size_t row_count)
{
   constexpr std::size_t kBlockSize = 16;
   const std::size_t block_count = row_count / kBlockSize;
   const std::size_t tail_start = block_count * kBlockSize;

   for(std::size_t block = 0; block < block_count; ++block)
   {
      const PriceRow* block_data = &data[block * kBlockSize];
      double* block_results = &results[block * kBlockSize];
#if defined(__clang__)
#pragma clang loop unroll_count(16)
#elif defined(__GNUC__)
#pragma GCC unroll 16
#endif
      for(std::size_t index = 0; index < kBlockSize; ++index)
      {
         block_results[index] =
            (block_data[index].price * block_data[index].quantity) + block_data[index].tax;
      }
   }

   for(std::size_t index = tail_start; index < row_count; ++index)
      results[index] = (data[index].price * data[index].quantity) + data[index].tax;
}

using Calculate = void (*)(const PriceRow*, double*, std::size_t);

void RunIterations(Calculate calculate, std::size_t iterations,
                   const std::vector<PriceRow>& data,
                   std::vector<double>& results)
{
   for(std::size_t iteration = 0; iteration < iterations; ++iteration)
   {
      calculate(data.data(), results.data(), data.size());
#if defined(__GNUC__) || defined(__clang__)
      asm volatile("" : : "g"(results.data()) : "memory");
#endif
   }
}

std::size_t IterationsFor(std::size_t logical_rows, std::size_t rows)
{
   return (logical_rows / rows) + static_cast<std::size_t>((logical_rows % rows) != 0);
}

bool ParseRows(std::string_view text, std::size_t& rows)
{
   const char* first = text.data();
   const char* last = first + text.size();
   const auto result = std::from_chars(first, last, rows);
   return result.ec == std::errc{} && result.ptr == last && rows >= kMinimumRows;
}

} // namespace

int main(int argc, char** argv)
{
   if(argc < 3 || argc > 4)
   {
      std::fprintf(stderr,
                   "usage: price_total_500k_benchmark "
                   "{unrestricted|restricted} {timing|perf} [ROWS >= 2000]\n");
      return 2;
   }

   const std::string_view calculation(argv[1]);
   const std::string_view mode(argv[2]);
   Calculate calculate = nullptr;
   if(calculation == "unrestricted") calculate = CalculateUnrestricted;
   if(calculation == "restricted") calculate = CalculateRestricted;
   if(calculate == nullptr || (mode != "timing" && mode != "perf")) return 2;

   std::size_t rows = kDefaultRows;
   if(argc == 4 && !ParseRows(argv[3], rows)) return 2;

   std::vector<PriceRow> data(rows);
   std::vector<double> results(rows);
   for(std::size_t row = 0; row < rows; ++row)
   {
      data[row] = PriceRow{1.0 + static_cast<double>(row % 10'000) * 0.01,
                           static_cast<double>(row % 26),
                           static_cast<std::uint32_t>(row % 100 + 1)};
   }

   RunIterations(calculate, IterationsFor(kWarmupLogicalRows, rows), data, results);
   if(mode == "timing")
   {
      for(int sample = 0; sample < kTimingSamples; ++sample)
      {
         const auto start = std::chrono::steady_clock::now();
         const std::size_t iterations = IterationsFor(kTimingLogicalRows, rows);
         RunIterations(calculate, iterations, data, results);
         const auto stop = std::chrono::steady_clock::now();
         const double microseconds =
            std::chrono::duration<double, std::micro>(stop - start).count() /
            static_cast<double>(iterations);
         std::printf("%.6f\n", microseconds);
      }
   }
   else
   {
      RunIterations(calculate, IterationsFor(kPerfLogicalRows, rows), data, results);
   }

   const std::size_t checksum_row = rows >= 10'000 ? 9'999 : rows - 1;
   const double checksum =
      results[0] + results[1] + results[25] + results[checksum_row] + results.back();
   std::fprintf(stderr, "mode=cpp-%.*s rows=%zu checksum=%.17g\n",
                static_cast<int>(calculation.size()), calculation.data(), rows, checksum);
}
