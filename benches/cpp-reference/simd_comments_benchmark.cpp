#include <algorithm>
#include <array>
#include <chrono>
#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <memory>
#include <span>
#include <string>
#include <string_view>
#include <vector>

#include "gd_table_simd.h"

namespace {

constexpr std::size_t kWarmupBytes = 32 * 1024 * 1024;
constexpr std::size_t kTimingBytes = 256 * 1024 * 1024;
constexpr std::size_t kSampleCount = 9;

using Table = gd::table::simd::table_8_8;

std::string GetCode(unsigned index) {
    switch (index) {
    case 0:
        return "-- This is a single line comment at the start\nlocal x = 5\nprint(x)\n-- Final "
               "comment";
    case 1:
        return "local a = 1\n-- First comment\nlocal b = 2\n-- Second comment\nlocal c = a + b\n-- "
               "Third comment\nprint(c)\n-- Fourth";
    case 2:
        return "-- This is a very long comment that will definitely span across pack boundaries in "
               "the table because we need to test the cross-pack handling logic properly "
               "here\nlocal y = 10\nprint(y)";
    case 3:
        return "local val1 = 100\nlocal val2 = 200\nlocal sum = val1 + val2\nlocal diff = val2 - "
               "val1\nlocal prod = val1 * val2\nlocal quot = val2 / val1\nprint(sum, diff, prod, "
               "quot)";
    case 4:
        return "-- Comment one\n-- Comment two\n-- Comment three\n-- Comment four\n-- Comment "
               "five\n-- All comments no code";
    case 5:
        return "local x = 1\nlocal y = 2\nlocal z = 3\nlocal w = 4\nlocal v = 5\nlocal u = "
               "6\nlocal t = 7\n-- ";
    case 6:
        return "local config = {\n  name = \"test\",\n  -- debug mode enabled\n  enabled = true,\n "
               " -- version tracking\n  version = 1,\n  settings = {\n    -- timeout in seconds\n  "
               "  timeout = 30,\n    retries = 3\n  }\n}\n-- End of configuration\nreturn config";
    case 7:
        return "-- Module initialization\nlocal module = {}\n-- Constants "
               "section\nmodule.MAX_VALUE = 1000\nmodule.MIN_VALUE = 0\nmodule.DEFAULT = 500\n-- "
               "Function definitions\nfunction module.process(data)\n  -- Validate input first\n  "
               "if not data then return nil end\n  -- Process the data\n  local result = data * "
               "module.MAX_VALUE\n  -- Apply constraints\n  if result > module.MAX_VALUE then\n    "
               "result = module.MAX_VALUE\n  end\n  return result\nend\n-- Export module\nreturn "
               "module";
    case 8:
        return "local Class = require(\"Class\")\n-- Player class extends base Class\nlocal Player "
               "= Class:extends()\n-- Constructor with default values\nfunction Player:init(name, "
               "health)\n  self.name = name\n  self.health = health or 100\n  self.maxHealth = "
               "100\nend\n-- Take damage method\nfunction Player:takeDamage(amount)\n  -- Clamp "
               "damage to prevent negative health\n  self.health = math.max(0, self.health - "
               "amount)\n  -- Check for death\n  if self.health == 0 then\n    self:onDeath()\n  "
               "end\nend\n-- Return player instance\nreturn Player";
    case 9:
        return "local t={a=1,b=2,c=3}\n-- table comment\nfor k,v in pairs(t)do print(k,v)end\n-- "
               "end loop\nlocal sum=0\nfor i=1,#t do sum=sum+t[i]end\nprint(sum)";
    case 10:
        return "-- Configuration loader module with extensive documentation and multiple comment "
               "blocks\nlocal ConfigLoader = {}\n\n-- Private helper functions section\nlocal "
               "function parseFile(filename)\n  -- Open file for reading with error handling\n  "
               "local file = io.open(filename, \"r\")\n  if not file then return nil, \"Could not "
               "open file\" end\n  -- Read all content into memory\n  local content = "
               "file:read(\"*all\")\n  file:close()\n  return content\nend\n\nlocal function "
               "validateConfig(config)\n  -- Check required fields exist\n  if not config.name "
               "then return false, \"Missing name field\" end\n  if not config.version then return "
               "false, \"Missing version field\" end\n  return true, nil\nend\n\n-- Public API "
               "functions\nfunction ConfigLoader.load(filepath)\n  -- Parse the file contents\n  "
               "local rawContent, err = parseFile(filepath)\n  if err then return nil, err end\n  "
               "-- TODO: Actually parse the config\n  return {name=\"default\", version=1}, "
               "nil\nend\n\nreturn ConfigLoader";
    case 11:
        return "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA-";
    case 12:
        return "local x = 1 -- inline comment\nlocal y = 2\n--- triple dash comment\nlocal z = "
               "3\n--\n-- double empty line comment\n--\nprint(x,y,z)";
    case 13:
        return "local greeting = \"Hello World!\"\nlocal unicode = \"Üñíçödé Tëst\"  -- test "
               "unicode support\nlocal emoji = \"👨‍💻\"  -- emoji in "
               "comment\nprint(greeting, "
               "unicode, emoji)";
    case 14:
        return "local level1 = {\n  -- Level 1 comment\n  level2 = {\n    -- Level 2 comment\n    "
               "level3 = {\n      -- Level 3 comment\n      data = \"deeply nested\",\n      -- "
               "More data\n      value = 42\n    }\n  }\n}\n-- End of nesting\nreturn level1";
    case 15:
        return "-- Array initialization\nlocal array = {}--comment\nfor i = 1, 10 do--loop "
               "comment\n  array[i] = i * 2--calculation\n  -- print(array[i])\nend\n-- "
               "Processing\nlocal total = 0\nfor _,v in ipairs(array)do total = total + v end--sum "
               "calculation\nprint(total)--output result\n-- Done";
    default:
        return {};
    }
}

struct Fixture {
    std::string code;
    std::unique_ptr<Table> table;
};

Fixture MakeFixture(unsigned index) {
    Fixture fixture{
        GetCode(index), std::make_unique<Table>(100u, gd::table::tag_repare_to_add_column{})};
    fixture.table->column_add("uint64", 0, "code");
    const auto prepared = fixture.table->prepare();
    if (!prepared.first) {
        std::abort();
    }
    fixture.table->row_clear();
    fixture.table->pack_plant_span<char>(std::span<const char>(fixture.code), 0, '\0');
    return fixture;
}

std::string CleanHybrid(Fixture& fixture) {
    Table& table = *fixture.table;
    const std::string_view code = fixture.code;
    std::string cleaned;
    cleaned.reserve(code.size());
    bool in_comment = false;
    Table::position end;
    end.advance(static_cast<unsigned>(code.size()));
    constexpr unsigned pack_bytes = Table::size_pack_s();
    const std::uint64_t full_pack_count = code.size() / pack_bytes;
    const unsigned tail_byte_count = static_cast<unsigned>(code.size() % pack_bytes);

    const auto scan_simple = [&](Table::position position, Table::position range_end) {
        while (position < range_end) {
            const std::uint8_t byte = table.get_uint8(position);
            if (in_comment) {
                position.advance(1);
                if (byte == '\n') {
                    in_comment = false;
                }
                continue;
            }
            if (byte == '-') {
                Table::position next = position;
                next.advance(1);
                if (next < end && table.get_uint8(next) == '-') {
                    in_comment = true;
                    position = next;
                    position.advance(1);
                    continue;
                }
            }
            cleaned.push_back(static_cast<char>(byte));
            position.advance(1);
        }
    };

    for (std::uint64_t pack = 0; pack < full_pack_count; ++pack) {
        if (in_comment) {
            if (table.pack_find_value<char>(pack, 0, '\n') == 0) {
                continue;
            }
        } else if (table.pack_find_value<char>(pack, 0, '-') == 0) {
            const auto pack_span = table.pack_harvest_span<char>(pack, 0);
            cleaned.append(pack_span.data(), pack_span.size());
            continue;
        }

        Table::position pack_start;
        pack_start.m_uRow = pack * Table::count_pack_s();
        Table::position pack_end = pack_start;
        pack_end.advance(pack_bytes);
        scan_simple(pack_start, pack_end);
    }

    if (tail_byte_count > 0) {
        Table::position tail_start;
        tail_start.m_uRow = full_pack_count * Table::count_pack_s();
        Table::position tail_end = tail_start;
        tail_end.advance(tail_byte_count);
        scan_simple(tail_start, tail_end);
    }
    return cleaned;
}

std::string CleanScalarTable(Fixture& fixture) {
    Table& table = *fixture.table;
    std::string cleaned;
    cleaned.reserve(fixture.code.size());
    bool in_comment = false;
    Table::position position;
    Table::position end;
    end.advance(static_cast<unsigned>(fixture.code.size()));
    while (position < end) {
        const std::uint8_t byte = table.get_uint8(position);
        if (in_comment) {
            position.advance(1);
            if (byte == '\n') {
                in_comment = false;
            }
            continue;
        }
        if (byte == '-') {
            Table::position next = position;
            next.advance(1);
            if (next < end && table.get_uint8(next) == '-') {
                in_comment = true;
                position = next;
                position.advance(1);
                continue;
            }
        }
        cleaned.push_back(static_cast<char>(byte));
        position.advance(1);
    }
    return cleaned;
}

std::string CleanStandard(Fixture& fixture) {
    const std::string_view code = fixture.code;
    std::string cleaned;
    cleaned.reserve(code.size());
    std::size_t line_start = 0;
    while (line_start < code.size()) {
        const std::size_t newline = code.find('\n', line_start);
        const std::size_t line_end = newline == std::string_view::npos ? code.size() : newline + 1;
        const std::size_t comment = code.find("--", line_start);
        const std::size_t copy_end = comment < line_end ? comment : line_end;
        cleaned.append(code.data() + line_start, copy_end - line_start);
        line_start = line_end;
    }
    return cleaned;
}

using Cleaner = std::string (*)(Fixture&);

std::size_t IterationsFor(std::size_t logical_bytes, std::size_t corpus_bytes) {
    return (logical_bytes / corpus_bytes) +
           static_cast<std::size_t>(logical_bytes % corpus_bytes != 0);
}

std::size_t RunIterations(Cleaner cleaner, std::vector<Fixture>& fixtures, std::size_t iterations) {
    std::size_t checksum = 0;
    for (std::size_t iteration = 0; iteration < iterations; ++iteration) {
        for (Fixture& fixture : fixtures) {
            const std::string output = cleaner(fixture);
            checksum += output.size();
        }
    }
#if defined(__GNUC__) || defined(__clang__)
    asm volatile("" : "+r"(checksum) : : "memory");
#endif
    return checksum;
}

void Measure(std::string_view name, Cleaner cleaner, std::vector<Fixture>& fixtures,
    std::size_t corpus_bytes) {
    const std::size_t warmup_iterations = IterationsFor(kWarmupBytes, corpus_bytes);
    const std::size_t timing_iterations = IterationsFor(kTimingBytes, corpus_bytes);
    const std::size_t warmup_checksum = RunIterations(cleaner, fixtures, warmup_iterations);
    (void)warmup_checksum;
    std::array<double, kSampleCount> samples{};
    for (std::size_t sample = 0; sample < samples.size(); ++sample) {
        const auto start = std::chrono::steady_clock::now();
        const std::size_t checksum = RunIterations(cleaner, fixtures, timing_iterations);
        const auto stop = std::chrono::steady_clock::now();
        const std::size_t bytes = timing_iterations * corpus_bytes;
        samples[sample] = std::chrono::duration<double, std::nano>(stop - start).count() /
                          static_cast<double>(bytes);
        std::printf("mode=cpp-%.*s sample=%zu ns_per_byte=%.6f checksum=%zu\n",
            static_cast<int>(name.size()), name.data(), sample + 1, samples[sample], checksum);
    }
    std::sort(samples.begin(), samples.end());
    std::printf("mode=cpp-%.*s median_ns_per_byte=%.6f\n", static_cast<int>(name.size()),
        name.data(), samples[samples.size() / 2]);
}

} // namespace

