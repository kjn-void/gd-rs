//! Order-processing stages: import three business tables, join and validate into
//! audit/clean tables, then produce eight parameterized subsets. driver.rs owns
//! timing and SQL verification; variants schedules the parallel result tasks.
use gd::{Arguments, ColumnSpec, DataType, Schema, SqliteDatabase, Table, Value, ValueRef};
use rayon::prelude::*;

// Audit and clean share this schema. Variants project line_id, name, region,
// day, status, and amount_cents using the positions in VARIANT_COLUMNS.
pub const AUDIT_NAMES: [&str; 11] = [
    "line_id",
    "order_id",
    "customer_id",
    "name",
    "region",
    "day",
    "status",
    "quantity",
    "unit_price",
    "amount_cents",
    "errors",
];

pub const VARIANT_COLUMNS: [usize; 6] = [0, 3, 4, 5, 6, 9];

// Region, inclusive start day, exclusive end day, status, minimum gross cents,
// and discount in basis points. The fixture supplies eight parameter sets.
pub type Parameters = [i64; 6];

pub struct Inputs {
    pub customers: Table,
    pub orders: Table,
    pub lines: Table,
}

pub struct Prepared {
    pub audit: Table,
    pub clean: Table,
}

// Input, intermediate, and output fields are nullable; name is an owned string,
// and all other fields are signed 64-bit integers. Measured stages include their
// schema construction and allocation costs.
pub fn schema(names: &[&str]) -> Schema {
    Schema::new(names.iter().map(|&name| {
        ColumnSpec::new(
            name,
            if name == "name" {
                DataType::String
            } else {
                DataType::I64
            },
        )
        .nullable(true)
    }))
    .unwrap()
}

// "import": read customers, orders, and lines sequentially in source row order,
// materializing independent gd-rs tables. The isolated stage destroys them
// within its timer; "complete" retains them for preparation in the same sample.
pub fn load(db: &SqliteDatabase) -> Inputs {
    let read = |table: &str, names: &[&str]| {
        db.query_table_with_schema(
            &format!("SELECT * FROM {table} ORDER BY rowid"),
            &Arguments::new(),
            schema(names),
        )
        .unwrap()
    };

    Inputs {
        customers: read("customers", &["id", "name", "region", "active"]),
        orders: read("orders", &["id", "customer_id", "day", "status"]),
        lines: read("lines", &["id", "order_id", "quantity", "unit_price"]),
    }
}

pub fn integer(value: ValueRef<'_>) -> Option<i64> {
    match value {
        ValueRef::I64(value) => Some(value),
        ValueRef::Null => None,
        _ => panic!("unexpected non-integer in fixture"),
    }
}

// "prepare", diagnostic index mode: sort right-table IDs and resolve each left
// row to an optional right-row position. Preserve every left row and its order
// without materializing an intermediate joined table. Fixture IDs are unique.
fn sorted_join(left: &Table, key: usize, right: &Table) -> Vec<(usize, Option<usize>)> {
    let mut index: Vec<_> = right
        .rows()
        .map(|row| (integer(row.get(0).unwrap()).unwrap(), row.position()))
        .collect();

    index.sort_unstable();

    left.rows()
        .map(|row| {
            let found = integer(row.get(key).unwrap()).and_then(|key| {
                index
                    .binary_search_by_key(&key, |&(value, _)| value)
                    .ok()
                    .map(|i| index[i].1)
            });

            (row.position(), found)
        })
        .collect()
}

