//! The order-processing application; timing and SQL reference checks live in main.rs.
use gd::{Arguments, ColumnSpec, DataType, Schema, SqliteDatabase, Table, Value, ValueRef};
use rayon::prelude::*;

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

pub fn cell(table: &Table, row: Option<usize>, column: usize) -> ValueRef<'_> {
    row.map_or(ValueRef::Null, |row| table.cell(row, column).unwrap())
}

pub fn integer(value: ValueRef<'_>) -> Option<i64> {
    match value {
        ValueRef::I64(value) => Some(value),
        ValueRef::Null => None,
        _ => panic!("unexpected non-integer in fixture"),
    }
}

// Diagnostic counterpart to C++ GD's sorted integer index. Keys are unique here.
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
    let join = |left: &Table, key: usize, right: &Table| {
        if sorted {
            sorted_join(left, key, right)
        } else {
            left.left_join_rows(key, &right.index(0).unwrap()).unwrap()
        }
    };
    let order_customers = join(&input.orders, 1, &input.customers);
    let line_orders = join(&input.lines, 1, &input.orders);
    let mut audit = Table::with_capacity(schema(&AUDIT_NAMES), input.lines.row_count());
    for (line, order) in line_orders {
        let customer = order.and_then(|i| order_customers[i].1);
        let l = |column| cell(&input.lines, Some(line), column);
        let o = |column| cell(&input.orders, order, column);
        let c = |column| cell(&input.customers, customer, column);
        let quantity = integer(l(2));
        let price = integer(l(3));
        let quantity_ok = quantity.is_some_and(|v| v > 0);
        let price_ok = price.is_some_and(|v| v >= 0);
        let mut errors = i64::from(order.is_none());
        errors |= i64::from(order.is_some() && customer.is_none()) * 2;
        errors |= i64::from(customer.is_some() && c(1).as_str().unwrap_or("").is_empty()) * 4;
        errors |= i64::from(customer.is_some() && integer(c(3)) == Some(0)) * 8;
        errors |= i64::from(!quantity_ok) * 16;
        errors |= i64::from(!price_ok) * 32;
        errors |= i64::from(order.is_some() && integer(o(2)).is_none()) * 64;
        let amount = if quantity_ok && price_ok {
            Value::I64(quantity.unwrap().checked_mul(price.unwrap()).unwrap())
        } else {
            Value::Null
        };
        audit
            .push_row([
                l(0).to_owned(),
                l(1).to_owned(),
                o(1).to_owned(),
                c(1).to_owned(),
                c(2).to_owned(),
                o(2).to_owned(),
                o(3).to_owned(),
                l(2).to_owned(),
                l(3).to_owned(),
                amount,
                Value::I64(errors),
            ])
            .unwrap();
    }
    let clean = audit
        .filter_rows(|row| row.get(10) == Some(ValueRef::I64(0)))
        .unwrap();
    Prepared { audit, clean }
}

pub fn variant(clean: &Table, p: &Parameters) -> Table {
    let [region, from_day, to_day, status, minimum, discount_bp] = *p;
    let rows = clean.select_rows(|row| {
        let number = |column| integer(row.get(column).unwrap()).unwrap();
        (region == -1 || number(4) == region)
            && number(5) >= from_day
            && number(5) < to_day
            && (status == -1 || number(6) == status)
            && number(9) >= minimum
    });
    let mut result = clean.select(&rows, &VARIANT_COLUMNS).unwrap();
    for row in 0..result.row_count() {
        let gross = integer(result.cell(row, 5).unwrap()).unwrap();
        let net = gross
            .checked_mul(10_000 - discount_bp)
            .unwrap()
            .checked_add(5_000)
            .unwrap()
            / 10_000;
        result.set_cell(row, 5, Value::I64(net)).unwrap();
    }
    result
}

pub fn variants(clean: &Table, parameters: &[Parameters], pool: &rayon::ThreadPool) -> Vec<Table> {
    pool.install(|| parameters.par_iter().map(|p| variant(clean, p)).collect())
}
