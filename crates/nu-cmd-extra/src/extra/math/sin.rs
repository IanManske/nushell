use nu_protocol::ast::Call;
use nu_protocol::engine::{Command, EngineState, Stack};
use nu_protocol::{Category, Example, PipelineData, ShellError, Signature, Span, Type, Value};

#[derive(Clone)]
pub struct SubCommand;

impl Command for SubCommand {
    fn name(&self) -> &str {
        "math sin"
    }

    fn signature(&self) -> Signature {
        Signature::build("math sin")
            .switch("degrees", "Use degrees instead of radians", Some('d'))
            .input_output_types(vec![
                (Type::Number, Type::Float),
                (
                    Type::List(Box::new(Type::Number)),
                    Type::List(Box::new(Type::Float)),
                ),
            ])
            .category(Category::Math)
    }

    fn usage(&self) -> &str {
        "Returns the sine of the number."
    }

    fn search_terms(&self) -> Vec<&str> {
        vec!["trigonometry"]
    }

    fn run(
        &self,
        engine_state: &EngineState,
        _stack: &mut Stack,
        call: &Call,
        input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let head = call.head;
        let use_degrees = call.has_flag("degrees");
        // This doesn't match explicit nulls
        if matches!(input, PipelineData::Empty) {
            return Err(ShellError::PipelineEmpty { dst_span: head });
        }
        input.map(
            move |value| operate(value, head, use_degrees),
            engine_state.ctrlc.clone(),
        )
    }

    fn examples(&self) -> Vec<Example> {
        vec![
            Example {
                description: "Apply the sine to π/2",
                example: "3.141592 / 2 | math sin | math round --precision 4",
                result: Some(Value::test_float(1f64)),
            },
            Example {
                description: "Apply the sine to a list of angles in degrees",
                example: "[0 90 180 270 360] | math sin -d | math round --precision 4",
                result: Some(Value::list(
                    [
                        Value::test_float(0f64),
                        Value::test_float(1f64),
                        Value::test_float(0f64),
                        Value::test_float(-1f64),
                        Value::test_float(0f64),
                    ]
                    .into(),
                    Span::test_data(),
                )),
            },
        ]
    }
}

fn operate(value: Value, head: Span, use_degrees: bool) -> Value {
    match value {
        numeric @ (Value::Int { .. } | Value::Float { .. }) => {
            let span = numeric.span();
            let (val, span) = match numeric {
                Value::Int { val, .. } => (val as f64, span),
                Value::Float { val, .. } => (val, span),
                _ => unreachable!(),
            };

            let val = if use_degrees { val.to_radians() } else { val };

            Value::float(val.sin(), span)
        }
        Value::Error { .. } => value,
        other => Value::error(
            ShellError::OnlySupportsThisInputType {
                exp_input_type: "numeric".into(),
                wrong_type: other.get_type().to_string(),
                dst_span: head,
                src_span: other.span(),
            },
            head,
        ),
    }
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
