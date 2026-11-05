//! The command sink: runs a program for each message, for services that have a command line
//! rather than an HTTP endpoint (mail through `msmtp` or `mutt`, `signal-cli`, `notify-send`
//! over `ssh`, `say`).
//!
//! No shell is involved. The configuration holds the program and its arguments as an array, so a
//! value that contains spaces, quotes, `;` or `$(...)` stays one argument and nothing parses it.
//! `${NAME}` is replaced once when the sink is built, as in a webhook; `{text}`, `{kind}`,
//! `{harness}` and `{folder}` are replaced at each send, as written, in the arguments and in the
//! optional standard input. The program inherits the resident's environment, so it can read its
//! own secrets from there rather than from its arguments, which other local users can list.
use crate::{
    http_common::Zero,
    https::Wiped,
    sink::{Message, Sink},
    template::{Escape, render},
};
use std::{
    io::{ErrorKind, Write},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// A command that has not ended by then is killed, so it cannot hold the sink's worker.
const TIMEOUT: Duration = Duration::from_secs(10);

/// The argument and standard-input templates, wiped when dropped because they may hold secrets.
struct Template {
    argv: Vec<String>,
    stdin: Option<String>,
}
impl Zero for Template {
    fn zero(&mut self) {
        for argument in &mut self.argv {
            argument.zero();
        }
        if let Some(stdin) = &mut self.stdin {
            stdin.zero();
        }
    }
}

/// What one send runs, wiped when dropped.
struct Rendered(Vec<String>);
impl Zero for Rendered {
    fn zero(&mut self) {
        for argument in &mut self.0 {
            argument.zero();
        }
    }
}

pub struct CommandSink {
    template: Wiped<Template>,
    timeout: Duration,
}

impl CommandSink {
    /// `argv` and `stdin` have their `${NAME}` placeholders already expanded.
    pub fn new(argv: Vec<String>, stdin: Option<String>) -> Result<Self, String> {
        if argv.first().is_none_or(|program| program.trim().is_empty()) {
            return Err("the command must start with the program to run".to_owned());
        }
        Ok(Self {
            template: Wiped(Template { argv, stdin }),
            timeout: TIMEOUT,
        })
    }

    #[cfg(all(test, unix))]
    fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

impl Sink for CommandSink {
    fn name(&self) -> &str {
        "command"
    }

    fn send(&mut self, message: &Message) -> Result<(), String> {
        let template = &self.template.0;
        let argv = Wiped(Rendered(
            template
                .argv
                .iter()
                .map(|argument| render(argument, message, Escape::Raw))
                .collect(),
        ));
        let stdin = template
            .stdin
            .as_deref()
            .map(|text| Wiped(render(text, message, Escape::Raw)));
        let (program, arguments) = argv.0.0.split_first().ok_or("the command is empty")?;
        let mut command = Command::new(program);
        command
            .args(arguments)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            // What the program prints may hold anything; the card says only how it ended.
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn().map_err(|error| match error.kind() {
            ErrorKind::NotFound => "the program was not found".to_owned(),
            ErrorKind::PermissionDenied => {
                "the program cannot be run (permission denied)".to_owned()
            }
            _ => "the program cannot be started".to_owned(),
        })?;
        if let (Some(mut pipe), Some(text)) = (child.stdin.take(), &stdin) {
            // A program that ends before reading closes the pipe; its status tells the rest.
            let _ = pipe.write_all(text.0.as_bytes());
        }
        let deadline = Instant::now() + self.timeout;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => {
                    return Err(match status.code() {
                        Some(code) => format!("the command ended with exit status {code}"),
                        None => "the command was stopped by a signal".to_owned(),
                    });
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "the command did not finish within {} s",
                        self.timeout.as_secs()
                    ));
                }
                Err(_) => return Err("cannot wait for the command".to_owned()),
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn message(folder: Option<&str>) -> Message {
        Message {
            kind: "input_required".into(),
            harness: "Claude".into(),
            folder: folder.map(str::to_owned),
            text: "Claude: input required".into(),
        }
    }

    fn sink(argv: &[&str], stdin: Option<&str>) -> CommandSink {
        CommandSink::new(
            argv.iter().map(|a| (*a).to_owned()).collect(),
            stdin.map(str::to_owned),
        )
        .unwrap()
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("aherald-command-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_arguments_and_the_standard_input_are_filled_for_each_message() {
        let dir = scratch("fill");
        let out = dir.join("out");
        // `sh -c 'script' name args...`: the script is fixed, only the arguments carry values.
        let mut sink = sink(
            &[
                "sh",
                "-c",
                "printf '%s|%s|' \"$1\" \"$2\" > \"$3\"; cat >> \"$3\"",
                "sh",
                "{kind}",
                "{text}",
                out.to_str().unwrap(),
            ],
            Some("body: {harness}"),
        );
        sink.send(&message(None)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&out).unwrap(),
            "input_required|Claude: input required|body: Claude"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_value_with_shell_syntax_stays_one_argument_and_nothing_runs() {
        let dir = scratch("inject");
        let marker = dir.join("marker");
        let out = dir.join("out");
        let hostile = format!(
            "a; touch {} #$(touch {}) `touch {}` \"'",
            marker.display(),
            marker.display(),
            marker.display()
        );
        let mut sink = sink(
            &[
                "sh",
                "-c",
                "printf '%s' \"$1\" > \"$2\"",
                "sh",
                "x{folder}",
                out.to_str().unwrap(),
            ],
            None,
        );
        sink.send(&message(Some(&hostile))).unwrap();
        assert_eq!(
            std::fs::read_to_string(&out).unwrap(),
            format!("x{hostile}")
        );
        assert!(!marker.exists(), "a shell parsed the value");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failures_say_how_the_command_ended_and_never_what_it_printed_or_its_arguments() {
        let mut missing = sink(&["/nonexistent/aherald-program", "TOP-SECRET"], None);
        assert_eq!(
            missing.send(&message(None)).unwrap_err(),
            "the program was not found"
        );
        let mut failing = sink(&["sh", "-c", "echo TOP-SECRET >&2; exit 7", "sh"], None);
        let error = failing.send(&message(None)).unwrap_err();
        assert_eq!(error, "the command ended with exit status 7");
        assert!(!error.contains("SECRET"));
    }

    #[test]
    fn a_command_that_does_not_end_is_killed_at_the_time_limit() {
        let mut slow =
            sink(&["sh", "-c", "sleep 30", "sh"], None).with_timeout(Duration::from_millis(300));
        let started = Instant::now();
        let error = slow.send(&message(None)).unwrap_err();
        assert!(
            error.starts_with("the command did not finish within"),
            "{error}"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn an_empty_program_is_refused() {
        assert!(CommandSink::new(Vec::new(), None).is_err());
        assert!(CommandSink::new(vec!["  ".into()], None).is_err());
    }
}
