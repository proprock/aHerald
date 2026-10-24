use std::path::PathBuf;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cli {
    pub operation: Option<String>,
    pub mode: Option<String>,
    pub event: Option<String>,
    pub config: Option<PathBuf>,
}

impl Cli {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut cli = Self::default();
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--hook" | "--status" | "--quit" | "--help" => {
                    if cli.operation.replace(arg[2..].to_owned()).is_some() {
                        return Err("conflicting explicit operations".into());
                    }
                }
                "--resident" | "--transient" => {
                    if cli.mode.replace(arg[2..].to_owned()).is_some() {
                        return Err("conflicting explicit modes".into());
                    }
                }
                "--event" => {
                    if cli.operation.replace("event".into()).is_some() {
                        return Err("conflicting explicit operations".into());
                    }
                    let event = args.next().ok_or("--event requires a value")?;
                    if !crate::config::EVENT_NAMES.contains(&event.as_str()) {
                        return Err("unknown event".into());
                    }
                    cli.event = Some(event);
                }
                "--config" => {
                    if cli.config.is_some() {
                        return Err("duplicate --config".into());
                    }
                    cli.config = Some(args.next().ok_or("--config requires a path")?.into());
                }
                _ => return Err("unknown argument".into()),
            }
        }
        if matches!(cli.operation.as_deref(), Some("status" | "quit" | "help"))
            && cli.mode.is_some()
        {
            return Err("operation does not accept a mode".into());
        }
        Ok(cli)
    }

    pub fn selected_mode<'a>(&'a self, config: &'a crate::config::Config) -> &'a str {
        self.mode.as_deref().unwrap_or(&config.general.mode)
    }
}
