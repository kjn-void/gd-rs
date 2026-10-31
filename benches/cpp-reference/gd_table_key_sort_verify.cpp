// Verify the posted key sort against a stable model, including complete row
// bytes, null flags, row status, partial ranges, and buffer reuse for metadata.
#include "gd_table_key_sort.h"

#include <array>
#include <iostream>
#include <limits>
#include <numeric>
#include <stdexcept>
#include <string>
#include <vector>

namespace {
using table_key_sort::table;
struct Model {
    std::uint64_t c1;
    std::string c2;
    double c3, c4;
    std::int32_t c5;
    std::uint32_t c6;
    std::string c7;
};

void Require(bool condition, const char* message) {
    if (!condition) throw std::runtime_error(message);
}

bool Less(const Model& a, const Model& b, unsigned column) {
    switch (column) {
        case 0: return a.c1 < b.c1;
        case 1: return a.c2 < b.c2;
        case 2: return a.c3 < b.c3;
        case 3: return a.c4 < b.c4;
        case 4: return a.c5 < b.c5;
        case 5: return a.c6 < b.c6;
        default: return a.c7 < b.c7;
    }
}

std::vector<std::uint8_t> Snapshot(const table& rows, std::uint64_t row) {
    std::vector<std::uint8_t> bytes(rows.size_row() + rows.size_row_meta());
    std::memcpy(bytes.data(), rows.row_get(row), rows.size_row());
    if (rows.is_rowmeta())
        std::memcpy(bytes.data() + rows.size_row(), rows.row_get_meta(row), rows.size_row_meta());
    return bytes;
}

void Check(unsigned column, bool ascending, std::uint64_t count, bool partial, unsigned flags) {
    table rows(flags,
        {{"uint64", 0, "C1"}, {"string", 8, "C2"}, {"double", 0, "C3"},
         {"double", 0, "C4"}, {"int32", 0, "C5"}, {"uint32", 0, "C6"},
         {"string", 50, "C7"}}, gd::table::tag_prepare{});
    std::vector<Model> model;
    for (std::uint64_t row = 0; row < count; ++row) {
        const auto key = row % 5;
        model.push_back({key == 0 ? std::numeric_limits<std::uint64_t>::max() : key,
            std::string(key, 'a'), static_cast<double>(key) - 2.0, 2.0 - static_cast<double>(key),
            static_cast<std::int32_t>(key) - 2, static_cast<std::uint32_t>(key),
            key == 0 ? "" : std::string(20, 'a') + std::string(key, 'b')});
        const auto& m = model.back();
        rows.row_add_one();
        std::memset(rows.row_get(row), 0, rows.size_row());
        if (rows.is_rowmeta()) std::memset(rows.row_get_meta(row), 0, rows.size_row_meta());
        rows.cell_set(row, 0u, m.c1); rows.cell_set(row, 1u, std::string_view(m.c2));
        rows.cell_set(row, 2u, m.c3); rows.cell_set(row, 3u, m.c4);
        rows.cell_set(row, 4u, m.c5); rows.cell_set(row, 5u, m.c6);
        rows.cell_set(row, 6u, std::string_view(m.c7));
        if (flags != 0) {
            rows.row_set_state(row, 0xa5000000U | static_cast<std::uint32_t>(row));
            if (row % 3 == 0) rows.cell_set_null(row, (column + 1) % 7);
        }
    }
    std::vector<std::vector<std::uint8_t>> before;
    for (std::uint64_t row = 0; row < count; ++row) before.push_back(Snapshot(rows, row));
    std::vector<std::uint64_t> expected(count);
    std::iota(expected.begin(), expected.end(), std::uint64_t{0});
    const std::uint64_t first = partial && count > 2 ? 1 : 0;
    const std::uint64_t length = partial && count > 2 ? count - 2 : count;
    std::stable_sort(expected.begin() + first, expected.begin() + first + length,
        [&](auto a, auto b) { return ascending ? Less(model[a], model[b], column) : Less(model[b], model[a], column); });
    table_key_sort::Sort(rows, column, ascending, first, length);
    for (std::uint64_t row = 0; row < count; ++row)
        Require(Snapshot(rows, row) == before[expected[row]], "lost/split row, unstable tie, or changed range boundary");
}

void CheckLargeMetadata() {
    table rows(table::eTableFlagNull64 | table::eTableFlagRowStatus,
        {{"uint8", 0, "key"}}, gd::table::tag_prepare{});
    for (std::uint64_t row = 0; row < 32; ++row) {
        rows.row_add_one();
        std::memset(rows.row_get(row), 0, rows.size_row());
        if (rows.is_rowmeta()) std::memset(rows.row_get_meta(row), 0, rows.size_row_meta());
        rows.cell_set(row, 0u, static_cast<std::uint8_t>(31 - row));
        rows.row_set_state(row, static_cast<std::uint32_t>(row));
    }
    Require(rows.size_row_meta() > rows.size_row(), "metadata-larger-than-data fixture is ineffective");
    std::vector<std::vector<std::uint8_t>> before;
    for (std::uint64_t row = 0; row < 32; ++row) before.push_back(Snapshot(rows, row));
    table_key_sort::Sort(rows, 0);
    for (std::uint64_t row = 0; row < 32; ++row)
        Require(Snapshot(rows, row) == before[31 - row], "metadata buffer reuse failed");
}
}  // namespace

int main() {
    for (auto flags : {0u, static_cast<unsigned>(table::eTableFlagNull32 | table::eTableFlagRowStatus)})
        for (auto count : {0u, 1u, 2u, 17u, 128u})
            for (unsigned column = 0; column < 7; ++column)
                for (bool ascending : {false, true})
                    for (bool partial : {false, true}) Check(column, ascending, count, partial, flags);
    CheckLargeMetadata();
    std::cout << "key sort verification: ok\n";
}
