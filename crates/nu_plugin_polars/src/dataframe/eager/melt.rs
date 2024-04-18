use nu_plugin::{EngineInterface, EvaluatedCall, PluginCommand};
use nu_protocol::{
    Category, Example, LabeledError, PipelineData, ShellError, ShellResult, Signature, Span,
    Spanned, SyntaxShape, Type, Value,
};

use crate::{
    dataframe::values::utils::convert_columns_string, values::CustomValueSupport, PolarsPlugin,
};

use super::super::values::{Column, NuDataFrame};

#[derive(Clone)]
pub struct MeltDF;

impl PluginCommand for MeltDF {
    type Plugin = PolarsPlugin;

    fn name(&self) -> &str {
        "polars melt"
    }

    fn usage(&self) -> &str {
        "Unpivot a DataFrame from wide to long format."
    }

    fn signature(&self) -> Signature {
        Signature::build(self.name())
            .required_named(
                "columns",
                SyntaxShape::Table(vec![]),
                "column names for melting",
                Some('c'),
            )
            .required_named(
                "values",
                SyntaxShape::Table(vec![]),
                "column names used as value columns",
                Some('v'),
            )
            .named(
                "variable-name",
                SyntaxShape::String,
                "optional name for variable column",
                Some('r'),
            )
            .named(
                "value-name",
                SyntaxShape::String,
                "optional name for value column",
                Some('l'),
            )
            .input_output_type(
                Type::Custom("dataframe".into()),
                Type::Custom("dataframe".into()),
            )
            .category(Category::Custom("dataframe".into()))
    }

    fn examples(&self) -> Vec<Example> {
        vec![Example {
            description: "melt dataframe",
            example:
                "[[a b c d]; [x 1 4 a] [y 2 5 b] [z 3 6 c]] | polars into-df | polars melt -c [b c] -v [a d]",
            result: Some(
                NuDataFrame::try_from_columns(vec![
                    Column::new(
                        "b".to_string(),
                        vec![
                            Value::test_int(1),
                            Value::test_int(2),
                            Value::test_int(3),
                            Value::test_int(1),
                            Value::test_int(2),
                            Value::test_int(3),
                        ],
                    ),
                    Column::new(
                        "c".to_string(),
                        vec![
                            Value::test_int(4),
                            Value::test_int(5),
                            Value::test_int(6),
                            Value::test_int(4),
                            Value::test_int(5),
                            Value::test_int(6),
                        ],
                    ),
                    Column::new(
                        "variable".to_string(),
                        vec![
                            Value::test_string("a"),
                            Value::test_string("a"),
                            Value::test_string("a"),
                            Value::test_string("d"),
                            Value::test_string("d"),
                            Value::test_string("d"),
                        ],
                    ),
                    Column::new(
                        "value".to_string(),
                        vec![
                            Value::test_string("x"),
                            Value::test_string("y"),
                            Value::test_string("z"),
                            Value::test_string("a"),
                            Value::test_string("b"),
                            Value::test_string("c"),
                        ],
                    ),
                ], None)
                .expect("simple df for test should not fail")
                .into_value(Span::test_data()),
            ),
        }]
    }

    fn run(
        &self,
        plugin: &Self::Plugin,
        engine: &EngineInterface,
        call: &EvaluatedCall,
        input: PipelineData,
    ) -> Result<PipelineData, LabeledError> {
        command(plugin, engine, call, input).map_err(LabeledError::from)
    }
}

fn command(
    plugin: &PolarsPlugin,
    engine: &EngineInterface,
    call: &EvaluatedCall,
    input: PipelineData,
) -> ShellResult<PipelineData> {
    let id_col: Vec<Value> = call.get_flag("columns")?.expect("required value");
    let val_col: Vec<Value> = call.get_flag("values")?.expect("required value");

    let value_name: Option<Spanned<String>> = call.get_flag("value-name")?;
    let variable_name: Option<Spanned<String>> = call.get_flag("variable-name")?;

    let (id_col_string, id_col_span) = convert_columns_string(id_col, call.head)?;
    let (val_col_string, val_col_span) = convert_columns_string(val_col, call.head)?;

    let df = NuDataFrame::try_from_pipeline_coerce(plugin, input, call.head)?;

    check_column_datatypes(df.as_ref(), &id_col_string, id_col_span)?;
    check_column_datatypes(df.as_ref(), &val_col_string, val_col_span)?;

    let mut res = df
        .as_ref()
        .melt(&id_col_string, &val_col_string)
        .map_err(|e| ShellError::GenericError {
            error: "Error calculating melt".into(),
            msg: e.to_string(),
            span: Some(call.head),
            help: None,
            inner: vec![],
        })?;

    if let Some(name) = &variable_name {
        res.rename("variable", &name.item)
            .map_err(|e| ShellError::GenericError {
                error: "Error renaming column".into(),
                msg: e.to_string(),
                span: Some(name.span),
                help: None,
                inner: vec![],
            })?;
    }

    if let Some(name) = &value_name {
        res.rename("value", &name.item)
            .map_err(|e| ShellError::GenericError {
                error: "Error renaming column".into(),
                msg: e.to_string(),
                span: Some(name.span),
                help: None,
                inner: vec![],
            })?;
    }

    let res = NuDataFrame::new(false, res);
    res.to_pipeline_data(plugin, engine, call.head)
}

fn check_column_datatypes<T: AsRef<str>>(
    df: &polars::prelude::DataFrame,
    cols: &[T],
    col_span: Span,
) -> ShellResult<()> {
    if cols.is_empty() {
        Err(ShellError::GenericError {
            error: "Merge error".into(),
            msg: "empty column list".into(),
            span: Some(col_span),
            help: None,
            inner: vec![],
        })?;
    }

    // Checking if they are same type
    if cols.len() > 1 {
        for w in cols.windows(2) {
            let l_series = df
                .column(w[0].as_ref())
                .map_err(|e| ShellError::GenericError {
                    error: "Error selecting columns".into(),
                    msg: e.to_string(),
                    span: Some(col_span),
                    help: None,
                    inner: vec![],
                })?;

            let r_series = df
                .column(w[1].as_ref())
                .map_err(|e| ShellError::GenericError {
                    error: "Error selecting columns".into(),
                    msg: e.to_string(),
                    span: Some(col_span),
                    help: None,
                    inner: vec![],
                })?;

            if l_series.dtype() != r_series.dtype() {
                Err(ShellError::GenericError {
                    error: "Merge error".into(),
                    msg: "found different column types in list".into(),
                    span: Some(col_span),
                    help: Some(format!(
                        "datatypes {} and {} are incompatible",
                        l_series.dtype(),
                        r_series.dtype()
                    )),
                    inner: vec![],
                })?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use crate::test::test_polars_plugin_command;

    use super::*;

    #[test]
    fn test_examples() -> ShellResult<()> {
        test_polars_plugin_command(&MeltDF)
    }
}
