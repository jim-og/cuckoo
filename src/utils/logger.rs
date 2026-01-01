use chrono::Utc;

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

pub struct StdoutLogger;

impl Logger for StdoutLogger {
    fn log(&self, level: LogLevel, message: &str) {
        let timestamp = Utc::now().format("%Y-%m-%d %H:%M:%S%.3f");
        println!("[{}] {:?}: {}", timestamp, level, message);
    }
}
