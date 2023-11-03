use ecow::{EcoString, EcoVec};
use nu_protocol::Value;
use std::collections::HashSet;

pub fn get_columns(input: &[Value]) -> EcoVec<EcoString> {
    let mut columns = EcoVec::new();
    for item in input {
        let Value::Record { val, .. } = item else {
            return EcoVec::new();
        };

        for col in val.columns() {
            if !columns.contains(col) {
                columns.push(col.clone());
            }
        }
    }

    columns
}

// If a column doesn't exist in the input, return it.
pub fn nonexistent_column(inputs: &[String], columns: &[EcoString]) -> Option<String> {
    let set = columns
        .iter()
        .map(EcoString::as_str)
        .collect::<HashSet<_>>();

    for input in inputs {
        if set.contains(input.as_str()) {
            continue;
        }
        return Some(input.clone());
    }
    None
}
