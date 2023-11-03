use crate::Value;

use ecow::{EcoString, EcoVec};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Record {
    pub cols: EcoVec<EcoString>,
    pub vals: EcoVec<Value>,
}

impl Record {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            cols: EcoVec::with_capacity(capacity),
            vals: EcoVec::with_capacity(capacity),
        }
    }

    // Constructor that checks that `cols` and `vals` are of the same length.
    //
    // For perf reasons does not validate the rest of the record assumptions.
    // - unique keys
    pub fn from_raw_cols_vals(cols: EcoVec<EcoString>, vals: EcoVec<Value>) -> Self {
        assert_eq!(cols.len(), vals.len());

        Self { cols, vals }
    }

    pub fn iter(&self) -> Iter {
        self.into_iter()
    }

    pub fn iter_mut(&mut self) -> IterMut {
        self.into_iter()
    }

    pub fn is_empty(&self) -> bool {
        self.cols.is_empty() || self.vals.is_empty()
    }

    pub fn len(&self) -> usize {
        usize::min(self.cols.len(), self.vals.len())
    }

    /// Naive push to the end of the datastructure.
    ///
    /// May duplicate data!
    ///
    /// Consider to use [`Record::insert`] instead
    pub fn push(&mut self, col: impl Into<EcoString>, val: Value) {
        self.cols.push(col.into());
        self.vals.push(val);
    }

    /// Insert into the record, replacing preexisting value if found.
    ///
    /// Returns `Some(previous_value)` if found. Else `None`
    pub fn insert<K>(&mut self, col: K, val: Value) -> Option<Value>
    where
        K: AsRef<str> + Into<EcoString>,
    {
        if let Some(idx) = self.index_of(&col) {
            // Can panic if vals.len() < cols.len()
            let curr_val = &mut self.vals.make_mut()[idx];
            Some(std::mem::replace(curr_val, val))
        } else {
            self.cols.push(col.into());
            self.vals.push(val);
            None
        }
    }

    pub fn contains(&self, col: impl AsRef<str>) -> bool {
        self.cols.iter().any(|k| k == col.as_ref())
    }

    pub fn index_of(&self, col: impl AsRef<str>) -> Option<usize> {
        self.columns().position(|k| k == col.as_ref())
    }

    pub fn get(&self, col: impl AsRef<str>) -> Option<&Value> {
        self.index_of(col).and_then(|idx| self.vals.get(idx))
    }

    pub fn get_mut(&mut self, col: impl AsRef<str>) -> Option<&mut Value> {
        self.index_of(col)
            .and_then(|idx| self.vals.make_mut().get_mut(idx))
    }

    pub fn get_index(&self, idx: usize) -> Option<(&EcoString, &Value)> {
        Some((self.cols.get(idx)?, self.vals.get(idx)?))
    }

    /// Remove single value by key
    ///
    /// Returns `None` if key not found
    ///
    /// Note: makes strong assumption that keys are unique
    pub fn remove(&mut self, col: impl AsRef<str>) -> Option<Value> {
        let idx = self.index_of(col)?;
        self.cols.remove(idx);
        Some(self.vals.remove(idx))
    }

    /// Remove elements in-place that do not satisfy `keep`
    ///
    /// Note: Panics if `vals.len() > cols.len()`
    /// ```rust
    /// use nu_protocol::{record, Value};
    ///
    /// let mut rec = record!(
    ///     "a" => Value::test_nothing(),
    ///     "b" => Value::test_int(42),
    ///     "c" => Value::test_nothing(),
    ///     "d" => Value::test_int(42),
    ///     );
    /// rec.retain(|_k, val| !val.is_nothing());
    /// let mut iter_rec = rec.columns();
    /// assert_eq!(iter_rec.next().map(String::as_str), Some("b"));
    /// assert_eq!(iter_rec.next().map(String::as_str), Some("d"));
    /// assert_eq!(iter_rec.next(), None);
    /// ```
    pub fn retain<F>(&mut self, mut keep: F)
    where
        F: FnMut(&str, &Value) -> bool,
    {
        self.retain_mut(|k, v| keep(k, v));
    }

    /// Remove elements in-place that do not satisfy `keep` while allowing mutation of the value.
    ///
    /// This can for example be used to recursively prune nested records.
    ///
    /// Note: Panics if `vals.len() > cols.len()`
    /// ```rust
    /// use nu_protocol::{record, Record, Value};
    ///
    /// fn remove_foo_recursively(val: &mut Value) {
    ///     if let Value::Record {val, ..} = val {
    ///         val.retain_mut(keep_non_foo);
    ///     }
    /// }
    ///
    /// fn keep_non_foo(k: &str, v: &mut Value) -> bool {
    ///     if k == "foo" {
    ///         return false;
    ///     }
    ///     remove_foo_recursively(v);
    ///     true
    /// }
    ///
    /// let mut test = Value::test_record(record!(
    ///     "foo" => Value::test_nothing(),
    ///     "bar" => Value::test_record(record!(
    ///         "foo" => Value::test_nothing(),
    ///         "baz" => Value::test_nothing(),
    ///         ))
    ///     ));
    ///
    /// remove_foo_recursively(&mut test);
    /// let expected = Value::test_record(record!(
    ///     "bar" => Value::test_record(record!(
    ///         "baz" => Value::test_nothing(),
    ///         ))
    ///     ));
    /// assert_eq!(test, expected);
    /// ```
    pub fn retain_mut<F>(&mut self, mut keep: F)
    where
        F: FnMut(&str, &mut Value) -> bool,
    {
        // `Vec::retain` is able to optimize memcopies internally.
        // For maximum benefit, `retain` is used on `vals`,
        // as `Value` is a larger struct than `String`.
        //
        // To do a simultaneous retain on the `cols`, three portions of it are tracked:
        //     [..retained, ..dropped, ..unvisited]

        // number of elements keep so far, start of ..dropped and length of ..retained
        let mut retained = 0;
        let len = self.len();
        let vals = self.vals.make_mut();
        let cols = self.cols.make_mut();
        // idx is the current element being checked, start of ..unvisited
        for idx in 0..len {
            if keep(&cols[idx], &mut vals[idx]) {
                // skip swaps for first consecutive run of kept elements
                if idx != retained {
                    cols.swap(idx, retained);
                    vals.swap(idx, retained);
                }
                retained += 1;
            }
        }

        self.vals.truncate(retained);
        self.cols.truncate(retained);
    }

    pub fn columns(&self) -> Columns {
        Columns {
            iter: self.cols.iter(),
        }
    }

    pub fn values(&self) -> Values {
        Values {
            iter: self.vals.iter(),
        }
    }

    pub fn into_values(self) -> IntoValues {
        IntoValues {
            iter: self.vals.into_iter(),
        }
    }
}

