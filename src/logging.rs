use std::fs::File;
use std::io::Write;
use std::path::Path;

pub struct ConversationLogger {
    log_file: Option<File>,
}

impl ConversationLogger {
    pub fn new(log_path: &str) -> Result<Self, std::io::Error> {
        let log_file = File::create(log_path)?;
        Ok(Self {
            log_file: Some(log_file),
        })
    }

    pub fn log(&mut self, message: &str) -> Result<(), std::io::Error> {
        if let Some(ref mut file) = self.log_file {
            writeln!(file, "{}", message)?;
        }
        Ok(())
    }
}