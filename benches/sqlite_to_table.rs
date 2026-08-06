//! Materialize three reproducible random-width `SQLite` tables into GD tables.

use std::{env, hint::black_box, time::Duration};

use criterion::{Criterion, SamplingMode, Throughput, criterion_group, criterion_main};
use gd::{DataType, SqliteDatabase};

const DEFAULT_ROWS_PER_TABLE: usize = 1_000_000;
const DEFAULT_SCHEMA_SEED: u64 = 0x6a09_e667_f3bc_c909;
const TABLE_COUNT: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NumericType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
}

impl NumericType {
    const ALL: [Self; 10] = [
        Self::I8,
        Self::I16,
        Self::I32,
        Self::I64,
        Self::U8,
        Self::U16,
        Self::U32,
        Self::U64,
        Self::F32,
        Self::F64,
    ];

    const fn declaration(self) -> &'static str {
        match self {
            Self::I8 => "INTEGER_I8",
            Self::I16 => "INTEGER_I16",
            Self::I32 => "INTEGER_I32",
            Self::I64 => "INTEGER_I64",
            Self::U8 => "INTEGER_U8",
            Self::U16 => "INTEGER_U16",
            Self::U32 => "INTEGER_U32",
            Self::U64 => "INTEGER_U64",
            Self::F32 => "REAL_F32",
            Self::F64 => "REAL_F64",
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::I8 => "I8",
            Self::I16 => "I16",
            Self::I32 => "I32",
            Self::I64 => "I64",
            Self::U8 => "U8",
            Self::U16 => "U16",
            Self::U32 => "U32",
            Self::U64 => "U64",
            Self::F32 => "F32",
            Self::F64 => "F64",
        }
    }

    const fn data_type(self) -> DataType {
        match self {
            Self::I8 => DataType::I8,
            Self::I16 => DataType::I16,
            Self::I32 => DataType::I32,
            Self::I64 => DataType::I64,
            Self::U8 => DataType::U8,
            Self::U16 => DataType::U16,
            Self::U32 => DataType::U32,
            Self::U64 => DataType::U64,
            Self::F32 => DataType::F32,
            Self::F64 => DataType::F64,
        }
    }

    const fn byte_width(self) -> u64 {
        match self {
            Self::I8 | Self::U8 => 1,
            Self::I16 | Self::U16 => 2,
            Self::I32 | Self::U32 | Self::F32 => 4,
            Self::I64 | Self::U64 | Self::F64 => 8,
        }
    }

    const fn expression(self) -> &'static str {
        match self {
            Self::I8 => "((value % 255) - 127)",
            Self::I16 => "((value % 65535) - 32767)",
            Self::I32 => "((value * 48271 % 2000000001) - 1000000000)",
            Self::I64 => "((value * 48271) - 24000)",
            Self::U8 => "(value % 256)",
            Self::U16 => "(value % 65536)",
            Self::U32 => "(value * 48271 % 4000000000)",
            Self::U64 => "(value * 48271)",
            Self::F32 => "CAST((value % 1000003) * 0.5 AS REAL)",
            Self::F64 => "CAST(value * 0.125 AS REAL)",
        }
    }
}

#[derive(Debug)]
struct TableSpec {
    name: String,
    columns: Vec<NumericType>,
}

