//! Borrowed selection, composition and export contracts.

use gd::{
    ColumnSpec, DataType, FormatError, Schema, Table, TableError, UnknownFields, Value, ValueRef,
    selection_to_csv, selection_to_json, table_to_csv, table_to_json,
};
use proptest::prelude::*;

fn source() -> Table {
    let mut table = Table::new(
        Schema::new([
            ColumnSpec::new("id", DataType::I64),
            ColumnSpec::new("name", DataType::String).with_alias("display"),
        ])
        .unwrap()
        .with_unknown_fields(UnknownFields::Store),
    );
    table
        .push_row_with_extras([1_i64.into(), "Ada".into()], [("extra", 9_i64)])
        .unwrap();
    table.push_row([2_i64.into(), "Grace".into()]).unwrap();
    table.tombstone_row(1).unwrap();
    table.push_row([3_i64.into(), "Linus".into()]).unwrap();
    table.set_property("owner", "source");
    table
}

#[test]
fn selected_rows_borrow_cells_and_preserve_projection_order_duplicates_and_metadata() {
    let table = source();
    let view = table.select_view(&[2, 0, 0, 1], &[1]).unwrap();
    assert!(std::ptr::eq(view.table(), std::ptr::from_ref(&table)));
    assert_eq!(view.positions(), &[2, 0, 0, 1]);
    assert_eq!(view.columns(), &[1]);
    assert_eq!(view.row_count(), 4);
    assert_eq!(view.column_count(), 1);
    assert!(!view.is_empty());
    assert_eq!(view.column(0).unwrap().alias(), Some("display"));
    assert!(view.column(1).is_none());
    let row = view.row(1).unwrap();
    assert_eq!(row.position(), 0);
    assert_eq!(row.len(), 1);
    assert!(!row.is_empty());
    assert_eq!(row.get(0), Some(ValueRef::String("Ada")));
    assert_eq!(row.get_named("display"), Some(ValueRef::String("Ada")));
    assert_eq!(row.get_named("extra"), Some(ValueRef::I64(9)));
    assert_eq!(row.get_named("id"), None);
    assert_eq!(row.get(1), None);
    assert!(view.row(4).is_none());
    assert_eq!(
        row.get(0).unwrap().as_str().unwrap().as_ptr(),
        table.cell(0, 1).unwrap().as_str().unwrap().as_ptr()
    );
    assert_eq!(
        view.rows()
            .rev()
            .map(gd::SelectedRow::position)
            .collect::<Vec<_>>(),
        [1, 0, 0, 2]
    );
    assert_eq!(
        view.live_rows()
            .map(gd::SelectedRow::position)
            .collect::<Vec<_>>(),
        [2, 0, 0]
    );
    let copied = view.materialize().unwrap();
    assert_eq!(copied.property("owner"), Some(ValueRef::String("source")));
    assert!(copied.is_tombstoned(3).unwrap());
    assert_eq!(copied.cell_named(1, "extra"), Ok(ValueRef::I64(9)));
}

#[test]
fn composed_views_filter_project_and_export_without_materializing_cells() {
    let table = source();
    let view = table.project_view(&[1, 0]).unwrap();
    let filtered = view.filter_rows(|row| row.get(1) == Some(ValueRef::I64(1)));
    let projected = filtered.project(&[0]).unwrap();
    assert_eq!(projected.positions(), &[0]);
    assert_eq!(projected.columns(), &[1]);
    assert_eq!(
        selection_to_json(&projected).unwrap(),
        "[{\"name\":\"Ada\"}]"
    );
    assert_eq!(selection_to_csv(&projected, true).unwrap(), "name\nAda\n");
    assert!(matches!(
        projected.project(&[1]),
        Err(TableError::ColumnOutOfBounds { .. })
    ));
    assert!(matches!(
        view.project(&[0, 0]),
        Err(TableError::DuplicateColumnName(_))
    ));
    let filter = table.filter_view(|row| row.get(0) == Some(ValueRef::I64(3)));
    assert_eq!(filter.positions(), &[2]);
    let duplicated = table.select_view(&[1, 0, 0], &[1, 0]).unwrap();
    let copied = duplicated.materialize().unwrap();
    assert_eq!(
        selection_to_json(&duplicated).unwrap(),
        table_to_json(&copied).unwrap()
    );
    assert_eq!(
        selection_to_csv(&duplicated, false).unwrap(),
        table_to_csv(&copied, false).unwrap()
    );
}

#[test]
fn selection_bounds_empty_projection_and_serialization_errors_are_explicit() {
    let table = source();
    assert!(matches!(
        table.select_view(&[3], &[0]),
        Err(TableError::RowOutOfBounds { .. })
    ));
    assert!(matches!(
        table.select_view(&[], &[2]),
        Err(TableError::ColumnOutOfBounds { .. })
    ));
    assert!(matches!(
        table.project_view(&[0, 0]),
        Err(TableError::DuplicateColumnName(_))
    ));
    let view = table.select_view(&[0, 1], &[]).unwrap();
    assert_eq!(view.row_count(), 2);
    assert!(!view.is_empty());
    assert!(view.row(0).unwrap().is_empty());
    assert_eq!(selection_to_json(&view).unwrap(), "[{}]");
    assert!(matches!(
        selection_to_csv(&view, false),
        Err(FormatError::ZeroColumnTable)
    ));
    assert_eq!(view.materialize().unwrap().row_count(), 2);
    let empty = table.select_view(&[], &[0]).unwrap();
    assert!(empty.is_empty());
    assert_eq!(selection_to_json(&empty).unwrap(), "[]");
    let mut floats = Table::new(Schema::new([ColumnSpec::new("value", DataType::F64)]).unwrap());
    floats.push_row([Value::F64(f64::NAN)]).unwrap();
    assert!(matches!(
        selection_to_json(&floats.project_view(&[0]).unwrap()),
        Err(FormatError::NonFiniteFloat)
    ));
}

proptest! {
    #[test]
    fn borrowed_selection_materialization_matches_the_row_model(
        values in prop::collection::vec(any::<i64>(),1..40),
        positions in prop::collection::vec(any::<usize>(),0..40)
    ) {
        let mut table = Table::new(Schema::new([ColumnSpec::new("value",DataType::I64)]).unwrap());
        for &value in &values { table.push_row([value.into()]).unwrap(); }
        let positions: Vec<_> = positions.iter().map(|position| position % values.len()).collect();
        let view = table.select_view(&positions,&[0]).unwrap();
        let expected: Vec<_> = positions.iter().map(|&position| Value::I64(values[position])).collect();
        let actual: Vec<_> = view.rows().map(|row| row.get(0).unwrap().to_owned()).collect();
        prop_assert_eq!(actual,expected.clone());
        let copied = view.materialize().unwrap();
        prop_assert_eq!(copied.rows().map(|row| row.get(0).unwrap().to_owned()).collect::<Vec<_>>(),expected);
    }
}
