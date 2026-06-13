use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq)]
pub enum ConsoleLevel {
    Log,
    Info,
    Warn,
    Error,
    Debug,
}

impl ConsoleLevel {
    pub fn name(&self) -> &'static str {
        match self {
            ConsoleLevel::Log => "log",
            ConsoleLevel::Info => "info",
            ConsoleLevel::Warn => "warn",
            ConsoleLevel::Error => "error",
            ConsoleLevel::Debug => "debug",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConsoleMessage {
    pub timestamp: u128,
    pub level: ConsoleLevel,
    pub text: String,
}

impl ConsoleMessage {
    pub fn new(level: ConsoleLevel, text: impl Into<String>) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        Self {
            timestamp,
            level,
            text: text.into(),
        }
    }
}

/// Captures console messages from JS execution.
#[derive(Debug, Clone)]
pub struct ConsoleCapture {
    messages: Vec<ConsoleMessage>,
    max_messages: usize,
}

impl ConsoleCapture {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            max_messages: 1000,
        }
    }

    pub fn with_max_messages(max: usize) -> Self {
        Self {
            messages: Vec::new(),
            max_messages: max,
        }
    }

    pub fn push(&mut self, level: ConsoleLevel, text: impl Into<String>) {
        if self.messages.len() >= self.max_messages {
            self.messages.remove(0);
        }
        self.messages.push(ConsoleMessage::new(level, text));
    }

    pub fn messages(&self) -> &[ConsoleMessage] {
        &self.messages
    }

    pub fn clear(&mut self) {
        self.messages.clear();
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}

impl Default for ConsoleCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_empty() {
        let cap = ConsoleCapture::new();
        assert!(cap.is_empty());
    }

    #[test]
    fn push_adds_message() {
        let mut cap = ConsoleCapture::new();
        cap.push(ConsoleLevel::Log, "hello");
        assert_eq!(cap.len(), 1);
        assert_eq!(cap.messages()[0].text, "hello");
        assert_eq!(cap.messages()[0].level, ConsoleLevel::Log);
    }

    #[test]
    fn clear_removes_all() {
        let mut cap = ConsoleCapture::new();
        cap.push(ConsoleLevel::Warn, "warning");
        cap.clear();
        assert!(cap.is_empty());
    }

    #[test]
    fn respects_max_messages() {
        let mut cap = ConsoleCapture::with_max_messages(3);
        cap.push(ConsoleLevel::Log, "a");
        cap.push(ConsoleLevel::Log, "b");
        cap.push(ConsoleLevel::Log, "c");
        cap.push(ConsoleLevel::Log, "d");
        assert_eq!(cap.len(), 3);
        assert_eq!(cap.messages()[0].text, "b");
        assert_eq!(cap.messages()[2].text, "d");
    }

    #[test]
    fn console_level_names() {
        assert_eq!(ConsoleLevel::Log.name(), "log");
        assert_eq!(ConsoleLevel::Warn.name(), "warn");
        assert_eq!(ConsoleLevel::Error.name(), "error");
    }
}
