//! Compiled expressions and scripts with typed GD value boundaries.

use std::fmt;

use compact_str::CompactString;
use rhai::{AST, Dynamic, Engine, Scope};
use thiserror::Error;

use crate::Value;

/// The syntax accepted by a compiled [`Program`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgramKind {
    /// Exactly one expression, with no statement terminator.
    Expression,
    /// A script containing statements, assignments, functions, or control flow.
    Script,
}

/// A compile-once expression or script.
///
/// The syntax tree owns everything needed to execute the source and may be reused
/// with different [`ExpressionContext`] values. Cloning a program is cheap because
/// Rhai shares the tree's immutable internals.
#[derive(Clone)]
pub struct Program {
    ast: AST,
    kind: ProgramKind,
}

impl Program {
    /// Returns whether this program was compiled as one expression or as a script.
    #[must_use]
    pub const fn kind(&self) -> ProgramKind {
        self.kind
    }

    /// Returns the underlying Rhai syntax tree for advanced integration.
    ///
    /// Most callers should use [`ExpressionEngine::evaluate`].
    #[must_use]
    pub const fn ast(&self) -> &AST {
        &self.ast
    }
}

impl fmt::Debug for Program {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Program")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// A failure at the Rust/Rhai expression boundary.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum ExpressionError {
    /// Source text is not valid for the requested program kind.
    #[error("expression compilation failed: {message}")]
    Compile {
        /// The parser message, including a source position when available.
        message: String,
    },
    /// A compiled program could not be evaluated.
    #[error("expression evaluation failed: {message}")]
    Evaluate {
        /// The runtime message, including a source position when available.
        message: String,
    },
    /// An unsigned integer exceeds the expression runtime's signed integer range.
    #[error("unsigned value {value} exceeds the expression integer range")]
    UnsignedOutOfRange {
        /// The rejected value.
        value: u64,
    },
    /// A script returned a value outside the GD value model.
    #[error("expression returned unsupported type {type_name}")]
    UnsupportedOutput {
        /// Rhai's stable display name for the returned type.
        type_name: String,
    },
}

/// Named variables used while evaluating a [`Program`].
///
/// Scripts may update existing values or introduce new variables. Lookup is linear
/// in the number of visible variables because Rhai scopes are stack-like and allow
/// shadowing; expression workloads normally contain only a small variable set.
#[derive(Clone, Debug, Default)]
pub struct ExpressionContext {
    scope: Scope<'static>,
}

impl ExpressionContext {
    /// Creates an empty variable context.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates an empty context with storage for at least `capacity` variables.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            scope: Scope::with_capacity(capacity),
        }
    }

    /// Inserts or replaces a named variable.
    ///
    /// # Errors
    ///
    /// Returns [`ExpressionError::UnsignedOutOfRange`] for a `u64` greater than
    /// `i64::MAX`. UUID values are represented as their hyphenated text form.
    pub fn set(
        &mut self,
        name: impl Into<String>,
        value: impl Into<Value>,
    ) -> Result<&mut Self, ExpressionError> {
        let name = name.into();
        let dynamic = value_to_dynamic(value.into())?;
        self.scope.set_or_push(name, dynamic);
        Ok(self)
    }

    /// Gets a named variable as an owned GD value.
    ///
    /// # Errors
    ///
    /// Returns [`ExpressionError::UnsupportedOutput`] if the variable holds a
    /// script-only value such as an array, map, function pointer, or custom type.
    pub fn get(&self, name: &str) -> Result<Option<Value>, ExpressionError> {
        self.scope.get(name).map(dynamic_to_value).transpose()
    }

    /// Removes a named variable and returns its value.
    ///
    /// # Errors
    ///
    /// Returns [`ExpressionError::UnsupportedOutput`] if the removed value cannot
    /// be represented by [`Value`]. The value is still removed in that case.
    pub fn remove(&mut self, name: &str) -> Result<Option<Value>, ExpressionError> {
        self.scope
            .remove::<Dynamic>(name)
            .map(dynamic_into_value)
            .transpose()
    }

    /// Returns whether the context contains a visible variable with this name.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.scope.contains(name)
    }

    /// Returns the number of entries, including any script-created shadow entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.scope.len()
    }

    /// Returns whether no variables are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.scope.is_empty()
    }

    /// Removes all variables.
    pub fn clear(&mut self) {
        self.scope.clear();
    }

    /// Returns the underlying Rhai scope for advanced integration.
    #[must_use]
    pub const fn scope(&self) -> &Scope<'static> {
        &self.scope
    }

    /// Returns the underlying Rhai scope mutably for advanced integration.
    pub fn scope_mut(&mut self) -> &mut Scope<'static> {
        &mut self.scope
    }
}