fn next_random(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

fn table_specs(seed: u64) -> Vec<TableSpec> {
    let mut state = seed;
    (0..TABLE_COUNT)
        .map(|table| {
            let column_count = 3 + usize::try_from(next_random(&mut state) % 3).unwrap();
            let columns = (0..column_count)
                .map(|_| {
                    let index = usize::try_from(next_random(&mut state) % 10).unwrap();
                    NumericType::ALL[index]
                })
                .collect();
            TableSpec {
                name: format!("table_{table}"),
                columns,
            }
        })
        .collect()
}

fn setting(name: &str, default: usize) -> usize {
    env::var(name).map_or(default, |value| {
        value
            .parse()
            .unwrap_or_else(|error| panic!("invalid {name}={value}: {error}"))
    })
}

fn seed_setting() -> u64 {
    env::var("GD_SQLITE_TO_TABLE_SEED").map_or(DEFAULT_SCHEMA_SEED, |value| {
        let (parsed, radix) = value
            .strip_prefix("0x")
            .or_else(|| value.strip_prefix("0X"))
            .map_or((value.as_str(), 10), |hexadecimal| (hexadecimal, 16));
        u64::from_str_radix(parsed, radix)
            .unwrap_or_else(|error| panic!("invalid GD_SQLITE_TO_TABLE_SEED={value}: {error}"))
    })
}

fn schema_label(specs: &[TableSpec]) -> String {
    specs
        .iter()
        .map(|spec| {
            format!(
                "{}={}",
                spec.name,
                spec.columns
                    .iter()
                    .map(|data_type| data_type.name())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn fixture(rows: usize, specs: &[TableSpec]) -> SqliteDatabase {
    assert!(
        rows > 0,
        "GD_SQLITE_TO_TABLE_ROWS must be greater than zero"
    );
    let database = SqliteDatabase::open_in_memory().unwrap();
    database
        .execute_batch("PRAGMA synchronous=OFF; PRAGMA temp_store=MEMORY;")
        .unwrap();
    for spec in specs {
        let declarations = spec
            .columns
            .iter()
            .enumerate()
            .map(|(column, data_type)| {
                format!("column_{column} {} NOT NULL", data_type.declaration())
            })
            .collect::<Vec<_>>()
            .join(", ");
        let expressions = spec
            .columns
            .iter()
            .map(|data_type| data_type.expression())
            .collect::<Vec<_>>()
            .join(", ");
        database
            .execute_batch(&format!(
                "CREATE TABLE {}({declarations});\
                 WITH RECURSIVE sequence(value) AS (\
                     VALUES(0) UNION ALL \
                     SELECT value + 1 FROM sequence WHERE value + 1 < {rows} \
                 ) \
                 INSERT INTO {} SELECT {expressions} FROM sequence;",
                spec.name, spec.name
            ))
            .unwrap();
    }
    database
}

fn validate_schemas(database: &SqliteDatabase, specs: &[TableSpec]) {
    for spec in specs {
        let schema = database.schema_for_table(&spec.name).unwrap();
        assert_eq!(schema.len(), spec.columns.len());
        for (column, data_type) in spec.columns.iter().enumerate() {
            let column = schema.column(column).unwrap();
            assert_eq!(column.data_type(), data_type.data_type());
            assert!(!column.is_nullable());
        }
    }
}

fn materialize(database: &SqliteDatabase, specs: &[TableSpec], rows: usize) -> Vec<gd::Table> {
    specs
        .iter()
        .map(|spec| {
            let table = database.load_table(&spec.name).unwrap();
            assert_eq!(table.row_count(), rows);
            table
        })
        .collect()
}

fn benchmark_sqlite_to_table(criterion: &mut Criterion) {
    let rows = setting("GD_SQLITE_TO_TABLE_ROWS", DEFAULT_ROWS_PER_TABLE);
    let seed = seed_setting();
    let specs = table_specs(seed);
    let logical_bytes = specs
        .iter()
        .flat_map(|spec| &spec.columns)
        .map(|data_type| data_type.byte_width())
        .sum::<u64>()
        * u64::try_from(rows).unwrap();
    let database = fixture(rows, &specs);
    validate_schemas(&database, &specs);
    let schema_label = schema_label(&specs);

    let mut group = criterion.benchmark_group(format!(
        "SQLite/ToTable/rows={rows}x{TABLE_COUNT}/seed={seed:#018x}/{schema_label}"
    ));
    group
        .throughput(Throughput::Bytes(logical_bytes))
        .sampling_mode(SamplingMode::Flat)
        .sample_size(10)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(10));
    group.bench_function("gd-rs/DeclaredSchema", |bencher| {
        bencher.iter(|| black_box(materialize(black_box(&database), black_box(&specs), rows)));
    });
    group.finish();
}

criterion_group!(benches, benchmark_sqlite_to_table);
criterion_main!(benches);
