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
// at its column's position(), uint64 values are stored as 8 bytes, and
// fixed-capacity strings as a uint32 length followed by the bytes. Strings are
// compared byte-wise.

#pragma once

#include <algorithm>
#include <array>
#include <cassert>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <iterator>
#include <string_view>

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

// Orders rows by one column, reading the key straight from the row bytes. GD
// stores uint64 values at the column offset and fixed-capacity strings as a
// uint32 length followed by the bytes.
struct Less {
    unsigned uPosition;
    bool bString;

    std::string_view Text(const std::uint8_t* puRow) const {
        std::uint32_t uLength;
        std::memcpy(&uLength, puRow + uPosition, sizeof uLength);
        return {reinterpret_cast<const char*>(puRow + uPosition + sizeof uLength), uLength};
    }
    std::uint64_t Number(const std::uint8_t* puRow) const {
        std::uint64_t uValue;
        std::memcpy(&uValue, puRow + uPosition, sizeof uValue);
        return uValue;
    }

    template <typename A, typename B>
    bool operator()(const A& a, const B& b) const {
        if (bString) return Text(Data(a)) < Text(Data(b));
        return Number(Data(a)) < Number(Data(b));
    }
};

// Sorts the table's rows in place by a uint64 or fixed-capacity string column.
inline void Sort(table& tableSort, unsigned uColumn) {
    assert(tableSort.size_row() <= kMaxRowBytes && tableSort.size_row_meta() <= kMaxMetaBytes);
    const auto* pcolumn = tableSort.column_get(uColumn, gd::table::tag_pointer{});
    Less less{pcolumn->position(), pcolumn->is_length()};
    std::sort(begin(tableSort), end(tableSort), less);
}

}  // namespace table_rows
