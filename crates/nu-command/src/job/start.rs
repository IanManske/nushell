use std::path::Path;

use nu_engine::{env_to_strings, CallExt};
use nu_protocol::{
    ast::Call,
    engine::{Command, EngineState, Stack},
    Category, Example, PipelineData, ShellError, Signature, Spanned, SyntaxShape, Type, Value,
};

#[derive(Clone)]
pub struct JobStart;

impl Command for JobStart {
    fn name(&self) -> &str {
        "job start"
    }

    fn signature(&self) -> Signature {
        Signature::build("job start")
            .input_output_types(vec![(Type::Nothing, Type::Nothing)])
            .required(
                "command",
                SyntaxShape::String,
                "the external command to run",
            )
            .rest(
                "args",
                SyntaxShape::Any,
                "the arguments for the external command",
            )
            .category(Category::Job)
    }

    fn usage(&self) -> &str {
        "Start a background job."
    }

    fn extra_usage(&self) -> &str {
        ""
    }

    fn run(
        &self,
        engine_state: &EngineState,
        stack: &mut Stack,
        call: &Call,
        _input: PipelineData,
    ) -> Result<PipelineData, ShellError> {
        let cmd: Spanned<String> = call.req(engine_state, stack, 0)?;
        let args: Vec<Value> = call.rest(engine_state, stack, 1)?;

        let args = args
            .iter()
            .map(Value::as_string)
            .collect::<Result<Vec<_>, _>>()?;

        // Translate environment variables from Values to Strings
        let envs = env_to_strings(engine_state, stack)?;

        let mut command = std::process::Command::new(cmd.item);
        if let Some(dir) = envs.get("PWD") {
            // do not try to set current directory if cwd does not exist
            if Path::new(&dir).exists() {
                command.current_dir(dir);
            }
        }
        command.args(args).envs(envs);

        if let Err(e) = engine_state
            .jobs
            .spawn_background(command, engine_state.is_interactive)
        {
            Err(ShellError::ExternalCommand {
                label: format!("{e}"),
                help: String::new(),
                span: cmd.span,
            })
        } else {
            Ok(PipelineData::Empty)
        }
    }

    fn examples(&self) -> Vec<Example> {
        vec![]
    }
}
