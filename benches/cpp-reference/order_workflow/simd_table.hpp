#pragma once
// Benchmark adapter for the pinned GD SIMD implementation; no upstream edits.
#include <algorithm>
#include <bit>
#include <memory>
#include <span>
#include <stdexcept>
#include <vector>
#include "gd_table_simd.h"

namespace workflow {
class SimdTable {
    using Storage = gd::table::simd::table_8_8;
    struct Delete {
        void operator()(Storage* table) const {
            // SIMD clear() frees data but does not release its schema. Each table
            // here owns a fresh schema; moving the pointer never copies GD storage.
            auto* columns = table->get_columns();
            delete table;
            if(columns) columns->release();
        }
    };
    std::unique_ptr<Storage, Delete> storage;
    unsigned width = 0;
public:
    static constexpr unsigned eTableFlagNull64 = Storage::eTableFlagNull64;
    static constexpr unsigned eTableFlagDuplicateStrings = Storage::eTableFlagDuplicateStrings;
    SimdTable() = default;
    SimdTable(std::uint64_t capacity, unsigned flags)
        : storage(new Storage(std::max<std::uint64_t>(1, (capacity + 7) / 8),
                              flags & eTableFlagDuplicateStrings)) {
        storage->column_prepare();
    }
    void column_add(const std::vector<std::tuple<std::string_view, unsigned, std::string_view>>& columns,
                    gd::table::tag_type_name) {
        // The SIMD vector/type-name overload is declared but not defined.
        for(const auto& [type, size, name] : columns) storage->column_add(type, size, name);
        width = static_cast<unsigned>(columns.size());
        // Native nullable metadata is allocated per pack but addressed per row.
        // Keep the logical row's null bitmap in a regular packed GD uint64 column.
        storage->column_add("uint64", 0, "__workflow_nulls");
        // SIMD prepare() omits descriptor positions and reference flags. Set
        // them through GD's public schema API so cell_set owns rstring values.
        for(unsigned c = 0; c <= width; ++c) {
            auto* column = storage->get_columns()->get(c);
            column->position(c * Storage::size_pack_s());
            if(gd::types::is_reference_g(column->type()))
                column->state(gd::table::detail::column::eColumnStateReference);
        }
    }
    auto prepare() { return storage->prepare(); }
    auto get_row_count() const { return storage ? storage->get_row_count() : 0; }
    unsigned get_column_count() const { return width; }
    gd::variant_view cell_get_variant_view(std::uint64_t row, unsigned column) const {
        if(storage->cell_get_value64(row, width) & (std::uint64_t{1} << column)) return {};
        // The native variant getter uses column.position(), which does not encode
        // the SIMD lane. These two public getters use the packed offset instead.
        if(storage->get_columns()->get(column)->is_reference()) {
            const auto* value = storage->cell_get_reference(row, column);
            return {value->ctype(), value->data(), value->length()};
        }
        return gd::variant_view(std::bit_cast<std::int64_t>(storage->cell_get_value64(row, column)));
    }
    void cell_set(std::uint64_t row, unsigned column, gd::variant_view value) {
        auto nulls = storage->cell_get_value64(row, width);
        const auto bit = std::uint64_t{1} << column;
        if(value.is_null()) nulls |= bit;
        else { storage->cell_set(row, column, value); nulls &= ~bit; }
        storage->cell_set_value(row, width, nulls);
    }
    void row_add(std::span<const gd::variant_view> values) {
        const auto row = get_row_count();
        if(row == storage->count_reserved_row()) {
            // Native row_add grows one pack at a time. Reserve geometrically to
            // avoid quadratic reallocations when importing an unknown row count.
            storage->row_reserve_add(std::max<std::uint64_t>(32, row / 8));
        }
        storage->row_add();
        std::uint64_t nulls = 0;
        for(unsigned column = 0; column < width; ++column) {
            if(values[column].is_null()) nulls |= std::uint64_t{1} << column;
            else storage->cell_set(row, column, values[column]);
        }
        storage->cell_set_value(row, width, nulls);
    }
    void row_add(std::initializer_list<gd::variant_view> values) {
        row_add(std::span<const gd::variant_view>(values.begin(), values.size()));
    }
    void harvest(const std::vector<unsigned>& columns, const std::vector<std::uint64_t>& rows,
                 SimdTable& destination) const {
        // SIMD harvest has no projected-column overload. Copy borrowed views
        // through GD cell_set into the destination's own reference-string store.
        std::vector<gd::variant_view> values(columns.size());
        for(auto row : rows) {
            for(unsigned c = 0; c < columns.size(); ++c) values[c] = cell_get_variant_view(row, columns[c]);
            destination.row_add(values);
        }
    }
    std::span<const std::int64_t> pack(std::uint64_t firstRow, unsigned column) const {
        // Exclude uninitialized lanes in the final partial pack. The upstream
        // pack_harvest_span asserts that the *whole table* has a multiple of 8 rows.
        const auto count = std::min<std::uint64_t>(8, get_row_count() - firstRow);
        return {reinterpret_cast<const std::int64_t*>(storage->rowpack_get(firstRow / 8, column)), count};
    }
};
} // namespace workflow
