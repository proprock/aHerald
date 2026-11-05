use aherald_ping::{DEADLINE, hook, parse_args, read_input, self_test, within};
use std::io::{Read, Write};

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

fn main() {
    let cli = match parse_args(std::env::args().skip(1)) {
        Ok(cli) => cli,
        Err(error) => {
            // A hook never fails its agent; the reason goes where only a person looks.
            let _ = std::io::copy(&mut std::io::stdin(), &mut std::io::sink());
            eprintln!("aherald-ping: {error}");
            return;
        }
    };
    if cli.self_test {
        match self_test(&cli, env) {
            Ok(text) => println!("{text}"),
            Err(error) => {
                eprintln!("aherald-ping: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    let input = read_input(std::io::stdin());
    // The agent may still be writing; drain so it never blocks on a full pipe.
    let _ = std::io::copy(&mut std::io::stdin().take(1 << 20), &mut std::io::sink());
    let reply = within(
        DEADLINE + std::time::Duration::from_millis(100),
        move || hook(&cli, &input, env),
    )
    .flatten();
    if let Some(reply) = reply {
        let _ = writeln!(std::io::stdout(), "{reply}");
    }
}