pub fn prepare(input: &Inputs, sorted: bool) -> Prepared {
    // Native mode uses gd-rs's hash index and left join; sorted mode uses the
    // diagnostic binary-search path corresponding to C++ GD's integer index.
    let join = |left: &Table, key: usize, right: &Table| {
        if sorted {
            sorted_join(left, key, right)
        } else {
            left.left_join_rows(key, &right.index(0).unwrap()).unwrap()
        }
    };

    // "prepare", joins: orders -> customers, then lines -> orders. These row
    // mappings connect each line to fields from all three business tables.
    let order_customers = join(&input.orders, 1, &input.customers);
    let line_orders = join(&input.lines, 1, &input.orders);

    // Borrow typed nullable columns once before scanning. The loop then follows
    // row mappings and reads column slices instead of converting cells each time.
    let lines: [_; 4] = std::array::from_fn(|i| {
        input
            .lines
            .column(i)
            .unwrap()
            .as_nullable_slice::<i64>()
            .unwrap()
    });

    let orders: [_; 4] = std::array::from_fn(|i| {
        input
            .orders
            .column(i)
            .unwrap()
            .as_nullable_slice::<i64>()
            .unwrap()
    });

    let names = input.customers.column(1).unwrap();
    let regions = input
        .customers
        .column(2)
        .unwrap()
        .as_nullable_slice::<i64>()
        .unwrap();
    let active = input
        .customers
        .column(3)
        .unwrap()
        .as_nullable_slice::<i64>()
        .unwrap();

    let value = |number: Option<i64>| number.map_or(Value::Null, Value::I64);

    let mut audit = Table::with_capacity(schema(&AUDIT_NAMES), input.lines.row_count());

    // "prepare", validation: retain one audit row for every source line,
    // including invalid rows. Missing join matches supply NULL field values.
    for (line, order) in line_orders {
        let customer = order.and_then(|i| order_customers[i].1);

        let l = |column: usize| lines[column][line];
        let o = |column: usize| order.and_then(|i| orders[column][i]);

        let name = customer.map_or(ValueRef::Null, |i| names.get(i).unwrap());
        let quantity = l(2);
        let price = l(3);
        let quantity_ok = quantity.is_some_and(|v| v > 0);
        let price_ok = price.is_some_and(|v| v >= 0);

        // Independent error bits: 1 missing order, 2 missing customer,
        // 4 missing/empty name, 8 inactive customer, 16 invalid quantity,
        // 32 invalid price, and 64 missing order date. Multiple bits may be set.
        let mut errors = i64::from(order.is_none());
        errors |= i64::from(order.is_some() && customer.is_none()) * 2;
        errors |= i64::from(customer.is_some() && name.as_str().unwrap_or("").is_empty()) * 4;
        errors |= i64::from(customer.is_some() && customer.and_then(|i| active[i]) == Some(0)) * 8;
        errors |= i64::from(!quantity_ok) * 16;
        errors |= i64::from(!price_ok) * 32;
        errors |= i64::from(order.is_some() && o(2).is_none()) * 64;

        // Gross cents are quantity * unit_price when both operands are valid.
        // Checked arithmetic rejects overflow instead of wrapping the amount.
        let amount = if quantity_ok && price_ok {
            Value::I64(quantity.unwrap().checked_mul(price.unwrap()).unwrap())
        } else {
            Value::Null
        };

        // Copy the customer name into audit-owned storage; prepared results
        // must remain usable after the imported source tables are destroyed.
        audit
            .push_row([
                value(l(0)),
                value(l(1)),
                value(o(1)),
                name.to_owned(),
                value(customer.and_then(|i| regions[i])),
                value(o(2)),
                value(o(3)),
                value(l(2)),
                value(l(3)),
                amount,
                Value::I64(errors),
            ])
            .unwrap();
    }

    // "prepare", exclusion: copy every column of error-free audit rows into
    // an owned clean table. Invalid rows remain available in audit.
    let errors = audit
        .column(10)
        .unwrap()
        .as_nullable_slice::<i64>()
        .unwrap();
    let clean = audit
        .filter_rows(|row| errors[row.position()] == Some(0))
        .unwrap();

    Prepared { audit, clean }
}

// "variants": one parameterized result. Each task scans the immutable clean
// table and owns its selection and output. Filtering, projection, copying, and
// discount calculation are all included in stage timing.
pub fn variant(clean: &Table, p: &Parameters) -> Table {
    let [region, from_day, to_day, status, minimum, discount_bp] = *p;

    let regions = clean.column(4).unwrap().as_nullable_slice::<i64>().unwrap();
    let days = clean.column(5).unwrap().as_nullable_slice::<i64>().unwrap();
    let statuses = clean.column(6).unwrap().as_nullable_slice::<i64>().unwrap();
    let gross = clean.column(9).unwrap().as_nullable_slice::<i64>().unwrap();

    // Select region/status (or any when -1), the half-open date interval,
    // and the minimum gross amount. Clean rows have non-null predicate fields;
    // the row selection retains source order.
    let rows = clean.select_rows(|row| {
        let i = row.position();

        (region == -1 || regions[i].unwrap() == region)
            && {
                let day = days[i].unwrap();

                day >= from_day && day < to_day
            }
            && (status == -1 || statuses[i].unwrap() == status)
            && gross[i].unwrap() >= minimum
    });

    // Project six columns and copy selected rows into an independent result.
    let mut result = clean.select(&rows, &VARIANT_COLUMNS).unwrap();
    let (_, [amounts]) = result.columns_io([], [5]).unwrap();

    // Replace this result's gross cents with discounted cents, rounded half up.
    // Mutating the typed output slice leaves clean and the other variants intact.
    for amount in amounts.as_nullable_mut_slice::<i64>().unwrap() {
        let net = amount
            .unwrap()
            .checked_mul(10_000 - discount_bp)
            .unwrap()
            .checked_add(5_000)
            .unwrap()
            / 10_000;

        *amount = Some(net);
    }

    result
}

pub fn variants(clean: &Table, parameters: &[Parameters], pool: &rayon::ThreadPool) -> Vec<Table> {
    // Parallelize parameter sets, not source row ranges. Each task scans all of
    // clean and writes its own table. The prebuilt pool is reused across samples;
    // collect waits for all eight outputs and preserves parameter order.
    pool.install(|| parameters.par_iter().map(|p| variant(clean, p)).collect())
}
