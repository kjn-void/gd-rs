// Focused API diagnostics, separate from the timed workload and unmodified GD.
#include <cstdint>
#include <iostream>
#include <memory>
#include <string_view>
#include <tuple>
#include <vector>
#include "gd_table_column-buffer.h"
#include "gd_table_index.h"

int main(int argc, char** argv) {
    if(argc != 2) return 2;
    const std::string_view mode = argv[1];
    if(mode == "index-miss") {
        gd::table::index_int64 index;
        index.add(gd::variant_view(std::int64_t{10}), 0);
        index.add(gd::variant_view(std::int64_t{30}), 1);
        index.sort();
        const auto [hit, row] = index.find(20);
        std::cout << "missing_key=20 hit=" << hit << " row=" << row << '\n';
        return 0;
    }
    if(mode == "name-view") {
        // A valid string_view need not include a trailing NUL in its allocation.
        const auto bytes = std::make_unique<char[]>(2);
        bytes[0] = 'i'; bytes[1] = 'd';
        gd::table::table_column_buffer table;
        table.column_add(std::vector<std::tuple<std::string_view, unsigned, std::string_view>>{
            {"int64", 0, std::string_view(bytes.get(), 2)}}, gd::table::tag_type_name{});
        const auto result = table.prepare();
        std::cout << "prepared=" << result.first << '\n';
        return result.first ? 0 : 1;
    }
    return 2;
}