int main(int argc, char** argv) {
    if (argc > 2) {
        std::fprintf(stderr, "usage: simd_comments_benchmark [all|hybrid|scalar-table|standard]\n");
        return 2;
    }
    const std::string_view mode = argc == 2 ? argv[1] : "all";
    std::vector<Fixture> fixtures;
    fixtures.reserve(16);
    for (unsigned index = 0; index < 16; ++index) {
        fixtures.push_back(MakeFixture(index));
    }
    for (Fixture& fixture : fixtures) {
        const std::string expected = CleanStandard(fixture);
        if (CleanHybrid(fixture) != expected || CleanScalarTable(fixture) != expected) {
            std::abort();
        }
    }
    std::size_t corpus_bytes = 0;
    for (const Fixture& fixture : fixtures) {
        corpus_bytes += fixture.code.size();
    }

    if (mode == "all" || mode == "hybrid") {
        Measure("hybrid", CleanHybrid, fixtures, corpus_bytes);
    }
    if (mode == "all" || mode == "scalar-table") {
        Measure("scalar-table", CleanScalarTable, fixtures, corpus_bytes);
    }
    if (mode == "all" || mode == "standard") {
        Measure("standard", CleanStandard, fixtures, corpus_bytes);
    }
    if (mode != "all" && mode != "hybrid" && mode != "scalar-table" && mode != "standard") {
        std::fprintf(stderr, "unknown mode: %.*s\n", static_cast<int>(mode.size()), mode.data());
        return 2;
    }
}
