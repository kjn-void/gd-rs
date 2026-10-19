#include <benchmark/benchmark.h>

#include <array>
#include <cstddef>
#include <string>
#include <string_view>

#include "gd_string_transport.h"

namespace {

constexpr std::string_view kExpected =
    "0123456789! 0123456789! 0123456789! 0123456789! 0123456789! 0123456789!";

// One assignment and five appends; the 71-character result fits a 150-byte buffer.
template <typename STRING>
void Fill(STRING& stringText) {
    stringText = "0123456789!";
    stringText += " 0123456789!";
    stringText += " 0123456789!";
    stringText += " 0123456789!";
    stringText += " 0123456789!";
    stringText += " 0123456789!";
}

// Hide the string's state before filling, like Rust's black_box(&mut value), so the
// compiler cannot precompute the constant result, then keep the result observable.
template <typename STRING>
void FillObserved(STRING& stringText) {
    benchmark::DoNotOptimize(stringText);
    Fill(stringText);
    benchmark::DoNotOptimize(stringText);
    benchmark::ClobberMemory();
}

void Buffer(benchmark::State& state) {
    std::array<std::byte, 150> arrayBuffer;
    {
        gd::string_transport stringText(arrayBuffer);
        Fill(stringText);
        if (!stringText.is_borrowed() || std::string_view(stringText.c_str()) != kExpected) {
            state.SkipWithError("borrowed string_transport content mismatch");
            return;
        }
    }

    for (auto _ : state) {
        gd::string_transport stringText(arrayBuffer);
        FillObserved(stringText);
    }
}

BENCHMARK(Buffer)->Name("StringTransport/Buffer");

void NoBuffer(benchmark::State& state) {
    {
        gd::string_transport stringText;
        Fill(stringText);
        if (!stringText.is_owned() || std::string_view(stringText.c_str()) != kExpected) {
            state.SkipWithError("owned string_transport content mismatch");
            return;
        }
    }

    for (auto _ : state) {
        gd::string_transport stringText;
        FillObserved(stringText);
    }
}

BENCHMARK(NoBuffer)->Name("StringTransport/NoBuffer");

void StdString(benchmark::State& state) {
    {
        std::string stringText;
        Fill(stringText);
        if (stringText != kExpected) {
            state.SkipWithError("std::string content mismatch");
            return;
        }
    }

    for (auto _ : state) {
        std::string stringText;
        FillObserved(stringText);
    }
}

BENCHMARK(StdString)->Name("StringTransport/StdString");

} // namespace
