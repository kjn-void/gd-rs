#include "soa_table_row_iterator.h"
#include "soa_table_key_sort.h"

#include <cstdint>
#include <iostream>
#include <memory>
#include <numeric>
#include <optional>
#include <random>
#include <stdexcept>
#include <string>
#include <vector>

namespace {
void Require(bool condition) {
    if (!condition) throw std::runtime_error("SoA row iterator verification failed");
}

using Table = soa_rows::Table<std::uint64_t, std::string, double, std::optional<double>,
                              std::optional<std::int32_t>, std::optional<std::uint32_t>, std::string>;
static_assert(!std::is_assignable_v<Table::const_iterator::reference, Table::row_type>);

Table MakeSource(std::size_t count) {
    Table source;
    source.reserve(count);
    for (std::size_t id = 0; id < count; ++id) {
        const auto key = (id * 7) % 13;
        source.push_back({id, std::string(key, 'a'), static_cast<double>(key),
                          id % 3 ? std::optional<double>(key) : std::nullopt,
                          id % 4 ? std::optional<std::int32_t>(static_cast<std::int32_t>(key) - 8) : std::nullopt,
                          id % 5 ? std::optional<std::uint32_t>(key) : std::nullopt,
                          std::string(80, 'z') + std::to_string(id)});
    }
    return source;
}

template <std::size_t Column>
void VerifyColumn(std::size_t count) {
    const Table original = MakeSource(count);
    Table target;
    target.reserve(count);
    for (int pass = 0; pass < 3; ++pass) {
        target.clear();
        target.append_from(original);
        // Shuffle through the ordinary (copying) iterator, then sort through
        // its explicit moving adaptor. Every cell must follow the row ID.
        std::mt19937 random(42 + pass);
        std::shuffle(target.begin(), target.end(), random);
        soa_rows::Sort<Column>(target);
        Require(std::is_sorted(target.begin(), target.end(), soa_rows::Less<Column>{}));
        std::vector<bool> seen(count);
        for (std::size_t row = 0; row < count; ++row) {
            const auto id = target.cell<0>(row);
            Require(id < count && !seen[id]);
            seen[id] = true;
            const Table::row_type actual = target.begin()[row];
            Require(actual == Table::row_type(original.cell<0>(id), original.cell<1>(id),
                                             original.cell<2>(id), original.cell<3>(id),
                                             original.cell<4>(id), original.cell<5>(id), original.cell<6>(id)));
        }
    }
}

void VerifyReadsAndWrites() {
    Table target = MakeSource(17);
    const Table original = target;
    Table::row_type snapshot = *target.begin();
    Require(std::get<6>(snapshot) == original.cell<6>(0));
    Require(target.cell<6>(0) == original.cell<6>(0)); // Reading must not move strings out.
    target.begin()[1] = *target.begin();
    Require(target.cell<6>(0) == original.cell<6>(0)); // Ordinary assignment copies.
    Require(target.cell<6>(1) == original.cell<6>(0));
    target.begin()[2] = std::move(snapshot);
    Require(target.cell<6>(2) == original.cell<6>(0));
    std::iter_swap(target.begin(), target.begin() + 3);
    Require(target.cell<0>(0) == 3 && target.cell<0>(3) == 0);
    auto moved = std::ranges::iter_move(target.begin() + 4);
    Table::row_type owned = moved;
    Require(std::get<6>(owned) == original.cell<6>(4));
    target.begin()[4] = std::move(owned);
    Require(target.cell<6>(4) == original.cell<6>(4));
    target.begin()[4] = std::ranges::iter_move(target.begin() + 4); // Self move is harmless.
    Require(target.cell<6>(4) == original.cell<6>(4));

    auto iterator = target.begin();
    Require(target.end() - iterator == 17 && iterator < target.end());
    Require((iterator + 7) - 3 == 4 + iterator && (iterator + 7)[-3].get<0>() == 4);
    Require((iterator++) == target.begin() && (--iterator) == target.begin());
    Require((++iterator)-- == target.begin() + 1 && iterator == target.begin());
}

// A move-only payload makes accidental row copies during sorting a compile
// error. It also verifies that the iterator is generic over column types.
void VerifyMoveOnly() {
    soa_rows::Table<int, std::unique_ptr<int>, std::string> target;
    for (int id = 0; id < 257; ++id) {
        target.push_back({256 - id, std::make_unique<int>(256 - id), std::string(80, 'a') + std::to_string(256 - id)});
    }
    soa_rows::Sort<0>(target);
    for (int id = 0; id < 257; ++id) {
        Require(target.cell<0>(id) == id && *target.cell<1>(id) == id);
        Require(target.cell<2>(id) == std::string(80, 'a') + std::to_string(id));
    }
}

std::vector<Table::row_type> Snapshot(const Table& table) {
    std::vector<Table::row_type> rows;
    for (auto row : table) rows.push_back(static_cast<Table::row_type>(row));
    return rows;
}

template <std::size_t Column>
void VerifyCachedColumn(std::size_t count) {
    Table source = MakeSource(count);
    if constexpr (Column >= 3 && Column <= 5) {
        for (std::size_t row = 0; row < count; ++row) {
            if (!source.cell<Column>(row)) source.cell<Column>(row) = (row * 7) % 13;
        }
    }
    const auto original = Snapshot(source);
    Table target;
    target.reserve(count);
    soa_rows::KeySortWorkspace<Column, Table> workspace(count);
    std::vector<const char*> output_buffers(count);
    for (int pass = 0; pass < 3; ++pass) {
        workspace.sort_into(source, target);
        Require(target.size() == count);
        Require(std::is_sorted(target.begin(), target.end(), soa_rows::Less<Column>{}));
        std::vector<bool> seen(count);
        for (std::size_t row = 0; row < count; ++row) {
            const auto id = target.cell<0>(row);
            Require(id < count && !seen[id]);
            seen[id] = true;
            Require(static_cast<Table::row_type>(target.begin()[row]) == original[id]);
            Require(target.cell<6>(row).data() != source.cell<6>(id).data());
            // The fixed source/order reuses the destination's owning heap text.
            if (pass != 0) Require(target.cell<6>(row).data() == output_buffers[row]);
            output_buffers[row] = target.cell<6>(row).data();
        }
        Require(Snapshot(source) == original);
        if (count != 0) target.cell<0>(0) = UINT64_MAX; // Every field must be overwritten next time.
    }
}

template <std::size_t Column>
void VerifyNullRejection() {
    Table source = MakeSource(17);
    for (std::size_t row = 0; row < source.size(); ++row) source.cell<Column>(row) = 7;
    source.cell<Column>(0) = std::nullopt;
    Table target = MakeSource(2);
    const auto before = Snapshot(target);
    soa_rows::KeySortWorkspace<Column, Table> workspace(17);
    bool rejected = false;
    try { workspace.sort_into(source, target); }
    catch (const std::invalid_argument&) { rejected = true; }
    Require(rejected && Snapshot(target) == before);
    source.cell<Column>(0) = 7;
    workspace.sort_into(source, target);
    Require(target.size() == 17); // A rejected key does not poison workspace reuse.
    rejected = false;
    try { workspace.sort_into(source, source); }
    catch (const std::invalid_argument&) { rejected = true; }
    Require(rejected);
}

void VerifyGather() {
    const Table source = MakeSource(5);
    const auto original = Snapshot(source);
    Table target = MakeSource(2);
    const std::size_t positions[] = {4, 0, 4, 1};
    target.assign_rows_from(source, 4, [&](std::size_t row) { return positions[row]; });
    Require(Snapshot(target) == std::vector<Table::row_type>({original[4], original[0], original[4], original[1]}));
    const auto before = Snapshot(target);
    bool rejected = false;
    try { target.assign_rows_from(source, 1, [](std::size_t) { return 5; }); }
    catch (const std::out_of_range&) { rejected = true; }
    Require(rejected && Snapshot(target) == before);
    rejected = false;
    try { target.assign_rows_from(source, 1, [](std::size_t) { return -1; }); }
    catch (const std::out_of_range&) { rejected = true; }
    Require(rejected && Snapshot(target) == before);
    rejected = false;
    try { target.assign_rows_from(target, 1, [](std::size_t) { return 0; }); }
    catch (const std::invalid_argument&) { rejected = true; }
    Require(rejected);
    target.assign_rows_from(source, 0, [](std::size_t) { return 0; });
    Require(target.size() == 0 && Snapshot(source) == original);
}
}  // namespace

int main() {
    try {
        VerifyReadsAndWrites();
        VerifyMoveOnly();
        VerifyGather();
        VerifyNullRejection<3>(); VerifyNullRejection<4>(); VerifyNullRejection<5>();
        for (auto count : {0, 1, 2, 17, 128, 10000}) {
            VerifyColumn<0>(count); VerifyColumn<1>(count); VerifyColumn<2>(count);
            VerifyColumn<3>(count); VerifyColumn<4>(count); VerifyColumn<5>(count);
            VerifyColumn<6>(count);
            VerifyCachedColumn<0>(count); VerifyCachedColumn<1>(count); VerifyCachedColumn<2>(count);
            VerifyCachedColumn<3>(count); VerifyCachedColumn<4>(count); VerifyCachedColumn<5>(count);
            VerifyCachedColumn<6>(count);
        }
        std::cout << "SoA iterator and cached-key materialization verified (7 columns, nullable/owning/move-only cells, output reuse)\n";
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
