use crate::{values::CustomValueSupport, PolarsPlugin};

use super::super::values::{Column, NuDataFrame};

use nu_plugin::{EngineInterface, EvaluatedCall, PluginCommand};
use nu_protocol::{
    Category, Example, LabeledError, PipelineData, ShellResult, Signature, Span, Type, Value,
};
use polars::prelude::{ArgAgg, IntoSeries, NewChunkedArray, UInt32Chunked};

#[derive(Clone)]
pub struct ArgMin;

impl PluginCommand for ArgMin {
    type Plugin = PolarsPlugin;

    fn name(&self) -> &str {
        "polars arg-min"
    }

    fn usage(&self) -> &str {
        "Return index for min value in series."
    }

    fn search_terms(&self) -> Vec<&str> {
        vec!["argmin", "minimum", "least", "smallest", "lowest"]
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
            description: "Returns index for min value",
            example: "[1 3 2] | polars into-df | polars arg-min",
            result: Some(
                NuDataFrame::try_from_columns(
                    vec![Column::new("arg_min".to_string(), vec![Value::test_int(0)])],
                    None,
                )
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
    let df = NuDataFrame::try_from_pipeline_coerce(plugin, input, call.head)?;
    let series = df.as_series(call.head)?;

    let res = series.arg_min();
    let chunked = match res {
        Some(index) => UInt32Chunked::from_slice("arg_min", &[index as u32]),
        None => UInt32Chunked::from_slice("arg_min", &[]),
    };

    let res = chunked.into_series();
    let df = NuDataFrame::try_from_series_vec(vec![res], call.head)?;
    df.to_pipeline_data(plugin, engine, call.head)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test::test_polars_plugin_command;

    #[test]
    fn test_examples() -> ShellResult<()> {
        test_polars_plugin_command(&ArgMin)
    }
}
