// GD reference cb11cff90d05260a88d59a30c9da421cd4e19c34.
// Native gd::table::table shares column definitions, not cell buffers.
// Projection below is an application view over one shared GD member table.
#include <cstdint>
#include <iostream>
#include <memory>
#include <stdexcept>
#include <string_view>
#include <utility>
#include <vector>

#include "gd_table_table.h"

using Table = gd::table::table;
using Columns = gd::table::detail::columns;

constexpr std::string_view ada = "Ada Lovelace customer record";
constexpr std::string_view grace = "Grace Hopper customer record";

void require(bool condition, const char* message) {
    if (!condition) {
        throw std::runtime_error(message);
    }
}

std::shared_ptr<Table> make_table() {
    auto table = std::make_shared<Table>(10u, 0u);
    table->column_prepare();
    table->column_add("uint64", "key");
    table->column_add("rstring", "name");
    table->column_add("uint64", "qty");
    table->column_add("uint64", "cost");
    const auto [ok, error] = table->prepare();
    if (!ok) {
        throw std::runtime_error(error);
    }

    table->row_add({std::uint64_t{1}, ada, std::uint64_t{10}, std::uint64_t{100}});
    table->row_add({std::uint64_t{2}, grace, std::uint64_t{20}, std::uint64_t{200}});
    return table;
}

// This class belongs to the example, not to GD. It retains the actual table and
// maps view-relative column positions to the source's columns. It never owns a
// second GD data buffer. Reads resolve the current cell each time.
class Projection {
    std::shared_ptr<Table> source_;
    std::vector<unsigned> columns_;

    unsigned source_column(std::uint64_t row, unsigned column) const {
        if (row >= source_->get_row_count()) {
            throw std::out_of_range("projection row");
        }
        return columns_.at(column);
    }

public:
    Projection(std::shared_ptr<Table> source, std::vector<unsigned> columns)
        : source_(std::move(source)), columns_(std::move(columns)) {
        require(source_ != nullptr, "missing projection source");
        for (auto column : columns_) {
            require(column < source_->get_column_count(), "invalid projection column");
        }
    }

    gd::variant_view get(std::uint64_t row, unsigned column) const {
        return source_->cell_get_variant_view(row, source_column(row, column));
    }

    const std::uint8_t* address(std::uint64_t row, unsigned column) const {
        return source_->cell_get(row, source_column(row, column));
    }

    void set_u64(std::uint64_t row, unsigned column, std::uint64_t value) {
        const auto source_index = source_column(row, column);
        require(
            source_->column_get_ctype_number(source_index) == gd::variant_view(value).type_number(),
            "projection value type mismatch");
        source_->cell_set(row, source_index, gd::variant_view(value));
    }
};

struct ReleaseColumns {
    void operator()(Columns* columns) const {
        columns->release();
    }
};

int main(int argc, char** argv) {
    try {
        auto source = make_table();

        // Run this in a separate process: native append asserts that the source
        // and destination have the same column count, so this is not a view API.
        if (argc == 2 && std::string_view(argv[1]) == "--native-subset-constructor") {
            unsigned selected[] = {0, 2};
            std::unique_ptr<Columns, ReleaseColumns> subset_columns(
                Table::new_columns_s(source->get_columns(), selected, 2));
            Table attempt(subset_columns.get(), source.get(), 0, source->get_row_count());
            throw std::runtime_error("subset constructor unexpectedly accepted unequal widths");
        }

        // Native sharing: the same schema, but a separate cell allocation.
        Table independent(source->get_columns(), 0u, 0u, 0u);
        independent.row_add({std::uint64_t{1}, ada, std::uint64_t{500}, std::uint64_t{100}});
        require(independent.get_columns() == source->get_columns(), "schema is not shared");
        require(independent.cell_get(0, 2) != source->cell_get(0, 2), "cells unexpectedly alias");
        require(source->cell_get<std::uint64_t>(0, 2) == 10, "independent table changed source");
        std::cout << "GD native: shared schema, independent cells\n";

        // The source-table constructor accepts the full schema and copies cells.
        Table native_copy(source->get_columns(), source.get(), 0, source->get_row_count());
        require(native_copy.cell_get(0, 2) != source->cell_get(0, 2),
            "source-table constructor unexpectedly aliases cells");

        // Native column subset: new_columns_s copies selected definitions.
        // Preparing and appending constructs an independently owned projection.
        unsigned selected[] = {0, 2};
        std::unique_ptr<Columns, ReleaseColumns> subset_columns(
            Table::new_columns_s(source->get_columns(), selected, 2));
        Table copied(2u, 0u);
        copied.set_columns(subset_columns.get());
        const auto [ok, error] = copied.prepare();
        if (!ok) {
            throw std::runtime_error(error);
        }
        copied.append(*source, std::vector<unsigned>{0, 2}, std::vector<unsigned>{0, 1});
        require(copied.get_column_count() == 2 && copied.get_row_count() == 2,
            "wrong native projection dimensions");
        require(copied.cell_get<std::uint64_t>(0, 1) == 10, "wrong copied quantity");
        require(
            copied.cell_get(0, 1) != source->cell_get(0, 2), "native projection aliases source");
        std::cout << "GD native: selected columns produce a copied table\n";

        // Application projection views: different, overlapping column subsets.
        Projection a(source, {0, 1, 2}); // key, name, qty
        Projection b(source, {1, 2, 3}); // name, qty, cost
        require(a.address(0, 2) == b.address(0, 1), "quantity storage is not shared");
        require(a.get(0, 1).as_string_view().data() == b.get(0, 0).as_string_view().data(),
            "name storage is not shared");
        require(a.get(1, 1).as_string_view() == grace, "wrong second row name");

        a.set_u64(0, 2, 99);
        require(source->cell_get<std::uint64_t>(0, 2) == 99, "source did not see view write");
        require(b.get(0, 1).as_uint64() == 99, "other view did not see write");
        require(copied.cell_get<std::uint64_t>(0, 1) == 10, "copied projection changed");
        require(
            native_copy.cell_get<std::uint64_t>(0, 2) == 10, "native source-table copy changed");
        std::cout << "GD views: A=[key,name,qty], B=[name,qty,cost], shared cell storage\n";
        std::cout << "GD views: A/source/B qty=99; copied projection qty=10\n";

        source.reset(); // Both views retain the GD table through std::shared_ptr.
        require(b.get(0, 1).as_uint64() == 99, "view lost its source lifetime");
        std::cout << "GD views: source handle dropped; views remain valid\n";
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
