use nu_engine::command_prelude::*;

#[derive(Clone)]
pub struct Return;

impl Command for Return {
    fn name(&self) -> &str {
        "return"
    }

    fn usage(&self) -> &str {
        "Return early from a function."
    }

    fn signature(&self) -> nu_protocol::Signature {
        Signature::build("return")
            .input_output_types(vec![(Type::Nothing, Type::Any)])
            .optional(
                "return_value",
                SyntaxShape::Any,
                "Optional value to return.",
            )
            .category(Category::Core)
    }

    fn extra_usage(&self) -> &str {
        r#"This command is a parser keyword. For details, check:
  https://www.nushell.sh/book/thinking_in_nu.html"#
    }

    fn is_parser_keyword(&self) -> bool {
        true
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> ShellResult<PipelineData> {
        let return_value: Option<Value> = call.opt(engine_state, stack, 0)?;
        if let Some(value) = return_value {
            Err(ShellError::Return {
                span: call.head,
                value: Box::new(value),
            })?
        } else {
            Err(ShellError::Return {
                span: call.head,
                value: Box::new(Value::nothing(call.head)),
            })?
        }
    }

    fn examples(&self) -> Vec<Example> {
        vec![Example {
            description: "Return early",
            example: r#"def foo [] { return }"#,
            result: None,
        }]
    }
}
