// Cached-key sort and direct column-wise materialization for the standard SoA
// table. The source is never appended or mutated. The workspace retains key
// storage; the target retains its vectors and std::string payload capacity.
#pragma once

#include "soa_table_row_iterator.h"

#include <optional>
#include <string>
#include <string_view>
#include <type_traits>

namespace soa_rows {

template <typename Cell>
struct CachedKey {
    using type = Cell;
    static type Read(const Cell& cell) { return cell; }
};

template <typename Character, typename Traits, typename Allocator>
struct CachedKey<std::basic_string<Character, Traits, Allocator>> {
    using type = std::basic_string_view<Character, Traits>;
    static type Read(const std::basic_string<Character, Traits, Allocator>& cell) { return cell; }
};

template <typename Cell>
struct CachedKey<std::optional<Cell>> {
    using type = typename CachedKey<Cell>::type;
    static type Read(const std::optional<Cell>& cell) {
        if (!cell) throw std::invalid_argument("SoA sort key must be non-null");
        return CachedKey<Cell>::Read(*cell);
    }
};

template <std::size_t Column, typename Table>
class KeySortWorkspace {
    using KeyCell = std::tuple_element_t<Column, typename Table::row_type>;
    using Key = typename CachedKey<KeyCell>::type;

public:
    explicit KeySortWorkspace(std::size_t capacity = 0) { keys_.reserve(capacity); }

    void sort_into(const Table& source, Table& target) {
        if (&source == &target) throw std::invalid_argument("SoA sort source and target must differ");
        keys_.clear();
        std::size_t position = 0;
        // The const row iterator specializes cell access at compile time. String
        // views remain valid because neither sorting nor gathering changes source.
        for (auto row : source) {
            keys_.emplace_back(CachedKey<KeyCell>::Read(row.template get<Column>()), position++);
        }
        std::sort(keys_.begin(), keys_.end(), [](const auto& left, const auto& right) {
            return left.first < right.first;
        });
        target.assign_rows_from(source, keys_.size(), [&](std::size_t row) { return keys_[row].second; });
    }

private:
    std::vector<std::pair<Key, std::size_t>> keys_;
};

}  // namespace soa_rows
