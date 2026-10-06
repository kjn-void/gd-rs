#pragma once
// Application stages shared by the AoS and AoSoA order-workflow benchmarks.
// Load imports the three business tables; Prepare joins and validates them;

// Variant produces one parameterized subset of the clean table. driver.cpp
// defines the timers, and pool.hpp schedules the eight Variant calls.
// The upstream GD checkout is never modified.
#include <array>
#include <cstdint>
#include <limits>
#include <optional>
#include <stdexcept>
#include <string>
#include <string_view>
#include <tuple>
#include <vector>
#include "gd_database_sqlite.h"
#include "gd_table_column-buffer.h"
#include "gd_table_index.h"
#ifdef GD_WORKFLOW_SIMD
#include "simd_table.hpp"
#endif

namespace workflow {
#ifdef GD_WORKFLOW_SIMD
using Table = SimdTable;
inline constexpr std::string_view implementation = "cpp_simd";
#else
using Table = gd::table::table_column_buffer;
inline constexpr std::string_view implementation = "cpp";
#endif
using Database = gd::database::sqlite::database;
using View = gd::variant_view;
using Parameters = std::array<std::int64_t, 6>;
using Position = std::optional<std::uint64_t>;

// Audit and clean share this schema. Variants project the six positions below:
// line_id, name, region, day, status, and amount_cents.
inline const std::vector<std::string_view> auditNames{"line_id", "order_id", "customer_id", "name",
    "region", "day", "status", "quantity", "unit_price", "amount_cents", "errors"};
inline const std::vector<unsigned> variantColumns{0, 3, 4, 5, 6, 9};

struct Inputs {
    Table customers, orders, lines;
};

struct Prepared {
    Table audit, clean;
};

inline void Check(const std::pair<bool, std::string>& result) {
    if (!result.first) {
        throw std::runtime_error(result.second);
    }
}

inline std::int64_t Multiply(std::int64_t a, std::int64_t b) {
    if (a < 0 || b < 0 || (b != 0 && a > std::numeric_limits<std::int64_t>::max() / b)) {
        throw std::overflow_error("amount multiplication overflow");
    }

    return a * b;
}

// Materialize a nullable native table for an input, intermediate, or output.
// The name column stores owned reference strings; other fields are signed
// 64-bit integers.
// Schema creation and allocation are timed when called by a measured stage.
inline Table Make(const std::vector<std::string_view>& names, std::size_t capacity = 0) {
    Table table(static_cast<unsigned>(capacity),
        Table::eTableFlagNull64 | Table::eTableFlagDuplicateStrings);
    std::vector<std::tuple<std::string_view, unsigned, std::string_view>> columns;
    for (auto name : names) {
        columns.emplace_back(name == "name" ? "rstring" : "int64", 0, name);
    }
    table.column_add(columns, gd::table::tag_type_name{});
    Check(table.prepare());

    return table;
}

// GD DTO harvest() adds the selected row count to a prepared destination's
// reservation, so a pre-sized destination would be allocated twice and copied.
// The SIMD adapter reserves only when full and needs the capacity up front.
inline Table MakeHarvestTarget(const std::vector<std::string_view>& names, std::size_t rows) {
#ifdef GD_WORKFLOW_SIMD
    return Make(names, rows);
#else
    static_cast<void>(rows);

    return Make(names);
#endif
}

// Import one SQLite business table in source row order. Copy each cursor row
// into independently owned GD storage before advancing the cursor.
inline Table Read(
    Database& db, const std::string& name, const std::vector<std::string_view>& columns) {
    auto table = Make(columns);

    gd::database::sqlite::cursor cursor(&db);
    Check(cursor.open("SELECT * FROM " + name + " ORDER BY rowid"));

    std::vector<View> values(columns.size());
    while (cursor.is_valid_row()) {
        for (unsigned column = 0; column < values.size(); ++column) {
            values[column] = cursor[column];
        }
        table.row_add(values);
        Check(cursor.next());
    }

    return table;
}

// "import": read customers, orders, and lines sequentially. The standalone
// stage destroys them before stopping its timer; "complete" keeps them for joins.
inline Inputs Load(Database& db) {
    return {Read(db, "customers", {"id", "name", "region", "active"}),
        Read(db, "orders", {"id", "customer_id", "day", "status"}),
        Read(db, "lines", {"id", "order_id", "quantity", "unit_price"})};
}

// A missing left-join match supplies NULL fields to the audit row.
inline View Cell(const Table& table, Position row, unsigned column) {
    return row ? table.cell_get_variant_view(*row, column) : View{};
}

// "prepare": build a sorted index on the right table's ID column, then map
// each left row to an optional right-row position. Keep every left row and its
// original order; this mapping avoids materializing an intermediate joined table.
// GD's integer index reports a lower_bound candidate as a hit. Check key equality.
inline std::vector<Position> Join(const Table& left, unsigned key, const Table& right) {
    gd::table::index_int64 index;
    index.m_vectorIndex.reserve(right.get_row_count());
    for (std::uint64_t row = 0; row < right.get_row_count(); ++row) {
        index.add(right.cell_get_variant_view(row, 0u), row);
    }
    index.sort();
    std::vector<Position> result;
    result.reserve(left.get_row_count());
    for (std::uint64_t row = 0; row < left.get_row_count(); ++row) {
        const auto value = left.cell_get_variant_view(row, key);
        Position found;
        if (!value.is_null()) {
            const auto [hit, candidate] = index.find(value.as_int64());
            if (hit && right.cell_get_variant_view(candidate, 0u).as_int64() == value.as_int64()) {
                found = candidate;
            }
        }
        result.push_back(found);
    }

    return result;
}

inline Prepared Prepare(const Inputs& input) {
    // "prepare", joins: orders -> customers, then lines -> orders. Together
    // these mappings connect each line to fields from all three business tables.
    const auto orderCustomers = Join(input.orders, 1, input.customers);
    const auto lineOrders = Join(input.lines, 1, input.orders);
    auto audit = Make(auditNames, input.lines.get_row_count());

    // "prepare", validation: retain one audit row for every input line,
    // including broken references and invalid values. Missing matches become NULL.
    for (std::uint64_t line = 0; line < input.lines.get_row_count(); ++line) {
        const auto order = lineOrders[line];
        const auto customer = order ? orderCustomers[*order] : Position{};
        const auto l = [&](unsigned c) {
            return Cell(input.lines, line, c);
        };
        const auto o = [&](unsigned c) {
            return Cell(input.orders, order, c);
        };
        const auto c = [&](unsigned col) {
            return Cell(input.customers, customer, col);
        };
        const bool quantityOk = !l(2).is_null() && l(2).as_int64() > 0;
        const bool priceOk = !l(3).is_null() && l(3).as_int64() >= 0;

        // Independent error bits: 1 missing order, 2 missing customer,
        // 4 missing/empty name, 8 inactive customer, 16 invalid quantity,
        // 32 invalid price, and 64 missing order date. Multiple bits may be set.
        std::int64_t errors = !order;
        errors |= (order && !customer) * 2;
        errors |= (customer && (c(1).is_null() || c(1).as_string_view().empty())) * 4;
        errors |= (customer && c(3).as_int64() == 0) * 8;
        errors |= !quantityOk * 16;
        errors |= !priceOk * 32;
        errors |= (order && o(2).is_null()) * 64;

        // Gross cents are quantity * unit_price when both operands are valid.
        // Checked arithmetic aborts on overflow rather than wrapping a value.
        const View amount =
            quantityOk && priceOk ? View(Multiply(l(2).as_int64(), l(3).as_int64())) : View{};
        audit.row_add({l(0), l(1), o(1), c(1), c(2), o(2), o(3), l(2), l(3), amount, View(errors)});
    }

    // "prepare", exclusion: harvest every column of error-free audit rows
    // into an owned clean table. Invalid rows remain available in the audit table.
    std::vector<std::uint64_t> valid;
    for (std::uint64_t row = 0; row < audit.get_row_count(); ++row) {
        if (audit.cell_get_variant_view(row, 10u).as_int64() == 0) {
            valid.push_back(row);
        }
    }
    auto clean = MakeHarvestTarget(auditNames, valid.size());
    audit.harvest({0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10}, valid, clean);

    return {std::move(audit), std::move(clean)};
}

// "variants": one of eight parameterized result tables. Each task scans the
// complete immutable clean table and owns its row selection and destination.
// Filtering, projection, copying, and discount calculation are all timed.
inline Table Variant(const Table& clean, const Parameters& p) {
    const auto [region, fromDay, toDay, status, minimum, discountBp] = p;
    std::vector<std::uint64_t> rows;

    // Select region/status (or any when -1), the half-open date interval,
    // and the minimum gross amount. Both layouts preserve source row order.
#ifdef GD_WORKFLOW_SIMD
    // Clean rows have no nulls in these fields. Read contiguous eight-lane
    // column packs directly; preserve source order and handle a partial tail.
    // These are AoSoA layout accesses; predicates still test each lane separately.
    for (std::uint64_t first = 0; first < clean.get_row_count(); first += 8) {
        const auto regions = clean.pack(first, 4), days = clean.pack(first, 5);
        const auto statuses = clean.pack(first, 6), amounts = clean.pack(first, 9);
        for (unsigned lane = 0; lane < days.size(); ++lane) {
            if ((region == -1 || regions[lane] == region) && days[lane] >= fromDay &&
                days[lane] < toDay && (status == -1 || statuses[lane] == status) &&
                amounts[lane] >= minimum) {
                rows.push_back(first + lane);
            }
        }
    }
#else
    for (std::uint64_t row = 0; row < clean.get_row_count(); ++row) {
        const auto n = [&](unsigned c) {
            return clean.cell_get_variant_view(row, c).as_int64();
        };
        if ((region == -1 || n(4) == region) && n(5) >= fromDay && n(5) < toDay &&
            (status == -1 || n(6) == status) && n(9) >= minimum) {
            rows.push_back(row);
        }
    }
#endif
    // Project six columns and copy the chosen rows into a separate result.
    // AoS uses native harvest; AoSoA uses the adapter's projected gather.
    auto result = MakeHarvestTarget(
        {"line_id", "name", "region", "day", "status", "amount_cents"}, rows.size());

    clean.harvest(variantColumns, rows, result);

    // Replace gross cents in this result with discounted cents, rounded half up.
    // The source clean table remains unchanged for the other parameter sets.
    for (std::uint64_t row = 0; row < result.get_row_count(); ++row) {
        const auto gross = result.cell_get_variant_view(row, 5u).as_int64();
        const auto discounted = Multiply(gross, 10000 - discountBp);

        if (discounted > std::numeric_limits<std::int64_t>::max() - 5000) {
            throw std::overflow_error("amount rounding overflow");
        }

        result.cell_set(row, 5u, View((discounted + 5000) / 10000));
    }

    return result;
}
} // namespace workflow
