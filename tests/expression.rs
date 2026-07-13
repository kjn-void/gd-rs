//! Integration and property tests for compiled expressions.

use gd::{ExpressionContext, ExpressionEngine, ExpressionError, ProgramKind, Value};
use proptest::prelude::*;

#[test]
fn compiled_expression_obeys_precedence_and_reads_variables() {
    let engine = ExpressionEngine::new();
    let program = engine.compile_expression("x + y * 2").unwrap();
    let mut context = ExpressionContext::new();
    context.set("x", 10_i64).unwrap().set("y", 20_i64).unwrap();

    assert_eq!(program.kind(), ProgramKind::Expression);
    assert_eq!(engine.evaluate(&program, &mut context), Ok(Value::I64(50)));
}

#[test]
fn script_control_flow_updates_context() {
    let engine = ExpressionEngine::new();
    let program = engine
        .compile(
            r"
                let total = 0;
                for value in 1..=5 { total += value; }
                result = total * multiplier;
                result
            ",
        )
        .unwrap();
    let mut context = ExpressionContext::new();
    context
        .set("result", 0_i64)
        .unwrap()
        .set("multiplier", 2_i64)
        .unwrap();

    assert_eq!(program.kind(), ProgramKind::Script);
    assert_eq!(engine.evaluate(&program, &mut context), Ok(Value::I64(30)));
    assert_eq!(context.get("result"), Ok(Some(Value::I64(30))));
}

#[test]
fn gd_values_cross_the_runtime_boundary_explicitly() {
    let engine = ExpressionEngine::new();
    let mut context = ExpressionContext::new();
    context
        .set("nothing", Value::Null)
        .unwrap()
        .set("flag", true)
        .unwrap()
        .set("text", "café")
        .unwrap()
        .set("bytes", Value::from(vec![1_u8, 2, 3]))
        .unwrap();

    assert_eq!(context.get("nothing"), Ok(Some(Value::Null)));
    assert_eq!(context.get("flag"), Ok(Some(Value::Bool(true))));
    assert_eq!(context.get("text"), Ok(Some(Value::from("café"))));
    assert_eq!(
        context.get("bytes"),
        Ok(Some(Value::from(vec![1_u8, 2, 3])))
    );
    assert_eq!(
        engine.evaluate_expression("'x'", &mut context),
        Ok(Value::from("x"))
    );
}

#[test]
fn invalid_and_unsupported_values_are_typed_errors() {
    let engine = ExpressionEngine::new();
    let mut context = ExpressionContext::new();

    assert!(matches!(
        engine.compile_expression("1 +"),
        Err(ExpressionError::Compile { .. })
    ));
    assert!(matches!(
        engine.evaluate_expression("missing + 1", &mut context),
        Err(ExpressionError::Evaluate { .. })
    ));
    assert_eq!(
        context.set("too_large", u64::MAX).unwrap_err(),
        ExpressionError::UnsignedOutOfRange { value: u64::MAX }
    );
    assert!(matches!(
        engine.evaluate_expression("[1, 2, 3]", &mut context),
        Err(ExpressionError::UnsupportedOutput { .. })
    ));
}

#[test]
fn applications_can_register_typed_functions() {
    let mut engine = ExpressionEngine::new();
    engine
        .inner_mut()
        .register_fn("distance2", |x: i64, y: i64| x * x + y * y);
    let program = engine.compile_expression("distance2(3, 4)").unwrap();

    assert_eq!(
        engine.evaluate(&program, &mut ExpressionContext::new()),
        Ok(Value::I64(25))
    );
}

#[test]
fn execution_limit_stops_unbounded_scripts() {
    let mut engine = ExpressionEngine::new();
    engine.inner_mut().set_max_operations(1_000);
    let program = engine.compile("while true {}").unwrap();

    assert!(matches!(
        engine.evaluate(&program, &mut ExpressionContext::new()),
        Err(ExpressionError::Evaluate { .. })
    ));
}

proptest! {
    #[test]
    fn compiled_integer_formula_matches_rust(
        a in -10_000_i64..10_000,
        b in -10_000_i64..10_000,
        c in -100_i64..100,
    ) {
        let engine = ExpressionEngine::new();
        let program = engine.compile_expression("a + b * c").unwrap();
        let mut context = ExpressionContext::with_capacity(3);
        context.set("a", a).unwrap().set("b", b).unwrap().set("c", c).unwrap();

        prop_assert_eq!(
            engine.evaluate(&program, &mut context),
            Ok(Value::I64(a + b * c))
        );
    }
}
