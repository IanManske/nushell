use crate::dataframe::values::{Column, NuDataFrame};
use nu_engine::command_prelude::*;
use polars::prelude::IntoSeries;

#[derive(Clone)]
pub struct ArgUnique;

impl Command for ArgUnique {
    fn name(&self) -> &str {
        "dfr arg-unique"
    }

    fn usage(&self) -> &str {
        "Returns indexes for unique values."
    }

    fn search_terms(&self) -> Vec<&str> {
        vec!["argunique", "distinct", "noduplicate", "unrepeated"]
    }

    fn signature(&self) -> Signature {
        Signature::build(self.name())
            .input_output_type(
                Type::Custom("dataframe".into()),
                Type::Custom("dataframe".into()),
            )
            .category(Category::Custom("dataframe".into()))
    }

    fn examples(&self) -> Vec<Example> {
        vec![Example {
            description: "Returns indexes for unique values",
            example: "[1 2 2 3 3] | dfr into-df | dfr arg-unique",
            result: Some(
                NuDataFrame::try_from_columns(
                    vec![Column::new(
                        "arg_unique".to_string(),
                        vec![Value::test_int(0), Value::test_int(1), Value::test_int(3)],
                    )],
                    None,
                )
                .expect("simple df for test should not fail")
                .into_value(Span::test_data()),
            ),
        }]
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> ShellResult<PipelineData> {
        command(engine_state, stack, call, input)
    }
}

fn command(
    _engine_state: &EngineState,
    _stack: &mut Stack,
    call: &Call,
    input: PipelineData,
) -> ShellResult<PipelineData> {
    let df = NuDataFrame::try_from_pipeline(input, call.head)?;

    let mut res = df
        .as_series(call.head)?
        .arg_unique()
        .map_err(|e| ShellError::GenericError {
            error: "Error extracting unique values".into(),
            msg: e.to_string(),
            span: Some(call.head),
            help: None,
            inner: vec![],
        })?
        .into_series();
    res.rename("arg_unique");

    NuDataFrame::try_from_series(vec![res], call.head)
        .map(|df| PipelineData::Value(NuDataFrame::into_value(df, call.head), None))
}

#[cfg(test)]
mod test {
    use super::super::super::super::test_dataframe::test_dataframe;
    use super::*;

    #[test]
    fn test_examples() {
        test_dataframe(vec![Box::new(ArgUnique {})])
    }
}
