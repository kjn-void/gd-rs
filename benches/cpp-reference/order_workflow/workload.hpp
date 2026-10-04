#pragma once
// Order-processing application. The upstream GD checkout is never modified.
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

namespace workflow {
using Table = gd::table::table_column_buffer;
using Database = gd::database::sqlite::database;
using View = gd::variant_view;
using Parameters = std::array<std::int64_t, 6>;
using Position = std::optional<std::uint64_t>;
inline const std::vector<std::string_view> auditNames{
    "line_id", "order_id", "customer_id", "name", "region", "day", "status",
    "quantity", "unit_price", "amount_cents", "errors"};
inline const std::vector<unsigned> variantColumns{0, 3, 4, 5, 6, 9};
struct Inputs { Table customers, orders, lines; };
struct Prepared { Table audit, clean; };
inline void Check(const std::pair<bool, std::string>& result) {
    if(!result.first) throw std::runtime_error(result.second);
}
inline std::int64_t Multiply(std::int64_t a, std::int64_t b) {
    if(a < 0 || b < 0 || (b != 0 && a > std::numeric_limits<std::int64_t>::max() / b))
        throw std::overflow_error("amount multiplication overflow");
    return a * b;
}
inline Table Make(const std::vector<std::string_view>& names, std::size_t capacity = 0) {
    Table table(static_cast<unsigned>(capacity), Table::eTableFlagNull64 | Table::eTableFlagDuplicateStrings);
    std::vector<std::tuple<std::string_view, unsigned, std::string_view>> columns;
    for(auto name : names) columns.emplace_back(name == "name" ? "rstring" : "int64", 0, name);
    table.column_add(columns, gd::table::tag_type_name{});
    Check(table.prepare());
    return table;
}
inline Table Read(Database& db, const std::string& name, const std::vector<std::string_view>& columns) {
    auto table = Make(columns);
    gd::database::sqlite::cursor cursor(&db);
    Check(cursor.open("SELECT * FROM " + name + " ORDER BY rowid"));
    std::vector<View> values(columns.size());
    while(cursor.is_valid_row()) {
        for(unsigned column = 0; column < values.size(); ++column) values[column] = cursor[column];
        table.row_add(values);
        Check(cursor.next());
    }
    return table;
}
inline Inputs Load(Database& db) {
    return {Read(db, "customers", {"id", "name", "region", "active"}),
            Read(db, "orders", {"id", "customer_id", "day", "status"}),
            Read(db, "lines", {"id", "order_id", "quantity", "unit_price"})};
}
inline View Cell(const Table& table, Position row, unsigned column) {
    return row ? table.cell_get_variant_view(*row, column) : View{};
}
// GD's integer index reports a lower_bound candidate as a hit. Check key equality.
inline std::vector<Position> Join(const Table& left, unsigned key, const Table& right) {
    gd::table::index_int64 index;
    index.m_vectorIndex.reserve(right.get_row_count());
    for(std::uint64_t row = 0; row < right.get_row_count(); ++row)
        index.add(right.cell_get_variant_view(row, 0u), row);
    index.sort();
    std::vector<Position> result;
    result.reserve(left.get_row_count());
    for(std::uint64_t row = 0; row < left.get_row_count(); ++row) {
        const auto value = left.cell_get_variant_view(row, key);
        Position found;
        if(!value.is_null()) {
            const auto [hit, candidate] = index.find(value.as_int64());
            if(hit && right.cell_get_variant_view(candidate, 0u).as_int64() == value.as_int64())
                found = candidate;
        }
        result.push_back(found);
    }
    return result;
}
inline Prepared Prepare(const Inputs& input) {
    const auto orderCustomers = Join(input.orders, 1, input.customers);
    const auto lineOrders = Join(input.lines, 1, input.orders);
    auto audit = Make(auditNames, input.lines.get_row_count());
    for(std::uint64_t line = 0; line < input.lines.get_row_count(); ++line) {
        const auto order = lineOrders[line];
        const auto customer = order ? orderCustomers[*order] : Position{};
        const auto l = [&](unsigned c) { return Cell(input.lines, line, c); };
        const auto o = [&](unsigned c) { return Cell(input.orders, order, c); };
        const auto c = [&](unsigned col) { return Cell(input.customers, customer, col); };
        const bool quantityOk = !l(2).is_null() && l(2).as_int64() > 0;
        const bool priceOk = !l(3).is_null() && l(3).as_int64() >= 0;
        std::int64_t errors = !order;
        errors |= (order && !customer) * 2;
        errors |= (customer && (c(1).is_null() || c(1).as_string_view().empty())) * 4;
        errors |= (customer && c(3).as_int64() == 0) * 8;
        errors |= !quantityOk * 16;
        errors |= !priceOk * 32;
        errors |= (order && o(2).is_null()) * 64;
        const View amount = quantityOk && priceOk ? View(Multiply(l(2).as_int64(), l(3).as_int64())) : View{};
        audit.row_add({l(0), l(1), o(1), c(1), c(2), o(2), o(3), l(2), l(3), amount, View(errors)});
    }
    std::vector<std::uint64_t> valid;
    for(std::uint64_t row = 0; row < audit.get_row_count(); ++row)
        if(audit.cell_get_variant_view(row, 10u).as_int64() == 0) valid.push_back(row);
    auto clean = Make(auditNames, valid.size());
    audit.harvest({0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10}, valid, clean);
    return {std::move(audit), std::move(clean)};
}
inline Table Variant(const Table& clean, const Parameters& p) {
    const auto [region, fromDay, toDay, status, minimum, discountBp] = p;
    std::vector<std::uint64_t> rows;
    for(std::uint64_t row = 0; row < clean.get_row_count(); ++row) {
        const auto n = [&](unsigned c) { return clean.cell_get_variant_view(row, c).as_int64(); };
        if((region == -1 || n(4) == region) && n(5) >= fromDay && n(5) < toDay &&
           (status == -1 || n(6) == status) && n(9) >= minimum) rows.push_back(row);
    }
    auto result = Make({"line_id", "name", "region", "day", "status", "amount_cents"}, rows.size());
    clean.harvest(variantColumns, rows, result);
    for(std::uint64_t row = 0; row < result.get_row_count(); ++row) {
        const auto gross = result.cell_get_variant_view(row, 5u).as_int64();
        const auto discounted = Multiply(gross, 10000 - discountBp);
        if(discounted > std::numeric_limits<std::int64_t>::max() - 5000)
            throw std::overflow_error("amount rounding overflow");
        result.cell_set(row, 5u, View((discounted + 5000) / 10000));
    }
    return result;
}
} // namespace workflow
