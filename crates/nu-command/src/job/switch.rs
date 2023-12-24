use nu_engine::CallExt;
use nu_protocol::{
    ast::Call,
    engine::{Command, EngineState, Stack},
    Category, Example, PipelineData, ShellError, Signature, Spanned, SyntaxShape, Type,
};

#[derive(Clone)]
pub struct JobSwitch;

impl Command for JobSwitch {
    fn name(&self) -> &str {
        "job switch"
    }

    fn signature(&self) -> Signature {
        Signature::build("job switch")
            .input_output_types(vec![(Type::Nothing, Type::Nothing)])
            .required("job id", SyntaxShape::Int, "the id of the job to switch to")
            .category(Category::Job)
    }

    fn usage(&self) -> &str {
        "Bring a background job to the foreground."
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
        let id: Spanned<i64> = call.req(engine_state, stack, 0)?;
        if engine_state.jobs.switch_foreground(id.item as usize) {
            Ok(PipelineData::Empty)
        } else {
            Err(ShellError::NotFound { span: id.span })
        }
    }

    fn examples(&self) -> Vec<Example> {
        vec![Example {
            description: "Switch to a job",
            example: "job switch 1",
            result: None,
        }]
    }
}
