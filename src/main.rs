use std::env;
use std::os::unix::process::ExitStatusExt;

#[cfg(feature = "serve")]
mod serve;

mod cli;
use cli::*;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if trust_maintenance(&args) {
        return;
    }
    if let [cmd, rest @ ..] = args.as_slice()
        && cmd == "guard"
    {
        // A panic exits 101, which Claude and Codex read as "let the call through"; exit 2 blocks
        // on every CLI.
        std::panic::set_hook(Box::new(|info| {
            eprintln!("swarm guard: {info}, so the call is blocked");
            std::process::exit(2);
        }));
        let (stdout, stderr, code) = guard(rest);
        print!("{stdout}");
        eprint!("{stderr}");
        std::process::exit(code);
    }
    if let [cmd, dashes, command @ ..] = args.as_slice()
        && cmd == "pty"
        && dashes == "--"
    {
        std::process::exit(pty(command, false));
    }
    if let [cmd, flag, dashes, command @ ..] = args.as_slice()
        && cmd == "pty"
        && flag == "--host"
        && dashes == "--"
    {
        std::process::exit(pty(command, true));
    }
    if let [cmd, verb] = args.as_slice()
        && cmd == "pty"
        && verb == "host"
    {
        std::process::exit(pty_host_headless());
    }
    if let [cmd, verb] = args.as_slice()
        && cmd == "pty"
        && verb != "--"
    {
        match pty_client(verb) {
            Ok(out) => print!("{out}"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let [cmd] = args.as_slice()
        && cmd == "status-feed"
    {
        if let Err(error) = status_feed() {
            eprintln!("swarm status-feed: {error}");
        }
        return;
    }
    if let [cmd, rest @ ..] = args.as_slice()
        && cmd == "hook"
    {
        let output = match hook(rest) {
            Ok(Some(output)) => output,
            Ok(None) => serde_json::json!({}),
            Err(error) => {
                eprintln!("{error}");
                serde_json::json!({})
            }
        };
        println!("{output}");
        return;
    }
    if let [cmd, agent_id] = args.as_slice()
        && cmd == "attach"
    {
        match attach(agent_id) {
            Ok(status) => std::process::exit(
                status
                    .code()
                    .unwrap_or_else(|| 128 + status.signal().unwrap_or(0)),
            ),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
    if let Err(error) = run(&args) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
