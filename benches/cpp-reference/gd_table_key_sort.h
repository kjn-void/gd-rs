// GD key sort from https://www.sweclockers.com/forum/post/21315112,
// adapted to public accessors in pinned GD a5b1e3e. No submodule edits.
//
// The pinned variant_view lacks tag_same_type. The two string kinds used by
// this fixture therefore use the equivalent direct byte/length comparison;
// numeric comparisons read the same union members as the posted implementation.
// Other kinds retain the pinned GD comparator as a compatibility fallback.
#pragma once

#include <algorithm>
#include <cassert>
#include <cstdint>
#include <cstring>
#include <memory>
#include <utility>
#include <vector>

#include "gd_table_column-buffer.h"

namespace table_key_sort {

using table = gd::table::dto::table;

inline bool SameTypeLess(const gd::variant_view& a, const gd::variant_view& b) {
    // Keep the per-comparison type dispatch of GD's same-type fallback, without
    // checking that both types match. The benchmark's string lengths are equal;
    // min(length) also makes prefix/empty strings well-defined in verification.
    switch (a.type_number()) {
        case gd::types::eTypeNumberString:
        case gd::types::eTypeNumberUtf8String: {
            const auto length = std::min(a.m_uSize, b.m_uSize);
            const int order = length == 0 ? 0 : std::memcmp(a.m_V.pbsz, b.m_V.pbsz, length);
            return order != 0 ? order < 0 : a.m_uSize < b.m_uSize;
        }
        default: return a.less(b);
    }
}

inline void Sort(table& rows, unsigned column, bool ascending,
                 std::uint64_t first, std::uint64_t count) {
    assert(column < rows.get_column_count());
    assert(first <= rows.size() && count <= rows.size() - first);
    if (count < 2) return;

    std::vector<std::pair<gd::variant_view, std::uint64_t>> keys;
    keys.reserve(count);
    for (std::uint64_t row = first; row < first + count; ++row) {
        assert((!rows.is_null32() && !rows.is_null64()) || !rows.cell_is_null(row, column));
        keys.emplace_back(rows.cell_get_variant_view(row, column, gd::table::tag_raw{}), row);
    }

    const auto sort = [&keys, ascending](auto compare) {
        if (ascending) {
            std::stable_sort(keys.begin(), keys.end(), [&compare](const auto& a, const auto& b) {
                return compare(a.first, b.first);
            });
        } else {
            std::stable_sort(keys.begin(), keys.end(), [&compare](const auto& a, const auto& b) {
                return compare(b.first, a.first);
            });
        }
    };

    switch (keys.front().first.type_number()) {
        case gd::types::eTypeNumberInt32:
            sort([](const auto& a, const auto& b) { return a.m_V.int32 < b.m_V.int32; }); break;
        case gd::types::eTypeNumberUInt32:
            sort([](const auto& a, const auto& b) { return a.m_V.uint32 < b.m_V.uint32; }); break;
        case gd::types::eTypeNumberInt64:
            sort([](const auto& a, const auto& b) { return a.m_V.int64 < b.m_V.int64; }); break;
        case gd::types::eTypeNumberUInt64:
            sort([](const auto& a, const auto& b) { return a.m_V.uint64 < b.m_V.uint64; }); break;
        case gd::types::eTypeNumberDouble:
            sort([](const auto& a, const auto& b) { return a.m_V.d < b.m_V.d; }); break;
        default:
            sort([](const auto& a, const auto& b) { return SameTypeLess(a, b); }); break;
    }

    const std::uint64_t row_size = rows.size_row();
    const std::uint64_t meta_size = rows.is_rowmeta() ? rows.size_row_meta() : 0;
    auto buffer = std::make_unique_for_overwrite<std::uint8_t[]>(
        std::max(count * row_size, count * meta_size));
    for (std::uint64_t position = 0; position < count; ++position) {
        std::memcpy(buffer.get() + position * row_size, rows.row_get(keys[position].second), row_size);
    }
    std::memcpy(rows.row_get(first), buffer.get(), count * row_size);

    // String key views point into rows that have now been overwritten. Only
    // the cached row numbers may be read during this second gather.
    if (meta_size != 0) {
        for (std::uint64_t position = 0; position < count; ++position) {
            std::memcpy(buffer.get() + position * meta_size,
                        rows.row_get_meta(keys[position].second), meta_size);
        }
        std::memcpy(rows.row_get_meta(first), buffer.get(), count * meta_size);
    }
}

inline void Sort(table& rows, unsigned column) {
    Sort(rows, column, true, 0, rows.size());
}

}  // namespace table_key_sort
