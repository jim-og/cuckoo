use super::*;

#[tokio::test]
async fn logger_contains_returns_true() {
    let logger = StdoutLogger::new().with_receiver();

    logger.debug("foo");
    logger.info("bar");
    logger.warn("baz");
    logger.error("qux");

    assert!(
        logger.contains("Debug").await,
        "Logger should contain 'Debug'"
    );
    assert!(
        logger.contains("Info").await,
        "Logger should contain 'Info'"
    );
    assert!(
        logger.contains("Warn").await,
        "Logger should contain 'Warn'"
    );
    assert!(
        logger.contains("Error").await,
        "Logger should contain 'Error'"
    );
}

#[tokio::test(start_paused = true)]
async fn contains_times_out_when_no_message_arrives() {
    let logger = StdoutLogger::new().with_receiver();
    assert!(!logger.contains("never").await);
}

#[tokio::test]
async fn contains_returns_false_when_sender_dropped() {
    let mut logger = StdoutLogger::new().with_receiver();
    // Drop the only outstanding sender so the receiver yields `Ok(None)`.
    logger.sender = None;
    assert!(!logger.contains("anything").await);
}

#[tokio::test]
async fn contains_returns_false_without_receiver() {
    let logger = StdoutLogger::new();
    assert!(!logger.contains("anything").await);
}
