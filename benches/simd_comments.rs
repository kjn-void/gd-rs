//! Matched gd-rs `U8` `SoA` benchmark for the C++ GD pack-oriented comment scanner.

use std::{hint::black_box, sync::Arc, time::Instant};

use gd::{ColumnSpec, DataType, Schema, Table, Value};

const PACK_BYTES: usize = 64;
const WARMUP_BYTES: usize = 32 * 1024 * 1024;
const TIMING_BYTES: usize = 256 * 1024 * 1024;
const SAMPLE_COUNT: usize = 9;

struct Fixture {
    code: &'static str,
    table: Table,
}

impl Fixture {
    fn values(&self) -> &[u8] {
        self.table.column(0).unwrap().as_slice::<u8>().unwrap()
    }
}

type Cleaner = fn(&Fixture) -> Vec<u8>;

fn get_code(index: usize) -> &'static str {
    match index {
        0 => {
            "-- This is a single line comment at the start\nlocal x = 5\nprint(x)\n-- Final comment"
        }
        1 => {
            "local a = 1\n-- First comment\nlocal b = 2\n-- Second comment\nlocal c = a + b\n-- Third comment\nprint(c)\n-- Fourth"
        }
        2 => {
            "-- This is a very long comment that will definitely span across pack boundaries in the table because we need to test the cross-pack handling logic properly here\nlocal y = 10\nprint(y)"
        }
        3 => {
            "local val1 = 100\nlocal val2 = 200\nlocal sum = val1 + val2\nlocal diff = val2 - val1\nlocal prod = val1 * val2\nlocal quot = val2 / val1\nprint(sum, diff, prod, quot)"
        }
        4 => {
            "-- Comment one\n-- Comment two\n-- Comment three\n-- Comment four\n-- Comment five\n-- All comments no code"
        }
        5 => {
            "local x = 1\nlocal y = 2\nlocal z = 3\nlocal w = 4\nlocal v = 5\nlocal u = 6\nlocal t = 7\n-- "
        }
        6 => {
            "local config = {\n  name = \"test\",\n  -- debug mode enabled\n  enabled = true,\n  -- version tracking\n  version = 1,\n  settings = {\n    -- timeout in seconds\n    timeout = 30,\n    retries = 3\n  }\n}\n-- End of configuration\nreturn config"
        }
        7 => {
            "-- Module initialization\nlocal module = {}\n-- Constants section\nmodule.MAX_VALUE = 1000\nmodule.MIN_VALUE = 0\nmodule.DEFAULT = 500\n-- Function definitions\nfunction module.process(data)\n  -- Validate input first\n  if not data then return nil end\n  -- Process the data\n  local result = data * module.MAX_VALUE\n  -- Apply constraints\n  if result > module.MAX_VALUE then\n    result = module.MAX_VALUE\n  end\n  return result\nend\n-- Export module\nreturn module"
        }
        8 => {
            "local Class = require(\"Class\")\n-- Player class extends base Class\nlocal Player = Class:extends()\n-- Constructor with default values\nfunction Player:init(name, health)\n  self.name = name\n  self.health = health or 100\n  self.maxHealth = 100\nend\n-- Take damage method\nfunction Player:takeDamage(amount)\n  -- Clamp damage to prevent negative health\n  self.health = math.max(0, self.health - amount)\n  -- Check for death\n  if self.health == 0 then\n    self:onDeath()\n  end\nend\n-- Return player instance\nreturn Player"
        }
        9 => {
            "local t={a=1,b=2,c=3}\n-- table comment\nfor k,v in pairs(t)do print(k,v)end\n-- end loop\nlocal sum=0\nfor i=1,#t do sum=sum+t[i]end\nprint(sum)"
        }
        10 => {
            "-- Configuration loader module with extensive documentation and multiple comment blocks\nlocal ConfigLoader = {}\n\n-- Private helper functions section\nlocal function parseFile(filename)\n  -- Open file for reading with error handling\n  local file = io.open(filename, \"r\")\n  if not file then return nil, \"Could not open file\" end\n  -- Read all content into memory\n  local content = file:read(\"*all\")\n  file:close()\n  return content\nend\n\nlocal function validateConfig(config)\n  -- Check required fields exist\n  if not config.name then return false, \"Missing name field\" end\n  if not config.version then return false, \"Missing version field\" end\n  return true, nil\nend\n\n-- Public API functions\nfunction ConfigLoader.load(filepath)\n  -- Parse the file contents\n  local rawContent, err = parseFile(filepath)\n  if err then return nil, err end\n  -- TODO: Actually parse the config\n  return {name=\"default\", version=1}, nil\nend\n\nreturn ConfigLoader"
        }
        11 => "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA-",
        12 => {
            "local x = 1 -- inline comment\nlocal y = 2\n--- triple dash comment\nlocal z = 3\n--\n-- double empty line comment\n--\nprint(x,y,z)"
        }
        13 => {
            "local greeting = \"Hello World!\"\nlocal unicode = \"Üñíçödé Tëst\"  -- test unicode support\nlocal emoji = \"👨‍💻\"  -- emoji in comment\nprint(greeting, unicode, emoji)"
        }
        14 => {
            "local level1 = {\n  -- Level 1 comment\n  level2 = {\n    -- Level 2 comment\n    level3 = {\n      -- Level 3 comment\n      data = \"deeply nested\",\n      -- More data\n      value = 42\n    }\n  }\n}\n-- End of nesting\nreturn level1"
        }
        15 => {
            "-- Array initialization\nlocal array = {}--comment\nfor i = 1, 10 do--loop comment\n  array[i] = i * 2--calculation\n  -- print(array[i])\nend\n-- Processing\nlocal total = 0\nfor _,v in ipairs(array)do total = total + v end--sum calculation\nprint(total)--output result\n-- Done"
        }
        _ => "",
    }
}

