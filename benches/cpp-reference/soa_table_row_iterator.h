// A standard-container SoA table and a typed random-access row iterator.
//
// Dereferencing reads a proxy over one element of every column. Converting that
// proxy to row_type copies the row. iter_move and the explicit moving iterator
// expose a distinct moving proxy, so legacy std::sort moves owning cells without
// making ordinary reads or row assignments destructive. A moving base iterator
// also preserves this distinction when libstdc++ unwraps iterator adaptors.
#pragma once

#include <algorithm>
#include <cassert>
#include <compare>
#include <cstddef>
#include <iterator>
#include <stdexcept>
#include <tuple>
#include <type_traits>
#include <utility>
#include <vector>

namespace soa_rows {

template <typename Table, bool Move>
class RowReference {
public:
    using row_type = typename Table::row_type;

    RowReference(Table& table, std::size_t row) : table_(&table), row_(row) {}
    RowReference(const RowReference&) = default;

    template <std::size_t Column>
    decltype(auto) get() const { return table_->template cell<Column>(row_); }

    operator row_type() const {
        return Read(std::make_index_sequence<Table::column_count>{});
    }

    const RowReference& operator=(const RowReference& source) const requires (!std::is_const_v<Table>) {
        Assign(source, std::make_index_sequence<Table::column_count>{});
        return *this;
    }
    template <bool OtherMove>
    const RowReference& operator=(const RowReference<Table, OtherMove>& source) const requires (!std::is_const_v<Table>) {
        Assign(source, std::make_index_sequence<Table::column_count>{});
        return *this;
    }
    const RowReference& operator=(const row_type& source) const requires (!std::is_const_v<Table>) {
        AssignValue(source, std::make_index_sequence<Table::column_count>{});
        return *this;
    }
    const RowReference& operator=(row_type&& source) const requires (!std::is_const_v<Table>) {
        AssignValue(std::move(source), std::make_index_sequence<Table::column_count>{});
        return *this;
    }

    friend void swap(RowReference left, RowReference right) requires (!std::is_const_v<Table>) {
        Swap(left, right, std::make_index_sequence<Table::column_count>{});
    }

private:
    template <std::size_t... Columns>
    row_type Read(std::index_sequence<Columns...>) const {
        if constexpr (Move) return row_type(std::move(get<Columns>())...);
        else return row_type(get<Columns>()...);
    }
    template <bool OtherMove, std::size_t... Columns>
    void Assign(const RowReference<Table, OtherMove>& source,
                std::index_sequence<Columns...>) const {
        if (table_ == source.table_ && row_ == source.row_) return;
        if constexpr (OtherMove) ((get<Columns>() = std::move(source.template get<Columns>())), ...);
        else ((get<Columns>() = source.template get<Columns>()), ...);
    }
    template <typename Row, std::size_t... Columns>
    void AssignValue(Row&& source, std::index_sequence<Columns...>) const {
        ((get<Columns>() = std::get<Columns>(std::forward<Row>(source))), ...);
    }
    template <std::size_t... Columns>
    static void Swap(RowReference left, RowReference right, std::index_sequence<Columns...>) {
        using std::swap;
        (swap(left.template get<Columns>(), right.template get<Columns>()), ...);
    }

    template <typename, bool> friend class RowReference;
    Table* table_;
    std::size_t row_;
};

template <typename Table, bool Move = false>
class RowIterator {
public:
    using iterator_category = std::random_access_iterator_tag;
    using value_type = typename Table::row_type;
    using difference_type = std::ptrdiff_t;
    using reference = RowReference<Table, Move>;
    using pointer = void;

    RowIterator() = default;
    RowIterator(Table& table, difference_type row) : table_(&table), row_(row) {}

    reference operator*() const { return {*table_, static_cast<std::size_t>(row_)}; }
    reference operator[](difference_type offset) const { return *(*this + offset); }
    RowIterator& operator++() { ++row_; return *this; }
    RowIterator operator++(int) { auto old = *this; ++*this; return old; }
    RowIterator& operator--() { --row_; return *this; }
    RowIterator operator--(int) { auto old = *this; --*this; return old; }
    RowIterator& operator+=(difference_type offset) { row_ += offset; return *this; }
    RowIterator& operator-=(difference_type offset) { row_ -= offset; return *this; }
    friend RowIterator operator+(RowIterator it, difference_type offset) { return it += offset; }
    friend RowIterator operator+(difference_type offset, RowIterator it) { return it += offset; }
    friend RowIterator operator-(RowIterator it, difference_type offset) { return it -= offset; }
    friend difference_type operator-(RowIterator left, RowIterator right) {
        assert(left.table_ == right.table_);
        return left.row_ - right.row_;
    }
    friend auto operator<=>(RowIterator left, RowIterator right) {
        assert(left.table_ == right.table_);
        return left.row_ <=> right.row_;
    }
    friend bool operator==(RowIterator left, RowIterator right) {
        return left.table_ == right.table_ && left.row_ == right.row_;
    }
    friend RowReference<Table, true> iter_move(const RowIterator& it) {
        return {*it.table_, static_cast<std::size_t>(it.row_)};
    }

private:
    Table* table_ = nullptr;
    difference_type row_ = 0;
};

template <typename... Cells>
class Table {
public:
    static_assert(sizeof...(Cells) > 0);
    using row_type = std::tuple<Cells...>;
    using iterator = RowIterator<Table>;
    using const_iterator = RowIterator<const Table>;
    using moving_iterator = RowIterator<Table, true>;
    static constexpr std::size_t column_count = sizeof...(Cells);

