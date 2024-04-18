use nu_engine::command_prelude::*;

use rand::prelude::{thread_rng, Rng};

#[derive(Clone)]
pub struct SubCommand;

impl Command for SubCommand {
    fn name(&self) -> &str {
        "random bool"
    }

    fn signature(&self) -> Signature {
        Signature::build("random bool")
            .input_output_types(vec![(Type::Nothing, Type::Bool)])
            .allow_variants_without_examples(true)
            .named(
                "bias",
                SyntaxShape::Number,
                "Adjusts the probability of a \"true\" outcome",
                Some('b'),
            )
            .category(Category::Random)
    }

    fn usage(&self) -> &str {
        "Generate a random boolean value."
    }

    fn search_terms(&self) -> Vec<&str> {
        vec!["generate", "boolean", "true", "false", "1", "0"]
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> ShellResult<PipelineData> {
        bool(engine_state, stack, call)
    }

    fn examples(&self) -> Vec<Example> {
        vec![
            Example {
                description: "Generate a random boolean value",
                example: "random bool",
                result: None,
            },
            Example {
                description: "Generate a random boolean value with a 75% chance of \"true\"",
                example: "random bool --bias 0.75",
                result: None,
            },
        ]
    }
}

fn bool(engine_state: &EngineState, stack: &mut Stack, call: &Call) -> ShellResult<PipelineData> {
    let span = call.head;
    let bias: Option<Spanned<f64>> = call.get_flag(engine_state, stack, "bias")?;

    let mut probability = 0.5;

    if let Some(prob) = bias {
        probability = prob.item;

        let probability_is_valid = (0.0..=1.0).contains(&probability);

        if !probability_is_valid {
            Err(ShellError::InvalidProbability { span: prob.span })?;
        }
    }

    let mut rng = thread_rng();
    let bool_result: bool = rng.gen_bool(probability);

    Ok(PipelineData::Value(Value::bool(bool_result, span), None))
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_examples() {
        use crate::test_examples;

        test_examples(SubCommand {})
    }
}
