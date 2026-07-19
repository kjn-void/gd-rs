#include <chrono>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <string_view>
#include <vector>

namespace {

constexpr std::size_t kRows = 500'000;
constexpr int kWarmups = 16;
constexpr int kTimingSamples = 9;
constexpr int kTimingIterations = 512;
constexpr int kPerfIterations = 4'096;

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

void RunIterations(Calculate calculate, int iterations, const std::vector<PriceRow>& data,
                   std::vector<double>& results)
{
   for(int iteration = 0; iteration < iterations; ++iteration)
   {
      calculate(data.data(), results.data(), data.size());
#if defined(__GNUC__) || defined(__clang__)
      asm volatile("" : : "g"(results.data()) : "memory");
#endif
   }
}

} // namespace

int main(int argc, char** argv)
{
   if(argc != 3)
   {
      std::fprintf(stderr,
                   "usage: price_total_500k_benchmark "
                   "{unrestricted|restricted} {timing|perf}\n");
      return 2;
   }

   const std::string_view calculation(argv[1]);
   const std::string_view mode(argv[2]);
   Calculate calculate = nullptr;
   if(calculation == "unrestricted") calculate = CalculateUnrestricted;
   if(calculation == "restricted") calculate = CalculateRestricted;
   if(calculate == nullptr || (mode != "timing" && mode != "perf")) return 2;

   std::vector<PriceRow> data(kRows);
   std::vector<double> results(kRows);
   for(std::size_t row = 0; row < kRows; ++row)
   {
      data[row] = PriceRow{1.0 + static_cast<double>(row % 10'000) * 0.01,
                           static_cast<double>(row % 26),
                           static_cast<std::uint32_t>(row % 100 + 1)};
   }

   RunIterations(calculate, kWarmups, data, results);
   if(mode == "timing")
   {
      for(int sample = 0; sample < kTimingSamples; ++sample)
      {
         const auto start = std::chrono::steady_clock::now();
         RunIterations(calculate, kTimingIterations, data, results);
         const auto stop = std::chrono::steady_clock::now();
         const double microseconds =
            std::chrono::duration<double, std::micro>(stop - start).count() /
            kTimingIterations;
         std::printf("%.6f\n", microseconds);
      }
   }
   else
   {
      RunIterations(calculate, kPerfIterations, data, results);
   }

   const double checksum =
      results[0] + results[1] + results[25] + results[9'999] + results.back();
   std::fprintf(stderr, "mode=cpp-%.*s checksum=%.17g\n",
                static_cast<int>(calculation.size()), calculation.data(), checksum);
}
