// Random-access iterator over the rows of a GD table_column_buffer, so standard
// algorithms such as std::sort can reorder the rows in place.
//
// An element is one row: its data bytes plus its metadata bytes (null flags and
// row status), which GD keeps in a separate region. Swapping two elements calls
// GD's own table_column_buffer::swap(); a temporary element is a copy of both
// parts. The table must have no more than kMaxRowBytes of data and
// kMaxMetaBytes of metadata per row.
//
// table_rows::Sort() reads keys straight from the row bytes instead of going
// through cell_get_variant_view(). It relies on GD's row layout: a value starts
// at its column's position(), numbers are stored in their native representation,
// and fixed-capacity strings as a uint32 length followed by the bytes. Strings
// are compared byte-wise. Key columns must not be null.

#pragma once

#include <algorithm>
#include <array>
#include <cassert>
#include <cstddef>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <iterator>
#include <string_view>
#include <type_traits>

#include "gd_table_column-buffer.h"

namespace table_rows {

using table = gd::table::dto::table;

constexpr std::size_t kMaxRowBytes = 128;
constexpr std::size_t kMaxMetaBytes = 16;

// A row copied out of the table, for the temporaries std::sort keeps.
struct Row {
    std::array<std::uint8_t, kMaxRowBytes> data;
    std::array<std::uint8_t, kMaxMetaBytes> meta;
};

// A row inside the table.
struct RowRef {
    table* ptable;
    std::uint64_t uRow;

    operator Row() const {
        Row row;
        std::memcpy(row.data.data(), ptable->row_get(uRow), ptable->size_row());
        std::memcpy(row.meta.data(), ptable->row_get_meta(uRow), ptable->size_row_meta());
        return row;
    }
    RowRef& operator=(const Row& row) {
        std::memcpy(ptable->row_get(uRow), row.data.data(), ptable->size_row());
        std::memcpy(ptable->row_get_meta(uRow), row.meta.data(), ptable->size_row_meta());
        return *this;
    }
    RowRef& operator=(const RowRef& o) { return *this = static_cast<Row>(o); }
    friend void swap(RowRef a, RowRef b) { a.ptable->swap(a.uRow, b.uRow); }

    const std::uint8_t* data() const { return ptable->row_get(uRow); }
};

inline const std::uint8_t* Data(const Row& row) { return row.data.data(); }
inline const std::uint8_t* Data(const RowRef& row) { return row.data(); }

struct Iterator {
    using iterator_category = std::random_access_iterator_tag;
    using value_type = Row;
    using difference_type = std::ptrdiff_t;
    using reference = RowRef;
    using pointer = void;

    table* ptable;
    std::uint64_t uRow;

    RowRef operator*() const { return {ptable, uRow}; }
    RowRef operator[](difference_type i) const { return {ptable, uRow + i}; }
    Iterator& operator++() { ++uRow; return *this; }
    Iterator operator++(int) { auto it = *this; ++uRow; return it; }
    Iterator& operator--() { --uRow; return *this; }
    Iterator operator--(int) { auto it = *this; --uRow; return it; }
    Iterator& operator+=(difference_type i) { uRow += i; return *this; }
    Iterator& operator-=(difference_type i) { uRow -= i; return *this; }
    friend Iterator operator+(Iterator it, difference_type i) { return it += i; }
    friend Iterator operator+(difference_type i, Iterator it) { return it += i; }
    friend Iterator operator-(Iterator it, difference_type i) { return it -= i; }
    friend difference_type operator-(Iterator a, Iterator b) {
        return static_cast<difference_type>(a.uRow) - static_cast<difference_type>(b.uRow);
    }
    friend auto operator<=>(Iterator a, Iterator b) { return a.uRow <=> b.uRow; }
    friend bool operator==(Iterator a, Iterator b) { return a.uRow == b.uRow; }
};

inline Iterator begin(table& tableRows) { return {&tableRows, 0}; }
inline Iterator end(table& tableRows) { return {&tableRows, tableRows.size()}; }

// Orders rows by one column of type T, reading the key straight from the row
// bytes: a number at the column offset, or for std::string_view a fixed-capacity
// string stored as a uint32 length followed by the bytes.
template <typename T>
struct Less {
    unsigned uPosition;

    T Key(const std::uint8_t* puRow) const {
        if constexpr (std::is_same_v<T, std::string_view>) {
            std::uint32_t uLength;
            std::memcpy(&uLength, puRow + uPosition, sizeof uLength);
            return {reinterpret_cast<const char*>(puRow + uPosition + sizeof uLength), uLength};
        } else {
            T value;
            std::memcpy(&value, puRow + uPosition, sizeof value);
            return value;
        }
    }

    template <typename A, typename B>
    bool operator()(const A& a, const B& b) const {
        return Key(Data(a)) < Key(Data(b));
    }
};

// Calls f with the Less<T> for a uint64, int32, uint32, double, or
// fixed-capacity string column.
template <typename F>
void WithLess(const table& tableKey, unsigned uColumn, F f) {
    const auto* pcolumn = tableKey.column_get(uColumn, gd::table::tag_pointer{});
    const unsigned uPosition = pcolumn->position();
    if (pcolumn->is_length()) return f(Less<std::string_view>{uPosition});
    switch (pcolumn->ctype_number()) {
        case gd::types::eTypeNumberUInt64: return f(Less<std::uint64_t>{uPosition});
        case gd::types::eTypeNumberInt32: return f(Less<std::int32_t>{uPosition});
        case gd::types::eTypeNumberUInt32: return f(Less<std::uint32_t>{uPosition});
        case gd::types::eTypeNumberDouble: return f(Less<double>{uPosition});
        default: assert(false && "unsupported key column type"); std::abort();
    }
}

// Sorts the table's rows in place by one column.
inline void Sort(table& tableSort, unsigned uColumn) {
    assert(tableSort.size_row() <= kMaxRowBytes && tableSort.size_row_meta() <= kMaxMetaBytes);
    WithLess(tableSort, uColumn, [&](auto less) { std::sort(begin(tableSort), end(tableSort), less); });
}

}  // namespace table_rows