    std::size_t size() const { return std::get<0>(columns_).size(); }
    void clear() noexcept { std::apply([](auto&... cols) { (cols.clear(), ...); }, columns_); }
    void reserve(std::size_t rows) {
        std::apply([&](auto&... cols) { (cols.reserve(rows), ...); }, columns_);
    }
    template <std::size_t Column>
    decltype(auto) cell(std::size_t row) { return std::get<Column>(columns_)[row]; }
    template <std::size_t Column>
    decltype(auto) cell(std::size_t row) const { return std::get<Column>(columns_)[row]; }

    void push_back(row_type row) {
        const auto old_size = size();
        try { Push(std::move(row), std::index_sequence_for<Cells...>{}); }
        catch (...) {
            std::apply([&](auto&... cols) { ((cols.size() > old_size ? cols.pop_back() : void()), ...); }, columns_);
            throw;
        }
    }
    void append_from(const Table& source) {
        assert(this != &source);
        const auto old_size = size();
        try { Append(source, std::index_sequence_for<Cells...>{}); }
        catch (...) {
            std::apply([&](auto&... cols) { (cols.erase(cols.begin() + old_size, cols.end()), ...); }, columns_);
            throw;
        }
    }

    // Gather from an independent source, retaining vector and owning-cell
    // capacity. PositionAt must return the same source row for a given index
    // throughout the call. Repeated positions and partial gathers are allowed.
    template <typename PositionAt>
    void assign_rows_from(const Table& source, std::size_t count, PositionAt position_at) {
        if (this == &source) throw std::invalid_argument("SoA gather source and target must differ");
        for (std::size_t row = 0; row < count; ++row) {
            const auto position = position_at(row);
            if (!std::in_range<std::size_t>(position) || static_cast<std::size_t>(position) >= source.size()) {
                throw std::out_of_range("SoA gather row");
            }
        }
        try { Gather(source, count, position_at, std::index_sequence_for<Cells...>{}); }
        catch (...) { clear(); throw; }
    }

    iterator begin() { return {*this, 0}; }
    iterator end() { return {*this, static_cast<std::ptrdiff_t>(size())}; }
    const_iterator begin() const { return {*this, 0}; }
    const_iterator end() const { return {*this, static_cast<std::ptrdiff_t>(size())}; }
    moving_iterator move_begin() { return {*this, 0}; }
    moving_iterator move_end() { return {*this, static_cast<std::ptrdiff_t>(size())}; }

private:
    template <std::size_t... Columns>
    void Push(row_type&& row, std::index_sequence<Columns...>) {
        (std::get<Columns>(columns_).push_back(std::get<Columns>(std::move(row))), ...);
    }
    template <std::size_t... Columns>
    void Append(const Table& source, std::index_sequence<Columns...>) {
        (std::get<Columns>(columns_).insert(std::get<Columns>(columns_).end(),
                                         std::get<Columns>(source.columns_).begin(),
                                         std::get<Columns>(source.columns_).end()), ...);
    }
    template <std::size_t Column, typename PositionAt>
    void GatherColumn(const Table& source, std::size_t count, PositionAt position_at) {
        auto& target = std::get<Column>(columns_);
        const auto& input = std::get<Column>(source.columns_);
        target.resize(count);
        for (std::size_t row = 0; row < count; ++row) target[row] = input[static_cast<std::size_t>(position_at(row))];
    }
    template <typename PositionAt, std::size_t... Columns>
    void Gather(const Table& source, std::size_t count, PositionAt position_at,
                std::index_sequence<Columns...>) {
        (GatherColumn<Columns>(source, count, position_at), ...);
    }
    std::tuple<std::vector<Cells>...> columns_;
};

// Works for both row proxies and owning tuple temporaries held by std::sort.
template <std::size_t Column, typename Row>
decltype(auto) Key(const Row& row) {
    if constexpr (requires { row.template get<Column>(); }) return row.template get<Column>();
    else return std::get<Column>(row);
}

template <std::size_t Column>
struct Less {
    template <typename Left, typename Right>
    bool operator()(const Left& left, const Right& right) const {
        return Key<Column>(left) < Key<Column>(right);
    }
};

template <std::size_t Column, typename... Cells>
void Sort(Table<Cells...>& table) {
    std::sort(table.move_begin(), table.move_end(), Less<Column>{});
}

}  // namespace soa_rows
