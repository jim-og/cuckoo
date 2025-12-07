use anyhow::Context;
use cuckoo::MainProgram;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut main_program = MainProgram::new()
        .await
        .context("Failed to create main program")?;

    main_program
        .run()
        .await
        .context("Application failed to run")
}
