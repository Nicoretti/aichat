mod commands;
mod completer;
mod highlighter;
mod parse;
mod prompt;
mod validator;

use self::completer::ReplCompleter;
use self::highlighter::ReplHighlighter;
use self::prompt::ReplPrompt;
use self::validator::ReplValidator;

use indoc::formatdoc;
use std::collections::HashMap;

use crate::config::GlobalConfig;
use crate::utils::{create_abort_signal, AbortSignal};

use anyhow::Result;
use fancy_regex::Regex;
use lazy_static::lazy_static;
use reedline::{
    default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
    ColumnarMenu, EditCommand, EditMode, Emacs, KeyCode, KeyModifiers, Keybindings, Reedline,
    ReedlineEvent, ReedlineMenu, Vi,
};
use reedline::{MenuBuilder, Signal};
use std::{env, process};

const MENU_NAME: &str = "completion_menu";

lazy_static! {
    static ref COMMAND_RE: Regex = Regex::new(r"^\s*(\.\S*)\s*").unwrap();
    static ref MULTILINE_RE: Regex = Regex::new(r"(?s)^\s*:::\s*(.*)\s*:::\s*$").unwrap();
}

pub(crate) struct Repl {
    config: GlobalConfig,
    editor: Reedline,
    prompt: ReplPrompt,
    abort: AbortSignal,
    commands: HashMap<String, Box<dyn commands::Command<commands::Context>>>,
}

impl Repl {
    pub fn init(config: &GlobalConfig) -> Result<Self> {
        config.write().in_repl = true;
        Ok(ReplBuilder::new(config)
            .add_command(Box::new(commands::Config {}))
            .add_command(Box::new(commands::Model {}))
            .add_command(Box::new(commands::Role {}))
            .add_command(Box::new(commands::Session {}))
            .add_command(Box::new(commands::Copy {}))
            .add_command(Box::new(commands::Exit {}))
            .into())
    }

    fn display_banner(&self) {
        fn banner() -> String {
            let version = env!("CARGO_PKG_VERSION");
            formatdoc! { r###"
                Welcome to aichat {version}
                Type ".help" for more information.
           "###
            }
        }
        print!("{}", banner());
    }

    pub fn run(&mut self) -> Result<()> {
        self.display_banner();
        loop {
            let signal : Signal = self.editor.read_line(&self.prompt)?;
            match signal {
                Signal::Success(line) => self.handle(&line)?,
                Signal::CtrlC => self.handle(".exit session")?,
                Signal::CtrlD => std::process::exit(-1),
            }
        }
    }

    fn handle(&self, mut line: &str) -> Result<()> {
        println!("Processed: {}", line);
        Ok(())
    }

    fn help(&self) -> String {
        let commands = self
            .commands
            .values()
            .map(|cmd| format!("{:<24} {}", cmd.name(), cmd.name()))
            .chain(std::iter::once(format!("{:<24} {}", ".help", "This help")))
            .collect::<Vec<String>>()
            .join("\n");
        formatdoc! { r###"
            Available Commands:
            -------------------
            {commands}

            Detailed help:
            --------------
            To get detailed help on a specific command type `.help <command>`.
            E.g: .help .config

            Multiline Editing:
            ------------------
            Type ::: to begin multi-line editing, type ::: to end it.
            Press Ctrl+O to open an editor to modify the current prompt.
            Press Ctrl+C to abort aichat, Ctrl+D to exit the REPL"##,
           "###
        }
    }
}

struct ReplBuilder {
    abort: AbortSignal,
    config: GlobalConfig,
    editor: Reedline,
    prompt: ReplPrompt,
    commands: HashMap<String, Box<dyn commands::Command<commands::Context>>>,
}

impl ReplBuilder {
    pub fn new(config: &GlobalConfig) -> Self {
        Self {
            config: config.clone(),
            editor: Self::create_editor(config),
            prompt: ReplPrompt::new(config),
            abort: create_abort_signal(),
            commands: HashMap::new(),
        }
    }

    pub fn add_command(mut self, command: Box<dyn commands::Command<commands::Context>>) -> Self {
        self.commands.insert(command.name().to_string(), command);
        self
    }

    fn create_editor(config: &GlobalConfig) -> Reedline {
        fn create_menu() -> ReedlineMenu {
            let completion_menu = ColumnarMenu::default().with_name(MENU_NAME);
            ReedlineMenu::EngineCompleter(Box::new(completion_menu))
        }
        fn create_edit_mode(config: &GlobalConfig) -> Box<dyn EditMode> {
            fn extra_keybindings(keybindings: &mut Keybindings) {
                keybindings.add_binding(
                    KeyModifiers::NONE,
                    KeyCode::Tab,
                    ReedlineEvent::UntilFound(vec![
                        ReedlineEvent::Menu(MENU_NAME.to_string()),
                        ReedlineEvent::MenuNext,
                    ]),
                );
                keybindings.add_binding(
                    KeyModifiers::SHIFT,
                    KeyCode::BackTab,
                    ReedlineEvent::MenuPrevious,
                );
                keybindings.add_binding(
                    KeyModifiers::CONTROL,
                    KeyCode::Enter,
                    ReedlineEvent::Edit(vec![EditCommand::InsertNewline]),
                );
            }
            let edit_mode: Box<dyn EditMode> = if config.read().keybindings.is_vi() {
                let mut normal_keybindings = default_vi_normal_keybindings();
                let mut insert_keybindings = default_vi_insert_keybindings();
                extra_keybindings(&mut normal_keybindings);
                extra_keybindings(&mut insert_keybindings);
                Box::new(Vi::new(insert_keybindings, normal_keybindings))
            } else {
                let mut keybindings = default_emacs_keybindings();
                extra_keybindings(&mut keybindings);
                Box::new(Emacs::new(keybindings))
            };
            edit_mode
        }
        let completer = ReplCompleter::new(config);
        let highlighter = ReplHighlighter::new(config);
        let menu = create_menu();
        let edit_mode = create_edit_mode(config);
        let mut editor = Reedline::create()
            .with_completer(Box::new(completer))
            .with_highlighter(Box::new(highlighter))
            .with_menu(menu)
            //.with_edit_mode(edit_mode)
            .with_quick_completions(true)
            .with_partial_completions(true)
            .use_bracketed_paste(true)
            .with_validator(Box::new(ReplValidator))
            .with_ansi_colors(true);

        if let Ok(cmd) = env::var("VISUAL").or_else(|_| env::var("EDITOR")) {
            let temp_file =
                env::temp_dir().join(format!("aichat-{}.txt", chrono::Utc::now().timestamp()));
            let command = process::Command::new(cmd);
            editor = editor.with_buffer_editor(command, temp_file);
        }
        editor
    }
}

impl Into<Repl> for ReplBuilder {
    fn into(self) -> Repl {
        Repl {
            abort: self.abort,
            config: self.config,
            editor: self.editor,
            prompt: self.prompt,
            commands: self.commands,
        }
    }
}
