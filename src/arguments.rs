//! Ordered named and positional dynamic arguments.

use std::iter::FromIterator;

use ahash::AHashMap;
use compact_str::CompactString;
use smallvec::SmallVec;

use crate::{Value, ValueRef};

/// One optionally named value in an [`Arguments`] sequence.
///
/// Names use inline storage when possible. This avoids a heap allocation for
/// the short option and URI-style names that dominate the C++ call sites.
#[derive(Clone, Debug, PartialEq)]
pub struct Argument {
    name: Option<CompactString>,
    value: Value,
}

impl Argument {
    /// Creates a named argument.
    pub fn named(name: impl Into<CompactString>, value: impl Into<Value>) -> Self {
        Self {
            name: Some(name.into()),
            value: value.into(),
        }
    }

    /// Creates an unnamed positional argument.
    pub fn positional(value: impl Into<Value>) -> Self {
        Self {
            name: None,
            value: value.into(),
        }
    }

    /// Returns the optional argument name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns a shared reference to the owned value.
    #[must_use]
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Returns a borrowed view of the value.
    #[must_use]
    pub fn value_ref(&self) -> ValueRef<'_> {
        self.value.as_ref()
    }

    /// Returns a mutable reference to the value.
    pub const fn value_mut(&mut self) -> &mut Value {
        &mut self.value
    }

    /// Splits the argument into its name and value.
    #[must_use]
    pub fn into_parts(self) -> (Option<CompactString>, Value) {
        (self.name, self.value)
    }
}

/// An insertion-ordered collection of named and positional dynamic values.
///
/// Unlike a map, `Arguments` preserves duplicate names and unnamed entries.
/// Positional access is O(1). Name lookup is O(n) and deliberately allocation
/// free; build an [`ArgumentIndex`] when a stable collection will be queried
/// repeatedly by name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arguments {
    entries: Vec<Argument>,
}

impl Arguments {
    /// Creates an empty argument collection.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Creates an empty collection with space for at least `capacity` entries.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            entries: Vec::with_capacity(capacity),
        }
    }

    /// Returns the number of arguments.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the collection has no arguments.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the number of entries that fit without reallocating.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    /// Reserves space for at least `additional` more entries.
    pub fn reserve(&mut self, additional: usize) {
        self.entries.reserve(additional);
    }

    /// Appends an already constructed argument.
    pub fn push(&mut self, argument: Argument) {
        self.entries.push(argument);
    }

    /// Constructs and appends a named argument.
    pub fn push_named(&mut self, name: impl Into<CompactString>, value: impl Into<Value>) {
        self.push(Argument::named(name, value));
    }

    /// Constructs and appends an unnamed positional argument.
    pub fn push_positional(&mut self, value: impl Into<Value>) {
        self.push(Argument::positional(value));
    }

    /// Returns the argument at `position` in O(1) time.
    #[must_use]
    pub fn get(&self, position: usize) -> Option<&Argument> {
        self.entries.get(position)
    }

    /// Returns the mutable argument at `position` in O(1) time.
    pub fn get_mut(&mut self, position: usize) -> Option<&mut Argument> {
        self.entries.get_mut(position)
    }

    /// Returns the first argument with `name` using an O(n) scan.
    #[must_use]
    pub fn get_named(&self, name: &str) -> Option<&Argument> {
        self.entries
            .iter()
            .find(|argument| argument.name() == Some(name))
    }

    /// Returns the `occurrence`th argument with `name` using an O(n) scan.
    ///
    /// Occurrences are zero-based and follow insertion order.
    #[must_use]
    pub fn get_nth_named(&self, name: &str, occurrence: usize) -> Option<&Argument> {
        self.entries
            .iter()
            .filter(|argument| argument.name() == Some(name))
            .nth(occurrence)
    }

    /// Returns whether at least one argument has `name` using an O(n) scan.
    #[must_use]
    pub fn contains_name(&self, name: &str) -> bool {
        self.get_named(name).is_some()
    }

    /// Removes and returns the argument at `position`.
    ///
    /// This is O(n) because later arguments retain their insertion order.
    pub fn remove(&mut self, position: usize) -> Option<Argument> {
        (position < self.len()).then(|| self.entries.remove(position))
    }

    /// Removes and returns the first argument with `name`.
    ///
    /// Finding and compacting the sequence are both O(n).
    pub fn remove_named(&mut self, name: &str) -> Option<Argument> {
        let position = self
            .entries
            .iter()
            .position(|argument| argument.name() == Some(name))?;
        Some(self.entries.remove(position))
    }

    /// Removes every argument while retaining allocated entry capacity.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Returns an iterator in insertion order.
    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &Argument> + DoubleEndedIterator {
        self.entries.iter()
    }

    /// Builds an `ahash` name index borrowing this immutable collection.
    ///
    /// Construction is expected O(n) and allocates the hash table plus
    /// overflow storage only for duplicate names. The borrow prevents all
    /// mutations while the index exists, so stale offsets are impossible.
    #[must_use]
    pub fn index(&self) -> ArgumentIndex<'_> {
        ArgumentIndex::new(self)
    }
}

