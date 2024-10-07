use indexmap::IndexMap;
use nu_engine::command_prelude::*;

#[derive(Clone)]
pub struct SplitBy;

impl Command for SplitBy {
    fn name(&self) -> &str {
        "split-by"
    }

    fn signature(&self) -> Signature {
        Signature::build("split-by")
            .input_output_types(vec![(Type::record(), Type::record())])
            .required("splitter", SyntaxShape::Any, "The splitter value to use.")
            .category(Category::Filters)
    }

    fn description(&self) -> &str {
        "Split a record into groups."
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        split_by(engine_state, stack, call, input)
    }

    fn examples(&self) -> Vec<Example> {
        vec![Example {
            description: "split items by column named \"lang\"",
            example: r#"{
    '2019': [
        { name: 'andres', lang: 'rb', year: '2019' },
        { name: 'jt', lang: 'rs', year: '2019' }
    ],
    '2021': [
        { name: 'storm', lang: 'rs', 'year': '2021' }
    ]
    } | split-by lang"#,
            result: Some(Value::test_record(record! {
                    "rb" => Value::test_record(record! {
                        "2019" => Value::test_list(
                            vec![Value::test_record(record! {
                                    "name" => Value::test_string("andres"),
                                    "lang" => Value::test_string("rb"),
                                    "year" => Value::test_string("2019"),
                            })],
                        ),
                    }),
                    "rs" => Value::test_record(record! {
                            "2019" => Value::test_list(
                                vec![Value::test_record(record! {
                                        "name" => Value::test_string("jt"),
                                        "lang" => Value::test_string("rs"),
                                        "year" => Value::test_string("2019"),
                                })],
                            ),
                            "2021" => Value::test_list(
                                vec![Value::test_record(record! {
                                        "name" => Value::test_string("storm"),
                                        "lang" => Value::test_string("rs"),
                                        "year" => Value::test_string("2021"),
                                })],
                            ),
                    }),
            })),
        }]
    }
}

fn split_by(
    engine_state: &EngineState,
    stack: &mut Stack,
    call: &Call,
    input: PipelineData,
) -> Result<PipelineData, ShellError> {
    let head = call.head;
    let splitter: Value = call.req(engine_state, stack, 0)?;

    if let PipelineData::Value(value, ..) = input {
        let column = Spanned {
            span: splitter.span(),
            item: splitter.coerce_into_string()?,
        };
        let record = Spanned {
            span: value.span(),
            item: value.into_record()?,
        };
        Ok(split(record, &column, head)?)
    } else {
        Err(input.unsupported_input_error("record", head))
    }
}

fn data_group(
    values: &Value,
    column_name: &Spanned<String>,
    span: Span,
) -> Result<Value, ShellError> {
    let mut groups: IndexMap<String, Vec<Value>> = IndexMap::new();

    for value in values.clone().into_pipeline_data().into_iter() {
        let key = value
            .as_record()?
            .get(&column_name.item)
            .ok_or_else(|| ShellError::CantFindColumn {
                col_name: column_name.item.clone(),
                span: Some(column_name.span),
                src_span: value.span(),
            })?
            .coerce_str()?
            .into_owned();

        groups.entry(key).or_default().push(value);
    }

    Ok(groups
        .into_iter()
        .map(|(k, v)| (k, Value::list(v, span)))
        .collect::<Record>()
        .into_value(span))
}

fn split(
    record: Spanned<Record>,
    column_name: &Spanned<String>,
    head: Span,
) -> Result<PipelineData, ShellError> {
    let mut splits = indexmap::IndexMap::new();

    for (outer_key, list) in record.item.iter() {
        match data_group(list, column_name, record.span) {
            Ok(grouped_vals) => {
                if let Value::Record { val: sub, .. } = grouped_vals {
                    for (inner_key, subset) in sub.into_owned() {
                        let s: &mut IndexMap<String, Value> = splits.entry(inner_key).or_default();

                        s.insert(outer_key.clone(), subset.clone());
                    }
                }
            }
            Err(reason) => return Err(reason),
        }
    }

    let record = splits
        .into_iter()
        .map(|(k, rows)| (k, Value::record(rows.into_iter().collect(), head)))
        .collect::<Record>();

    Ok(record.into_value(head).into_pipeline_data())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_examples() {
        use crate::test_examples;

        test_examples(SplitBy {})
    }
}
