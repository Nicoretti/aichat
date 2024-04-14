use crate::config::GlobalConfig;
use reedline::{Completer, Suggestion};

pub struct ReplCompleter {
    config: GlobalConfig,
}

impl Completer for ReplCompleter {
    fn complete(&mut self, line: &str, pos: usize) -> Vec<Suggestion> {
        let mut suggestions = vec![];
        suggestions
    }
}

impl ReplCompleter {
    pub fn new(config: &GlobalConfig) -> Self {
        Self { config: config.clone() }
    }
}
