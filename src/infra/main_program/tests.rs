use super::*;

impl MainProgram {
    pub fn new_with_logger(logger: Arc<dyn Logger>) -> Self {
        Self { logger }
    }
}

#[tokio::test]
async fn new_creates_main_program() -> Result<()> {
    let result = MainProgram::new().await;

    assert!(result.is_ok());

    Ok(())
}

// TODO valid config folder
// TODO invalid config folder
// TODO missing config file
// TODO invalid config file

#[tokio::test]
async fn run_sets_up_signal_handling() -> Result<()> {
    let logger = Arc::new(StdoutLogger::new().with_receiver());
    let mut main_program = MainProgram::new_with_logger(logger.clone());
    let join_handle = tokio::task::spawn(async move { main_program.run().await });

    assert!(logger.contains("CTRL-C").await);

    join_handle.abort();

    Ok(())
}

#[tokio::test]
async fn run_logs_startup_banner() -> Result<()> {
    let logger = Arc::new(StdoutLogger::new().with_receiver());
    let mut main_program = MainProgram::new_with_logger(logger.clone());
    let join_handle = tokio::task::spawn(async move { main_program.run().await });

    assert!(logger.contains("cuckoo - version").await);

    join_handle.abort();

    Ok(())
}
