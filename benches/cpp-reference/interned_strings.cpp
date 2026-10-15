// Matched counterpart to benches/interned_strings/workload.rs.
#include <chrono>
#include <cstdint>
#include <iomanip>
#include <iostream>
#include <optional>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>
#include "interned_strings_memory.h"
#include "gd_table_table.h"

using Table = gd::table::table;
using Clock = std::chrono::steady_clock;

static void require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
}

static std::vector<std::string> strings(std::size_t count, std::size_t length, char prefix) {
    std::vector<std::string> result;
    result.reserve(count);
    for (std::size_t i = 0; i < count; ++i) {
        std::ostringstream text;
        text << prefix << std::setw(8) << std::setfill('0') << i;
        auto value = text.str();
        value.append(length - value.size(), 'x');
        result.push_back(std::move(value));
    }
    return result;
}

static Table insert(gd::table::detail::columns* columns, unsigned rows,
                    const std::vector<std::string>& input) {
    Table result(columns, rows, 0u, 0u);
    for (unsigned row = 0; row < rows; ++row)
        result.row_add({gd::variant_view(std::string_view(input[row % input.size()]))});
    return result;
}

static Table copy(const Table& source) {
    Table result(source);
    // Pinned GD's copy constructor copies the schema pointer without acquiring
    // its reference. Balance ownership explicitly so both tables can be safely
    // destroyed. Cells and reference payloads are copied by the native constructor.
    // This is the only adapter; no upstream source is modified.
    result.get_columns()->add_reference();
    return result;
}

static void mutate(Table& table, const std::vector<std::string>& input, bool rotate) {
    for (std::uint64_t row = 0; row < table.get_row_count(); ++row) {
        const auto index = rotate ? (row + 1) % input.size() : row;
        table.cell_set(row, 0u, gd::variant_view(std::string_view(input[index])));
    }
}

static std::uint64_t digest(const Table& table) {
    std::uint64_t hash = 0xcbf29ce484222325;
    for (std::uint64_t row = 0; row < table.get_row_count(); ++row) {
        auto value = table.cell_get_variant_view(row, 0u).as_string_view();
        for (unsigned char byte : value) hash = (hash ^ byte) * 0x100000001b3;
        hash *= 0x100000001b3; // zero byte row separator
    }
    return hash;
}

int main(int argc, char** argv) {
    try {
        require(argc == 8, "implementation mode operation rows unique length samples");
        const std::string implementation = argv[1], mode = argv[2], operation = argv[3];
        const auto rows = static_cast<unsigned>(std::stoul(argv[4]));
        const auto unique = static_cast<unsigned>(std::stoul(argv[5]));
        const auto length = static_cast<unsigned>(std::stoul(argv[6]));
        const auto samples = static_cast<unsigned>(std::stoul(argv[7]));
        require(implementation == "gd", "unknown implementation");
        require(rows && unique && unique <= rows && length >= 9 && samples, "invalid dimensions");
        require(mode == "timing" || mode == "memory" || mode == "verify", "unknown mode");
        require(operation == "insert" || operation == "copy" || operation == "rotate" || operation == "replace", "unknown operation");
        Table schema(0u, 0u);
        schema.column_prepare();
        schema.column_add("rstring", "text");
        const auto [ok, error] = schema.prepare();
        require(ok, error.c_str());
        const auto input = strings(unique, length, 'a');
        const auto replacement = strings(rows, length, 'b');
        const auto baseline = live_bytes();
        const auto source = insert(schema.get_columns(), rows, input);
        const auto source_bytes = live_bytes() - baseline;
        const auto expected_source = digest(source);
        {
            const auto lifetime_copy = [&source]() {
                const auto temporary = copy(source);
                return copy(temporary);
            }();
            require(digest(lifetime_copy) == expected_source, "copy lost source lifetime");
        }
        std::vector<std::uint64_t> times;
        times.reserve(samples);
        std::uint64_t last_digest = 0;
        std::size_t result_bytes = 0;
        for (unsigned sample = 0; sample < samples; ++sample) {
            const auto result_baseline = live_bytes();
            std::optional<Table> result;
            if (operation == "rotate" || operation == "replace")
                result.emplace(insert(schema.get_columns(), rows, input));
            const auto start = Clock::now();
            if (operation == "insert") result.emplace(insert(schema.get_columns(), rows, input));
            else if (operation == "copy") result.emplace(copy(source));
            else mutate(*result, operation == "rotate" ? input : replacement, operation == "rotate");
            const auto nanos = std::chrono::duration_cast<std::chrono::nanoseconds>(Clock::now() - start).count();
            result_bytes = live_bytes() - result_baseline;
            last_digest = digest(*result);
            if (mode == "verify") result->cell_set(0u, 0u, gd::variant_view(std::string_view(replacement[0])));
            require(digest(source) == expected_source, "operation changed source");
            times.push_back(nanos);
        }
        std::cout << "{\"digest\":" << last_digest << ",\"source_digest\":" << expected_source
                  << ",\"source_bytes\":" << source_bytes << ",\"result_bytes\":" << result_bytes
                  << ",\"nanos\":[";
        for (unsigned i = 0; i < times.size(); ++i) std::cout << (i ? "," : "") << times[i];
        std::cout << "]}\n";
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