impl FromIterator<(EcoString, Value)> for Record {
    fn from_iter<T: IntoIterator<Item = (EcoString, Value)>>(iter: T) -> Self {
        let (cols, vals) = iter.into_iter().unzip();
        Self { cols, vals }
    }
}

impl Extend<(EcoString, Value)> for Record {
    fn extend<T: IntoIterator<Item = (EcoString, Value)>>(&mut self, iter: T) {
        for (k, v) in iter {
            // TODO: should this .insert with a check?
            self.cols.push(k);
            self.vals.push(v);
        }
    }
}

pub type IntoIter = std::iter::Zip<ecow::vec::IntoIter<EcoString>, ecow::vec::IntoIter<Value>>;

impl IntoIterator for Record {
    type Item = (EcoString, Value);

    type IntoIter = IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.cols.into_iter().zip(self.vals)
    }
}

pub type Iter<'a> = std::iter::Zip<std::slice::Iter<'a, EcoString>, std::slice::Iter<'a, Value>>;

impl<'a> IntoIterator for &'a Record {
    type Item = (&'a EcoString, &'a Value);

    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.cols.iter().zip(&self.vals)
    }
}

pub type IterMut<'a> =
    std::iter::Zip<std::slice::Iter<'a, EcoString>, std::slice::IterMut<'a, Value>>;

impl<'a> IntoIterator for &'a mut Record {
    type Item = (&'a EcoString, &'a mut Value);

    type IntoIter = IterMut<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.cols.iter().zip(self.vals.make_mut())
    }
}

pub struct Columns<'a> {
    iter: std::slice::Iter<'a, EcoString>,
}

impl<'a> Iterator for Columns<'a> {
    type Item = &'a EcoString;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<'a> DoubleEndedIterator for Columns<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back()
    }
}

impl<'a> ExactSizeIterator for Columns<'a> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

pub struct Values<'a> {
    iter: std::slice::Iter<'a, Value>,
}

impl<'a> Iterator for Values<'a> {
    type Item = &'a Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<'a> DoubleEndedIterator for Values<'a> {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back()
    }
}

impl<'a> ExactSizeIterator for Values<'a> {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

pub struct IntoValues {
    iter: ecow::vec::IntoIter<Value>,
}

impl Iterator for IntoValues {
    type Item = Value;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl DoubleEndedIterator for IntoValues {
    fn next_back(&mut self) -> Option<Self::Item> {
        self.iter.next_back()
    }
}

impl ExactSizeIterator for IntoValues {
    fn len(&self) -> usize {
        self.iter.len()
    }
}

#[macro_export]
macro_rules! record {
    {$($col:expr => $val:expr),+ $(,)?} => {
        $crate::Record::from_raw_cols_vals (
            ecow::eco_vec![$($col.into(),)+],
            ecow::eco_vec![$($val,)+]
        )
    };
    {} => {
        $crate::Record::new()
    };
}