#[inline]
fn find_value_mask(pack: &[u8], value: u8) -> u64 {
    debug_assert_eq!(pack.len(), PACK_BYTES);
    let mut mask = 0_u64;
    for (position, &byte) in pack.iter().enumerate() {
        mask |= u64::from(byte == value) << position;
    }
    mask
}

fn scan_simple(
    input: &[u8],
    range: std::ops::Range<usize>,
    output: &mut Vec<u8>,
    in_comment: &mut bool,
) {
    let mut position = range.start;
    while position < range.end {
        let byte = input[position];
        if *in_comment {
            position += 1;
            if byte == b'\n' {
                *in_comment = false;
            }
            continue;
        }
        if byte == b'-' && position + 1 < input.len() && input[position + 1] == b'-' {
            *in_comment = true;
            position += 2;
            continue;
        }
        output.push(byte);
        position += 1;
    }
}

#[inline(never)]
fn clean_pack_hybrid(fixture: &Fixture) -> Vec<u8> {
    let input = fixture.values();
    let mut output = Vec::with_capacity(input.len());
    let mut in_comment = false;
    let full_pack_bytes = input.len() / PACK_BYTES * PACK_BYTES;

    for pack_start in (0..full_pack_bytes).step_by(PACK_BYTES) {
        let pack_end = pack_start + PACK_BYTES;
        let pack = &input[pack_start..pack_end];
        if in_comment {
            if find_value_mask(pack, b'\n') == 0 {
                continue;
            }
        } else if find_value_mask(pack, b'-') == 0 {
            output.extend_from_slice(pack);
            continue;
        }
        scan_simple(input, pack_start..pack_end, &mut output, &mut in_comment);
    }

    if full_pack_bytes < input.len() {
        scan_simple(
            input,
            full_pack_bytes..input.len(),
            &mut output,
            &mut in_comment,
        );
    }
    output
}

