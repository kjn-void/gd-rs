//! Shared cell storage with overlapping column projections.
//!
//! Run: `cargo run --no-default-features --example flyweight_projection`
//! `Rc<RefCell<Table>>` permits sequential edits between borrowed selections.
//! `Arc<Table>` demonstrates immutable projections in separate threads.

use gd::{ColumnSpec, DataType, Schema, Table, TableError, TableSelection, Value, ValueRef};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

const ADA: &str = "Ada Lovelace customer record";
const GRACE: &str = "Grace Hopper customer record";

fn make_table() -> Result<Table, TableError> {
    let schema = Schema::new([
        ColumnSpec::new("key", DataType::U64),
        ColumnSpec::new("name", DataType::String),
        ColumnSpec::new("qty", DataType::U64),
        ColumnSpec::new("cost", DataType::U64),
    ])?;
    let mut table = Table::new(schema);
    table.push_row([1_u64.into(), ADA.into(), 10_u64.into(), 100_u64.into()])?;
    table.push_row([2_u64.into(), GRACE.into(), 20_u64.into(), 200_u64.into()])?;
    Ok(table)
}

// This example's owning descriptor retains one shared Table and a column map.
// Actual borrowed selections are created with gd-rs's native project_view API.
// The callback keeps all borrowed cell values inside the RefCell read borrow.
struct Projection {
    source: Rc<RefCell<Table>>,
    columns: Vec<usize>,
}

impl Projection {
    fn new(source: Rc<RefCell<Table>>, columns: Vec<usize>) -> Result<Self, TableError> {
        {
            let table = source.borrow();
            table.project_view(&columns)?;
        }
        Ok(Self { source, columns })
    }

    fn with_view<R>(&self, read: impl FnOnce(&TableSelection<'_>) -> R) -> Result<R, TableError> {
        let table = self.source.borrow();
        let view = table.project_view(&self.columns)?;
        Ok(read(&view))
    }

    fn set_u64(&self, row: usize, column: usize, value: u64) -> Result<(), TableError> {
        let source_column =
            self.columns
                .get(column)
                .copied()
                .ok_or(TableError::ColumnOutOfBounds {
                    column,
                    column_count: self.columns.len(),
                })?;
        self.source
            .borrow_mut()
            .set_cell(row, source_column, Value::U64(value))
    }
}

fn quantity(view: &TableSelection<'_>, row: usize, column: usize) -> u64 {
    match view.row(row).unwrap().get(column).unwrap() {
        ValueRef::U64(value) => value,
        other => panic!("expected quantity, got {other:?}"),
    }
}

fn main() -> Result<(), TableError> {
    let table = make_table()?;

    // Sharing a schema alone leaves cell values independently owned.
    let mut independent = Table::new(table.schema_arc());
    independent.push_row([1_u64.into(), ADA.into(), 500_u64.into(), 100_u64.into()])?;
    assert!(std::ptr::eq(table.schema(), independent.schema()));
    assert_eq!(table.cell(0, 2)?, ValueRef::U64(10));
    assert_eq!(independent.cell(0, 2)?, ValueRef::U64(500));
    println!("gd-rs native: shared schema, independent cells");

    // Table::project materializes a copy; project_view borrows the cells.
    let copied = table.project(&[0, 2])?;
    assert_eq!(copied.column_count(), 2);
    assert_eq!(copied.row_count(), 2);
    println!("gd-rs native: project produces a copied table");

    let source = Rc::new(RefCell::new(table));
    let a = Projection::new(Rc::clone(&source), vec![0, 1, 2])?;
    let b = Projection::new(Rc::clone(&source), vec![1, 2, 3])?;
    assert!(Rc::ptr_eq(&a.source, &b.source));

    // Verify the actual stored string bytes are borrowed by both projections.
    {
        let owner_a = a.source.borrow();
        let owner_b = b.source.borrow();
        let view_a = owner_a.project_view(&a.columns)?;
        let view_b = owner_b.project_view(&b.columns)?;
        assert!(std::ptr::eq(view_a.table(), view_b.table()));
        let name_a = view_a.row(0).unwrap().get(1).unwrap().as_str().unwrap();
        let name_b = view_b.row(0).unwrap().get(0).unwrap().as_str().unwrap();
        assert_eq!(name_a, ADA);
        assert_eq!(name_a.as_ptr(), name_b.as_ptr());
        assert_eq!(view_a.row(1).unwrap().get(1), Some(ValueRef::String(GRACE)));
    } // Release the borrowed selections before editing the shared source.

    a.set_u64(0, 2, 99)?;
    assert_eq!(source.borrow().cell(0, 2)?, ValueRef::U64(99));
    assert_eq!(a.with_view(|view| quantity(view, 0, 2))?, 99);
    assert_eq!(b.with_view(|view| quantity(view, 0, 1))?, 99);
    assert_eq!(copied.cell(0, 1)?, ValueRef::U64(10));
    println!("gd-rs views: A=[key,name,qty], B=[name,qty,cost], shared cell storage");
    println!("gd-rs views: A/source/B qty=99; copied projection qty=10");

    drop(source);
    assert_eq!(b.with_view(|view| quantity(view, 0, 1))?, 99);
    println!("gd-rs views: source handle dropped; views remain valid");

    // Arc<Table> retains the same data across threads. Each worker constructs
    // its native borrowing projection after receiving its owned Arc handle.
    let shared = Arc::new(make_table()?);
    let owner_a = Arc::clone(&shared);
    let owner_b = Arc::clone(&shared);
    let worker_a = std::thread::spawn(move || {
        let view = owner_a.project_view(&[0, 1, 2]).unwrap();
        quantity(&view, 0, 2)
    });
    let worker_b = std::thread::spawn(move || {
        let view = owner_b.project_view(&[1, 2, 3]).unwrap();
        quantity(&view, 0, 1)
    });
    assert_eq!(worker_a.join().unwrap(), 10);
    assert_eq!(worker_b.join().unwrap(), 10);
    println!("gd-rs Arc: both worker projections read the same immutable table");
    Ok(())
}
