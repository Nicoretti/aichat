use crate::config::GlobalConfig;
use reedline::{Highlighter, StyledText};

pub struct ReplHighlighter {
    config: GlobalConfig,
}

impl ReplHighlighter {
    pub fn new(config: &GlobalConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }
}

impl Highlighter for ReplHighlighter {
    fn highlight(&self, line: &str, _cursor: usize) -> StyledText {
        let mut styled_text = StyledText::new();
        styled_text
    }
}
