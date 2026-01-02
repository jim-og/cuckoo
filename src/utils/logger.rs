use chrono::Utc;
use tokio::sync::Mutex;
use tokio::{
    sync::mpsc::{self, Receiver, Sender},
    time::{Duration, timeout},
};

#[derive(Debug, Clone, Copy)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

pub trait Logger: Send + Sync {
    fn log(&self, level: LogLevel, message: &str);

    fn info(&self, msg: &str) {
        self.log(LogLevel::Info, msg);
    }
    fn debug(&self, msg: &str) {
        self.log(LogLevel::Debug, msg);
    }
    fn warn(&self, msg: &str) {
        self.log(LogLevel::Warn, msg);
    }
    fn error(&self, msg: &str) {
        self.log(LogLevel::Error, msg);
    }
}

pub struct StdoutLogger {
    sender: Option<Sender<String>>,
    receiver: Mutex<Option<Receiver<String>>>,
}

impl StdoutLogger {
    pub fn new() -> Self {
        Self {
            sender: None,
            receiver: Mutex::new(None),
        }
    }

    pub fn with_receiver(mut self) -> Self {
        let (sender, receiver) = mpsc::channel::<String>(1024);
        self.sender = Some(sender);
        self.receiver = Mutex::new(Some(receiver));
        self
    }

    pub async fn contains(&self, needle: &str) -> bool {
        let mut guard = self.receiver.lock().await;

        let Some(receiver) = guard.as_mut() else {
            return false;
        };

        loop {
            match timeout(Duration::from_secs(2), receiver.recv()).await {
                Ok(Some(msg)) => {
                    if msg.contains(needle) {
                        return true;
                    }
                }
                Ok(None) => {
                    // channel closed
                    eprintln!("logger channel receiver closed");
                    return false;
                }
                Err(_) => {
                    // timeout elapsed
                    eprintln!("logger channel timeout elapsed");
                    return false;
                }
            }
        }
    }
}

impl Default for StdoutLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl Logger for StdoutLogger {
    fn log(&self, level: LogLevel, message: &str) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let msg = format!("[{}] {:?}: {}", timestamp, level, message);

        println!("{}", msg);

        if let Some(sender) = &self.sender {
            let sender = sender.clone();
            let msg = msg.clone();
            tokio::spawn(async move {
                let _ = sender.send(msg).await;
            });
        }
    }
}

#[cfg(test)]
mod tests {
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

    // TODO timeout
    // TODO channel close
}