/// Compiler and evaluator for expressions and scripts.
///
/// The default engine limits one evaluation to 1,000,000 operations, 64 call
/// levels, and expression nesting depth 64. `print` and `debug` are disabled so
/// library evaluation never writes to stdout or stderr. Use [`Self::inner_mut`]
/// to register application functions or adjust these limits.
pub struct ExpressionEngine {
    engine: Engine,
}

impl ExpressionEngine {
    /// Creates an engine with bounded execution and no output functions.
    #[must_use]
    pub fn new() -> Self {
        let mut engine = Engine::new();
        engine
            .set_max_operations(1_000_000)
            .set_max_call_levels(64)
            .set_max_expr_depths(64, 64)
            .disable_symbol("print")
            .disable_symbol("debug");
        Self { engine }
    }

    /// Compiles exactly one expression for repeated evaluation.
    ///
    /// Compilation is O(n) in source length and allocates an owned syntax tree.
    ///
    /// # Errors
    ///
    /// Returns [`ExpressionError::Compile`] for invalid syntax or statements where
    /// only an expression is allowed.
    pub fn compile_expression(&self, source: &str) -> Result<Program, ExpressionError> {
        self.engine
            .compile_expression(source)
            .map(|ast| Program {
                ast,
                kind: ProgramKind::Expression,
            })
            .map_err(|error| ExpressionError::Compile {
                message: error.to_string(),
            })
    }

    /// Compiles a script containing expressions, assignments, and control flow.
    ///
    /// Compilation is O(n) in source length and allocates an owned syntax tree.
    ///
    /// # Errors
    ///
    /// Returns [`ExpressionError::Compile`] when the script is invalid.
    pub fn compile(&self, source: &str) -> Result<Program, ExpressionError> {
        self.engine
            .compile(source)
            .map(|ast| Program {
                ast,
                kind: ProgramKind::Script,
            })
            .map_err(|error| ExpressionError::Compile {
                message: error.to_string(),
            })
    }

    /// Evaluates a compiled program, allowing it to mutate the context.
    ///
    /// # Errors
    ///
    /// Returns [`ExpressionError::Evaluate`] for missing variables, type errors,
    /// operation-limit exhaustion, or a function failure. Returns
    /// [`ExpressionError::UnsupportedOutput`] for arrays, maps, and custom values.
    pub fn evaluate(
        &self,
        program: &Program,
        context: &mut ExpressionContext,
    ) -> Result<Value, ExpressionError> {
        let result = self
            .engine
            .eval_ast_with_scope::<Dynamic>(&mut context.scope, &program.ast)
            .map_err(|error| ExpressionError::Evaluate {
                message: error.to_string(),
            })?;
        dynamic_into_value(result)
    }

    /// Compiles and evaluates one expression.
    ///
    /// Prefer [`Self::compile_expression`] plus [`Self::evaluate`] when evaluating
    /// the same source repeatedly.
    ///
    /// # Errors
    ///
    /// Returns the same errors as compilation and evaluation.
    pub fn evaluate_expression(
        &self,
        source: &str,
        context: &mut ExpressionContext,
    ) -> Result<Value, ExpressionError> {
        let program = self.compile_expression(source)?;
        self.evaluate(&program, context)
    }

