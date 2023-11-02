use calamine::*;
use indexmap::map::IndexMap;
use nu_engine::CallExt;
use nu_protocol::ast::Call;
use nu_protocol::engine::{Command, EngineState, Stack};
use nu_protocol::{
    Category, Example, PipelineData, ShellError, Signature, Span, SyntaxShape, Type, Value,
};
use std::io::Cursor;

#[derive(Clone)]
pub struct FromOds;

impl Command for FromOds {
    fn name(&self) -> &str {
        "from ods"
    }

    fn signature(&self) -> Signature {
        Signature::build("from ods")
            .input_output_types(vec![(Type::String, Type::Table(vec![]))])
            .allow_variants_without_examples(true)
            .named(
                "sheets",
                SyntaxShape::List(Box::new(SyntaxShape::String)),
                "Only convert specified sheets",
                Some('s'),
            )
            .category(Category::Formats)
    }

    fn usage(&self) -> &str {
        "Parse OpenDocument Spreadsheet(.ods) data and create table."
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;

        let sel_sheets = if let Some(Value::List { vals: columns, .. }) =
            call.get_flag(engine_state, stack, "sheets")?
        {
            convert_columns(columns.as_slice())?
        } else {
            vec![]
        };

        from_ods(input, head, sel_sheets)
    }

    fn examples(&self) -> Vec<Example> {
        vec![
            Example {
                description: "Convert binary .ods data to a table",
                example: "open --raw test.ods | from ods",
                result: None,
            },
            Example {
                description: "Convert binary .ods data to a table, specifying the tables",
                example: "open --raw test.ods | from ods --sheets [Spreadsheet1]",
                result: None,
            },
        ]
    }
}

fn convert_columns(columns: &[Value]) -> Result<Vec<String>, ShellError> {
    let res = columns
        .iter()
        .map(|value| match &value {
            Value::String { val: s, .. } => Ok(s.clone()),
            _ => Err(ShellError::IncompatibleParametersSingle {
                msg: "Incorrect column format, Only string as column name".to_string(),
                span: value.span(),
            }),
        })
        .collect::<Result<Vec<String>, _>>()?;

    Ok(res)
}

fn collect_binary(input: PipelineData, span: Span) -> Result<Vec<u8>, ShellError> {
    let mut bytes = vec![];
    let mut values = input.into_iter();

    loop {
        match values.next() {
            Some(Value::Binary { val: b, .. }) => {
                bytes.extend_from_slice(&b);
            }
            Some(Value::Error { error, .. }) => return Err(*error),
            Some(x) => {
                return Err(ShellError::UnsupportedInput(
                    "Expected binary from pipeline".to_string(),
                    "value originates from here".into(),
                    span,
                    x.span(),
                ))
            }
            None => break,
        }
    }

    Ok(bytes)
}

fn from_ods(
    input: PipelineData,
    head: Span,
    sel_sheets: Vec<String>,
) -> Result<PipelineData, ShellError> {
    let span = input.span();
    let bytes = collect_binary(input, head)?;
    let buf: Cursor<Vec<u8>> = Cursor::new(bytes);
    let mut ods = Ods::<_>::new(buf).map_err(|_| {
        ShellError::UnsupportedInput(
            "Could not load ODS file".to_string(),
            "value originates from here".into(),
            head,
            span.unwrap_or(head),
        )
    })?;

    let mut dict = IndexMap::new();

    let mut sheet_names = ods.sheet_names();
    if !sel_sheets.is_empty() {
        sheet_names.retain(|e| sel_sheets.contains(e));
    }

    for sheet_name in sheet_names {
        if let Some(Ok(current_sheet)) = ods.worksheet_range(&sheet_name) {
            let sheet_output = current_sheet
                .rows()
                .map(|row| {
                    let record = row
                        .iter()
                        .enumerate()
                        .map(|(i, cell)| {
                            let value = match cell {
                                DataType::Empty => Value::nothing(head),
                                DataType::String(s) => Value::string(s, head),
                                DataType::Float(f) => Value::float(*f, head),
                                DataType::Int(i) => Value::int(*i, head),
                                DataType::Bool(b) => Value::bool(*b, head),
                                _ => Value::nothing(head),
                            };

                            (format!("column{i}"), value)
                        })
                        .collect();

                    Value::record(record, head)
                })
                .collect();

            dict.insert(sheet_name, Value::list(sheet_output, head));
        } else {
            return Err(ShellError::UnsupportedInput(
                "Could not load sheet".to_string(),
                "value originates from here".into(),
                head,
                span.unwrap_or(head),
            ));
        }
    }

    Ok(PipelineData::Value(
        Value::record(dict.into_iter().collect(), head),
        None,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_examples() {
        use crate::test_examples;

        test_examples(FromOds {})
    }
}