#[inline(never)]
fn clean_soa(fixture: &Fixture) -> Vec<u8> {
    let input = fixture.values();
    let mut output = Vec::with_capacity(input.len());
    let mut in_comment = false;
    scan_simple(input, 0..input.len(), &mut output, &mut in_comment);
    output
}

#[inline(never)]
fn clean_standard(fixture: &Fixture) -> Vec<u8> {
    let mut output = Vec::with_capacity(fixture.code.len());
    for line in fixture.code.split_inclusive('\n') {
        let code = line.find("--").map_or(line, |comment| &line[..comment]);
        output.extend_from_slice(code.as_bytes());
    }
    output
}

fn iterations_for(logical_bytes: usize, corpus_bytes: usize) -> usize {
    logical_bytes.div_ceil(corpus_bytes)
}

fn run_iterations(cleaner: Cleaner, fixtures: &[Fixture], iterations: usize) -> usize {
    let mut checksum = 0_usize;
    for _ in 0..iterations {
        for fixture in fixtures {
            let output = cleaner(black_box(fixture));
            checksum = checksum.wrapping_add(black_box(output.len()));
        }
    }
    black_box(checksum)
}

#[allow(clippy::cast_precision_loss)]
fn measure(name: &str, cleaner: Cleaner, fixtures: &[Fixture], corpus_bytes: usize) {
    let warmup_iterations = iterations_for(WARMUP_BYTES, corpus_bytes);
    let timing_iterations = iterations_for(TIMING_BYTES, corpus_bytes);
    let warmup_checksum = run_iterations(cleaner, fixtures, warmup_iterations);
    black_box(warmup_checksum);

    let mut samples = [0.0_f64; SAMPLE_COUNT];
    for (sample, result) in samples.iter_mut().enumerate() {
        let start = Instant::now();
        let checksum = run_iterations(cleaner, fixtures, timing_iterations);
        let elapsed = start.elapsed();
        black_box(checksum);
        let bytes = timing_iterations * corpus_bytes;
        *result = elapsed.as_secs_f64() * 1_000_000_000.0 / bytes as f64;
        println!(
            "mode=rust-{name} sample={} ns_per_byte={result:.6} checksum={checksum}",
            sample + 1,
        );
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "mode=rust-{name} median_ns_per_byte={:.6}",
        samples[SAMPLE_COUNT / 2]
    );
}

fn main() {
    let schema = Arc::new(Schema::new([ColumnSpec::new("code", DataType::U8)]).unwrap());
    let fixtures: Vec<_> = (0..16)
        .map(|index| {
            let code = get_code(index);
            let mut table = Table::with_capacity(Arc::clone(&schema), code.len());
            for &byte in code.as_bytes() {
                table.push_row([Value::U8(byte)]).unwrap();
            }
            Fixture { code, table }
        })
        .collect();
    for fixture in &fixtures {
        let expected = clean_standard(fixture);
        assert_eq!(clean_pack_hybrid(fixture), expected);
        assert_eq!(clean_soa(fixture), expected);
    }
    let corpus_bytes = fixtures.iter().map(|fixture| fixture.code.len()).sum();
    let mode = std::env::args().nth(1).unwrap_or_else(|| "all".to_owned());
    match mode.as_str() {
        "all" => {
            measure("pack-hybrid", clean_pack_hybrid, &fixtures, corpus_bytes);
            measure("soa", clean_soa, &fixtures, corpus_bytes);
            measure("standard", clean_standard, &fixtures, corpus_bytes);
        }
        "pack-hybrid" => measure("pack-hybrid", clean_pack_hybrid, &fixtures, corpus_bytes),
        "soa" => measure("soa", clean_soa, &fixtures, corpus_bytes),
        "standard" => measure("standard", clean_standard, &fixtures, corpus_bytes),
        _ => {
            eprintln!("usage: simd_comments [all|pack-hybrid|soa|standard]");
            std::process::exit(2);
        }
    }
}