    /// Returns the underlying Rhai engine for advanced read-only integration.
    #[must_use]
    pub const fn inner(&self) -> &Engine {
        &self.engine
    }

    /// Returns the underlying Rhai engine to register functions or tune limits.
    ///
    /// Re-enabling output functions can violate this wrapper's no-output default.
    pub fn inner_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }
}

impl Default for ExpressionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ExpressionEngine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExpressionEngine")
            .field("max_operations", &self.engine.max_operations())
            .field("max_call_levels", &self.engine.max_call_levels())
            .finish_non_exhaustive()
    }
}

fn value_to_dynamic(value: Value) -> Result<Dynamic, ExpressionError> {
    Ok(match value {
        Value::Null => Dynamic::UNIT,
        Value::Bool(value) => Dynamic::from_bool(value),
        Value::I8(value) => Dynamic::from_int(i64::from(value)),
        Value::I16(value) => Dynamic::from_int(i64::from(value)),
        Value::I32(value) => Dynamic::from_int(i64::from(value)),
        Value::I64(value) => Dynamic::from_int(value),
        Value::U8(value) => Dynamic::from_int(i64::from(value)),
        Value::U16(value) => Dynamic::from_int(i64::from(value)),
        Value::U32(value) => Dynamic::from_int(i64::from(value)),
        Value::U64(value) => Dynamic::from_int(
            i64::try_from(value).map_err(|_| ExpressionError::UnsignedOutOfRange { value })?,
        ),
        Value::F32(value) => Dynamic::from_float(f64::from(value)),
        Value::F64(value) => Dynamic::from_float(value),
        Value::String(value) => Dynamic::from(value.into_string()),
        Value::Bytes(value) => Dynamic::from_blob(value.into_vec()),
        Value::Uuid(value) => Dynamic::from(value.hyphenated().to_string()),
    })
}

fn dynamic_into_value(value: Dynamic) -> Result<Value, ExpressionError> {
    if value.is_unit() {
        Ok(Value::Null)
    } else if value.is_bool() {
        Ok(Value::Bool(value.as_bool().expect("type checked")))
    } else if value.is_int() {
        Ok(Value::I64(value.as_int().expect("type checked")))
    } else if value.is_float() {
        Ok(Value::F64(value.as_float().expect("type checked")))
    } else if value.is_char() {
        Ok(Value::String(CompactString::from(
            value.as_char().expect("type checked").to_string(),
        )))
    } else if value.is_string() {
        Ok(Value::String(
            value.into_string().expect("type checked").into(),
        ))
    } else if value.is_blob() {
        Ok(Value::Bytes(
            value.into_blob().expect("type checked").into_boxed_slice(),
        ))
    } else {
        Err(ExpressionError::UnsupportedOutput {
            type_name: value.type_name().to_owned(),
        })
    }
}

fn dynamic_to_value(value: &Dynamic) -> Result<Value, ExpressionError> {
    if value.is_unit() {
        Ok(Value::Null)
    } else if value.is_bool() {
        Ok(Value::Bool(value.as_bool().expect("type checked")))
    } else if value.is_int() {
        Ok(Value::I64(value.as_int().expect("type checked")))
    } else if value.is_float() {
        Ok(Value::F64(value.as_float().expect("type checked")))
    } else if value.is_char() {
        Ok(Value::String(CompactString::from(
            value.as_char().expect("type checked").to_string(),
        )))
    } else if value.is_string() {
        Ok(Value::String(CompactString::from(
            value
                .as_immutable_string_ref()
                .expect("type checked")
                .as_str(),
        )))
    } else if value.is_blob() {
        Ok(Value::Bytes(
            value
                .as_blob_ref()
                .expect("type checked")
                .to_vec()
                .into_boxed_slice(),
        ))
    } else {
        Err(ExpressionError::UnsupportedOutput {
            type_name: value.type_name().to_owned(),
        })
    }
}
