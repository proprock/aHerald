//! Builds the configured sinks, resolving each `${NAME}` secret from the environment, and turns a
//! sink that cannot be built into a notice, never into a stopped resident.
//!
//! A notice names the variable, never a value, so a misconfigured token cannot leak through a card
//! or a log.
use crate::{
    command_sink::CommandSink,
    config::{Forward, SinkConfig, SinkType},
    events::Event,
    sink::{Dispatcher, Policy, Route, Sink},
    webhook::{Webhook, expand_env},
};
use std::sync::Mutex;

pub struct Built {
    pub routes: Vec<Route>,
    /// One line for each sink left out, to show as a card at startup.
    pub notices: Vec<String>,
}

/// Builds every sink in `forward`. `env` looks a variable up.
pub fn build(forward: &Forward, env: impl Fn(&str) -> Option<String>) -> Built {
    let mut built = Built {
        routes: Vec::new(),
        notices: Vec::new(),
    };
    for (index, sink) in forward.sinks.iter().enumerate() {
        match route(sink, &env) {
            Ok(route) => built.routes.push(route),
            Err(why) => built
                .notices
                .push(format!("Messenger forwarding, sink {}: {why}", index + 1)),
        }
    }
    built
}

fn route(sink: &SinkConfig, env: &impl Fn(&str) -> Option<String>) -> Result<Route, String> {
    let (sink_impl, secrets): (Box<dyn Sink>, Vec<String>) = match sink.kind {
        SinkType::Webhook => webhook(sink, env)?,
        SinkType::Command => command(sink, env)?,
    };
    Ok(Route {
        sink: sink_impl,
        kinds: sink.kinds.clone(),
        secrets,
        include_folder: sink.include_folder,
    })
}

fn webhook(
    sink: &SinkConfig,
    env: &impl Fn(&str) -> Option<String>,
) -> Result<(Box<dyn Sink>, Vec<String>), String> {
    let (url, mut secrets) = expand_env(sink.url.as_deref().unwrap_or_default(), env)?;
    let (body, more) = expand_env(sink.body.as_deref().unwrap_or("{text}"), env)?;
    secrets.extend(more);
    let content_type = sink
        .content_type
        .clone()
        .unwrap_or_else(|| "text/plain; charset=utf-8".to_owned());
    Ok((Box::new(Webhook::new(&url, body, content_type)?), secrets))
}

fn command(
    sink: &SinkConfig,
    env: &impl Fn(&str) -> Option<String>,
) -> Result<(Box<dyn Sink>, Vec<String>), String> {
    let mut secrets = Vec::new();
    let mut argv = Vec::new();
    for argument in sink.command.as_deref().unwrap_or_default() {
        let (argument, more) = expand_env(argument, env)?;
        secrets.extend(more);
        argv.push(argument);
    }
    let stdin = match &sink.stdin {
        Some(template) => {
            let (text, more) = expand_env(template, env)?;
            secrets.extend(more);
            Some(text)
        }
        None => None,
    };
    Ok((Box::new(CommandSink::new(argv, stdin)?), secrets))
}

/// The resident's running sinks. One process has one resident, so this is process state, like the
/// notice sink.
static RUNNING: Mutex<Option<Dispatcher>> = Mutex::new(None);

fn running() -> std::sync::MutexGuard<'static, Option<Dispatcher>> {
    RUNNING.lock().unwrap_or_else(|e| e.into_inner())
}

/// Builds the configured sinks from the real environment and starts them. A sink that cannot be
/// built is a card; with no sink built nothing runs and the tray shows no messenger switch.
pub fn start(forward: &Forward) {
    let built = build(forward, |name| std::env::var(name).ok());
    for notice in &built.notices {
        crate::notice::post(notice);
    }
    stop();
    if !built.routes.is_empty() {
        install(Dispatcher::start(
            built.routes,
            Policy::default(),
            forward.enabled,
            |text| crate::notice::post(&format!("Messenger forwarding, {text}")),
        ));
    }
}

fn install(dispatcher: Dispatcher) {
    *running() = Some(dispatcher);
}

/// Stops forwarding; queued messages are abandoned.
pub fn stop() {
    *running() = None;
}

/// Sends an accepted card to the sinks that want it, if any run.
pub fn submit(event: &Event) {
    if let Some(dispatcher) = running().as_ref() {
        dispatcher.submit_event(event);
    }
}