impl FromIterator<Argument> for Arguments {
    fn from_iter<T: IntoIterator<Item = Argument>>(iter: T) -> Self {
        Self {
            entries: iter.into_iter().collect(),
        }
    }
}

impl Extend<Argument> for Arguments {
    fn extend<T: IntoIterator<Item = Argument>>(&mut self, iter: T) {
        self.entries.extend(iter);
    }
}

impl<'a> IntoIterator for &'a Arguments {
    type Item = &'a Argument;
    type IntoIter = std::slice::Iter<'a, Argument>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

impl IntoIterator for Arguments {
    type Item = Argument;
    type IntoIter = std::vec::IntoIter<Argument>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.into_iter()
    }
}

/// A reusable name index over an immutable [`Arguments`] collection.
///
/// The index uses [`ahash`](https://crates.io/crates/ahash) and therefore
/// assumes trusted/non-adversarial keys. Names are borrowed from the source and
/// each name usually stores its sole position inline. Name lookup is expected
/// O(name length), while duplicate occurrence lookup is O(1).
#[derive(Clone, Debug)]
pub struct ArgumentIndex<'a> {
    arguments: &'a Arguments,
    by_name: AHashMap<&'a str, SmallVec<[usize; 1]>>,
}

impl<'a> ArgumentIndex<'a> {
    /// Builds a reusable name index over `arguments`.
    #[must_use]
    pub fn new(arguments: &'a Arguments) -> Self {
        let mut by_name: AHashMap<&'a str, SmallVec<[usize; 1]>> =
            AHashMap::with_capacity(arguments.len());
        for (position, argument) in arguments.iter().enumerate() {
            if let Some(name) = argument.name() {
                by_name.entry(name).or_default().push(position);
            }
        }
        Self { arguments, by_name }
    }

    /// Returns the total number of arguments, including unnamed entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.arguments.len()
    }

    /// Returns whether the source contains no arguments.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.arguments.is_empty()
    }

    /// Returns the number of distinct names in the hash index.
    #[must_use]
    pub fn distinct_name_count(&self) -> usize {
        self.by_name.len()
    }

    /// Returns an argument by position in O(1) time.
    #[must_use]
    pub fn get(&self, position: usize) -> Option<&'a Argument> {
        self.arguments.get(position)
    }

    /// Returns the first argument with `name` in expected O(name length).
    #[must_use]
    pub fn get_named(&self, name: &str) -> Option<&'a Argument> {
        self.get_nth_named(name, 0)
    }

    /// Returns the `occurrence`th argument with `name` in expected O(1) after hashing.
    #[must_use]
    pub fn get_nth_named(&self, name: &str, occurrence: usize) -> Option<&'a Argument> {
        let position = *self.by_name.get(name)?.get(occurrence)?;
        self.arguments.get(position)
    }

    /// Returns every insertion position associated with `name`.
    #[must_use]
    pub fn positions(&self, name: &str) -> &[usize] {
        self.by_name.get(name).map_or(&[], SmallVec::as_slice)
    }

    /// Returns whether the hash index contains `name`.
    #[must_use]
    pub fn contains_name(&self, name: &str) -> bool {
        self.by_name.contains_key(name)
    }

    /// Returns the borrowed source collection.
    #[must_use]
    pub const fn arguments(&self) -> &'a Arguments {
        self.arguments
    }
}
