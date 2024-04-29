use anyhow::{bail, Result};
use indoc::indoc;

use crate::config::GlobalConfig;

pub struct Context {
    config: GlobalConfig,
}

pub trait Command<Context> {
    fn name(&self) -> &str;
    fn usage(&self) -> &str;
    fn description(&self) -> &str;
    fn help(&self) -> &str;
    fn execute(&mut self, args: &[&str], ctx: &mut Context) -> Result<String>;
}

pub struct Config;

impl Config {
    fn show(&mut self, _: &[&str], ctx: &mut Context) -> Result<String> {
        Ok(ctx.config.read().system_info()?)
    }

    fn set(&mut self, args: &[&str], ctx: &mut Context) -> Result<String> {
        if args.len() != 2 {
            bail!("Set requires key and value, usage: .set <key> <value>.");
        }
        ctx.config
            .write()
            .update(&args.join(" "))
            .map(|v| String::from(""))
    }
}

impl Command<Context> for Config {
    fn name(&self) -> &str {
        ".config"
    }

    fn usage(&self) -> &str {
        ".config subcommand"
    }

    fn description(&self) -> &str {
        "Show or set configuration options"
    }

    fn help(&self) -> &str {
        indoc! {"
            Usage: .config [subcommand]

            Command to manage the application configuration.

            Subcommands:
              show        Displays the current settings.
              set         Updates a setting with a specified key and value.
              help        outputs this help.

            Examples:
              .config show
              .config set SETTING VALUE
        "}
    }

    fn execute(&mut self, args: &[&str], ctx: &mut Context) -> Result<String> {
        if args.is_empty() {
            bail!("No subcommand specified. Use 'set' or 'show'.")
        }
        match args[0] {
            "set" => self.set(&args[1..], ctx),
            "show" => self.show(&args[1..], ctx),
            _ => bail!("Unsupported subcommand '{}'. Use 'set' or 'show'.", args[0]),
        }
    }
}

pub struct Model;

impl Command<Context> for Model {
    fn name(&self) -> &str {
        ".model"
    }
    fn usage(&self) -> &str {
        "Usage: .model <name>"
    }

    fn description(&self) -> &str {
        todo!()
    }

    fn help(&self) -> &str {
        "Help: Manipulate and query the model."
    }
    fn execute(&mut self, _args: &[&str], _ctx: &mut Context) -> Result<String> {
        Ok("Model command executed".to_string())
    }
}

pub struct Role;

impl Command<Context> for Role {
    fn name(&self) -> &str {
        ".role"
    }
    fn usage(&self) -> &str {
        "Usage: .role [args]"
    }

    fn description(&self) -> &str {
        todo!()
    }

    fn help(&self) -> &str {
        "Help: Define or modify roles."
    }
    fn execute(&mut self, _args: &[&str], _ctx: &mut Context) -> Result<String> {
        Ok("Role command executed".to_string())
    }
}

pub struct Session;

impl Command<Context> for Session {
    fn name(&self) -> &str {
        ".session"
    }
    fn usage(&self) -> &str {
        "Usage: .session [args]"
    }

    fn description(&self) -> &str {
        todo!()
    }

    fn help(&self) -> &str {
        "Help: Manage user sessions."
    }
    fn execute(&mut self, _args: &[&str], _ctx: &mut Context) -> Result<String> {
        Ok("Session command executed".to_string())
    }
}

pub struct Copy;

impl Command<Context> for Copy {
    fn name(&self) -> &str {
        ".copy"
    }
    fn usage(&self) -> &str {
        "Usage: .copy [args]"
    }

    fn description(&self) -> &str {
        todo!()
    }

    fn help(&self) -> &str {
        "Help: Copy data from one place to another."
    }
    fn execute(&mut self, _args: &[&str], _ctx: &mut Context) -> Result<String> {
        Ok("Copy command executed".to_string())
    }
}

pub struct Exit;

impl Command<Context> for Exit {
    fn name(&self) -> &str {
        ".exit"
    }
    fn usage(&self) -> &str {
        "Usage: .exit"
    }

    fn description(&self) -> &str {
        todo!()
    }

    fn help(&self) -> &str {
        "Help: Exit the application."
    }
    fn execute(&mut self, _args: &[&str], _ctx: &mut Context) -> Result<String> {
        std::process::exit(0);
    }
}