/// The state of the messenger switch, or `None` when no sink runs.
pub fn enabled() -> Option<bool> {
    running().as_ref().map(Dispatcher::enabled)
}

/// Flips the messenger switch and returns the new state; `None` when no sink runs.
pub fn toggle() -> Option<bool> {
    let guard = running();
    let dispatcher = guard.as_ref()?;
    dispatcher.set_enabled(!dispatcher.enabled());
    Some(dispatcher.enabled())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::{collections::HashMap, path::PathBuf};

    fn forward(text: &str) -> Forward {
        Config::parse(text, &PathBuf::from(".")).unwrap().forward
    }

    const HOOK: &str = "[[forward.sinks]]\ntype = \"webhook\"\nurl = \"https://h.example/bot${TOKEN}/send\"\nbody = '{\"chat_id\": \"${CHAT}\", \"text\": \"{text}\"}'\n";

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| map.get(name).cloned()
    }

    #[test]
    fn a_sink_with_its_variables_set_is_built_and_every_value_is_a_secret_to_scrub() {
        let built = build(
            &forward(HOOK),
            env(&[("TOKEN", " 12:abc\n"), ("CHAT", "42")]),
        );
        assert!(built.notices.is_empty(), "{:?}", built.notices);
        assert_eq!(built.routes.len(), 1);
        assert_eq!(built.routes[0].secrets, ["12:abc", "42"]);
        assert_eq!(built.routes[0].sink.name(), "webhook");
        assert!(!built.routes[0].include_folder);
    }

    #[test]
    fn a_missing_or_empty_variable_is_a_notice_naming_the_variable_not_a_value() {
        for pairs in [
            vec![("CHAT", "42")],
            vec![("TOKEN", "  "), ("CHAT", "42")],
            vec![("TOKEN", "12:secret-value")],
        ] {
            let built = build(&forward(HOOK), env(&pairs));
            assert!(built.routes.is_empty());
            assert_eq!(built.notices.len(), 1);
            let notice = &built.notices[0];
            assert!(
                notice.contains("TOKEN") || notice.contains("CHAT"),
                "{notice}"
            );
            assert!(notice.contains("user environment"), "{notice}");
            assert!(!notice.contains("secret-value"), "{notice}");
        }
    }

    const COMMAND: &str = "[[forward.sinks]]\ntype = \"command\"\ncommand = [\"mail\", \"--to\", \"${TO}\"]\nstdin = \"${PASS}: {text}\"\n";

    #[test]
    fn a_command_sink_expands_its_variables_in_the_arguments_and_stdin_and_scrubs_them() {
        let built = build(
            &forward(COMMAND),
            env(&[("TO", "me@example.com"), ("PASS", "hunter2")]),
        );
        assert!(built.notices.is_empty(), "{:?}", built.notices);
        assert_eq!(built.routes[0].sink.name(), "command");
        assert_eq!(built.routes[0].secrets, ["me@example.com", "hunter2"]);
        let missing = build(&forward(COMMAND), env(&[("PASS", "hunter2")]));
        assert!(missing.routes.is_empty());
        assert!(missing.notices[0].contains("TO"), "{:?}", missing.notices);
        assert!(!missing.notices[0].contains("hunter2"));
    }

    #[test]
    fn one_broken_sink_does_not_stop_the_next() {
        let text = format!("{HOOK}{}", HOOK.replace("TOKEN", "OTHER"));
        let built = build(&forward(&text), env(&[("OTHER", "9:ok"), ("CHAT", "42")]));
        assert_eq!((built.routes.len(), built.notices.len()), (1, 1));
        assert!(built.notices[0].contains("sink 1"), "{:?}", built.notices);
    }

    #[test]
    fn the_switch_exists_only_while_sinks_run_and_flips_both_ways() {
        stop();
        assert_eq!((enabled(), toggle()), (None, None));
        install(Dispatcher::start(
            Vec::new(),
            Policy::default(),
            true,
            |_| {},
        ));
        assert_eq!(enabled(), Some(true));
        assert_eq!(toggle(), Some(false));
        assert_eq!(enabled(), Some(false));
        assert_eq!(toggle(), Some(true));
        stop();
        assert_eq!(enabled(), None);
    }
}
