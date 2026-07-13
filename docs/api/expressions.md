# Expressions

The C++ expression documentation combines token compilation, callbacks, and a custom
runtime. `gd-rs` provides the same compile-once/evaluate-many shape through a bounded
[Rhai](https://rhai.rs/) engine while keeping values at the GD type boundary.

## Compile and evaluate

```rust
use gd::{ExpressionContext, ExpressionEngine, Value};

let engine = ExpressionEngine::new();
let program = engine.compile_expression("price * quantity").unwrap();

let mut context = ExpressionContext::new();
context.set("price", 12_i64).unwrap();
context.set("quantity", 4_i64).unwrap();

assert_eq!(engine.evaluate(&program, &mut context).unwrap(), Value::I64(48));
```

`compile_expression` accepts exactly one expression. `compile` accepts a script with
assignments, statements, functions, and control flow. A `Program` owns its syntax
tree and can be reused with different mutable `ExpressionContext` values; cloning it
is cheap because Rhai shares immutable internals.

For one-off work, `evaluate_expression` compiles and evaluates in one call. Prefer a
stored `Program` in a loop so parsing does not become the dominant cost.

## Context and type mapping

```rust
use gd::{ExpressionContext, ExpressionEngine, Value};

let engine = ExpressionEngine::new();
let program = engine.compile("counter += 1; counter").unwrap();
let mut context = ExpressionContext::new();
context.set("counter", 9_i64).unwrap();

assert_eq!(engine.evaluate(&program, &mut context).unwrap(), Value::I64(10));
assert_eq!(context.get("counter").unwrap(), Some(Value::I64(10)));
```

GD signed integers and representable unsigned integers become Rhai `i64`; a `u64`
above `i64::MAX` is rejected. `F32` becomes `f64`, UUID becomes canonical text, bytes
become a Rhai blob, and null becomes Rhai unit. Evaluation converts scalar results
back to `Value`. Arrays, maps, function pointers, and custom objects return
`ExpressionError::UnsupportedOutput` instead of inventing a lossy GD representation.

Scripts may update existing variables or introduce new ones. `get`, `remove`,
`contains`, and `clear` manage the context; `scope` and `scope_mut` expose Rhai's
scope for advanced integration.

## Execution limits and extension

The default engine allows at most 1,000,000 operations, 64 call levels, and expression
nesting depth 64. Its `print` and `debug` symbols are disabled, so evaluation does not
write to standard output. Exceeding a runtime limit is an evaluation error.

Use `ExpressionEngine::inner_mut` to register typed application functions or tune
limits, and `Program::ast` for advanced read-only AST integration. Those escape
hatches expose Rhai policy directly; re-enabling output or accepting custom types can
change the wrapper's default guarantees.

The crate does not duplicate the C++ compiler macros, callback signatures, or custom
token bytecode. It also does not promise that a configured engine and mutable context
can be shared concurrently. Put synchronization and one-engine-per-worker policies at
the application boundary when required.
