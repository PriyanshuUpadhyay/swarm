//! CLI and process lifecycle for the loopback page shelf.

use super::*;
use std::{fs, net::Ipv4Addr, process::Command as Process, time::Instant};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const PORT: u16 = 4748;
const WAIT: Duration = Duration::from_secs(5);
const USAGE: &str = "usage: swarm pages [publish <folder> [--slug <slug>] | stop | --foreground [--port N] [--id <id>]...]";

#[derive(Debug, PartialEq)]
enum Command {
    List,
    Stop,
    Foreground(u16, Vec<String>),
    Publish(PathBuf, Option<String>),
}

pub fn pages(args: &[String]) -> Result<()> {
    let command = parse(args)?;
    let root = swarm::paths::root_dir()?;
    let record = root.join("pages-url");
    match command {
        Command::List => {
            let running =
                read_record(&record).filter(|(url, pid)| answers(url, *pid, Instant::now() + WAIT));
            println!(
                "{}",
                running.as_ref().map_or("no shelf runs", |(url, _)| url)
            );
            let shelf = root.join("pages");
            pages_hub::prepare_shelf(&shelf)?;
            for page in pages_hub::list(&shelf)? {
                println!("{}", list_line(&page));
            }
        }
        Command::Stop => stop(&record)?,
        Command::Foreground(port, ids) => {
            if let Some((url, _)) =
                read_record(&record).filter(|(url, pid)| answers(url, *pid, Instant::now() + WAIT))
            {
                return Err(format!(
                    "swarm pages: a shelf already runs at {url}; run `swarm pages stop` first"
                )
                .into());
            }
            foreground(&root, &record, port, ids)?;
        }
        Command::Publish(folder, slug) => {
            let absolute = std::env::current_dir()?.join(folder);
            let source = fs::canonicalize(&absolute)?;
            let slug = slug.unwrap_or_else(|| default_slug(&source));
            // Keep the original path so publish can refuse a symlink at the source itself.
            let published = pages_hub::publish(&root.join("pages"), &absolute, &slug, &today()?)?;
            let deadline = Instant::now() + WAIT;
            let url = match read_record(&record).filter(|(url, pid)| answers(url, *pid, deadline)) {
                Some((url, _)) => url,
                None => spawn(&root, &record, deadline)?,
            };
            print!(
                "{}",
                publish_lines(
                    &url,
                    &published.id,
                    &published.hash,
                    &source,
                    source_commit(&source).as_deref()
                )
            );
        }
    }
    Ok(())
}

fn parse(args: &[String]) -> Result<Command> {
    match args {
        [] => Ok(Command::List),
        [verb] if verb == "stop" => Ok(Command::Stop),
        [flag, rest @ ..] if flag == "--foreground" => {
            let mut port = PORT;
            let mut ids = Vec::new();
            let mut rest = rest.iter();
            while let Some(option) = rest.next() {
                let value = rest.next().ok_or(USAGE)?;
                match option.as_str() {
                    "--port" => {
                        port = value
                            .parse::<u16>()
                            .ok()
                            .filter(|port| *port != 0)
                            .ok_or(USAGE)?;
                    }
                    "--id" if pages_hub::page_id(value) => ids.push(value.clone()),
                    "--id" => return Err(format!("swarm pages: {value} is not a page ID").into()),
                    _ => return Err(USAGE.into()),
                }
            }
            Ok(Command::Foreground(port, ids))
        }
        [verb, folder] if verb == "publish" => Ok(Command::Publish(folder.into(), None)),
        [verb, folder, flag, slug] if verb == "publish" && flag == "--slug" => {
            Ok(Command::Publish(folder.into(), Some(slug.clone())))
        }
        _ => Err(USAGE.into()),
    }
}

fn list_line(page: &pages_hub::Page) -> String {
    let name = match (page.date.as_str(), page.slug.as_str()) {
        ("", slug) => slug.to_owned(),
        (date, slug) => format!("{date}-{slug}"),
    };
    let mut line = format!("{}  {name}", page.id);
    if !page.source.is_empty() {
        line.push_str("  ");
        line.push_str(&page.source);
    }
    line
}

fn default_slug(source: &Path) -> String {
    source
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_owned()
}

fn url_port(url: &str) -> Option<u16> {
    url.strip_prefix("http://127.0.0.1:")?
        .strip_suffix('/')?
        .parse()
        .ok()
        .filter(|port| *port != 0)
}

fn read_record(record: &Path) -> Option<(String, i32)> {
    let text = fs::read_to_string(record).ok()?;
    let mut lines = text.lines();
    let url = lines.next()?;
    url_port(url)?;
    let pid = lines.next()?.parse::<i32>().ok().filter(|pid| *pid > 1)?;
    Some((url.to_owned(), pid))
}

fn write_record(record: &Path, url: &str, pid: u32) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    match fs::remove_file(record) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(record)?
        .write_all(format!("{url}\n{pid}\n").as_bytes())?;
    Ok(())
}

fn publish_lines(url: &str, id: &str, hash: &str, source: &Path, commit: Option<&str>) -> String {
    let mut text = format!(
        "{url}{id}/\nhash sha256:{hash}\nsource {}\n",
        source.display()
    );
    if let Some(commit) = commit {
        text.push_str(commit);
        text.push('\n');
    }
    text
}

fn commit_line(head: &str, status: &str) -> Option<String> {
    let head = head.trim();
    if head.is_empty() || !head.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(format!(
        "commit {head}{}",
        if status.is_empty() { "" } else { "+dirty" }
    ))
}

fn source_commit(source: &Path) -> Option<String> {
    let git = |args: &[&str]| {
        let output = Process::new("git")
            .arg("--no-optional-locks")
            .arg("-C")
            .arg(source)
            .args(args)
            .output()
            .ok()?;
        output.status.success().then_some(output.stdout)
    };
    let head = String::from_utf8(git(&["rev-parse", "--short", "HEAD"])?).ok()?;
    let status = String::from_utf8(git(&["status", "--porcelain"])?).ok()?;
    // Bare repositories have a HEAD but no work tree, so status must succeed too.
    commit_line(&head, &status)
}

fn today() -> Result<String> {
    let output = Process::new("date").arg("+%Y-%m-%d").output()?;
    if !output.status.success() {
        return Err("swarm pages: could not read today's local date".into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn foreground(root: &Path, record: &Path, port: u16, ids: Vec<String>) -> Result<()> {
    let shelf = root.join("pages");
    pages_hub::prepare_shelf(&shelf)?;
    let pages = pages_hub::list(&shelf)?;
    if let Some(missing) = ids
        .iter()
        .find(|id| !pages.iter().any(|page| &page.id == *id))
    {
        return Err(format!("swarm pages: no page has ID {missing}").into());
    }
    let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).map_err(
        |error| -> Box<dyn std::error::Error> {
            if error.kind() == std::io::ErrorKind::AddrInUse {
                format!("swarm pages: port {port} is in use by another program; pass --port N")
                    .into()
            } else {
                error.into()
            }
        },
    )?;
    listener.set_nonblocking(true)?;
    let url = format!("http://127.0.0.1:{port}/");
    let runtime = tokio::runtime::Runtime::new()?;
    write_record(record, &url, std::process::id())?;
    println!("{url}");
    std::io::stdout().flush()?;
    runtime.block_on(async {
        axum::serve(
            tokio::net::TcpListener::from_std(listener)?,
            pages_hub::router(shelf, port, (!ids.is_empty()).then_some(ids)),
        )
        .await
    })?;
    Ok(())
}

/// Every socket timeout uses the remaining probe budget, which is within the caller's deadline.
fn answers(url: &str, pid: i32, deadline: Instant) -> bool {
    let deadline = deadline.min(Instant::now() + Duration::from_millis(500));
    let expected_pid = pid.to_string();
    let probe = || -> Option<()> {
        let port = url_port(url)?;
        let address = (Ipv4Addr::LOCALHOST, port).into();
        let remaining = || {
            deadline
                .checked_duration_since(Instant::now())
                .filter(|left| !left.is_zero())
        };
        let mut stream = std::net::TcpStream::connect_timeout(&address, remaining()?).ok()?;
        let request = format!(
            "GET /.swarm-pages-probe HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        );
        let mut bytes = request.as_bytes();
        while !bytes.is_empty() {
            stream.set_write_timeout(Some(remaining()?)).ok()?;
            let written = stream.write(bytes).ok()?;
            if written == 0 {
                return None;
            }
            bytes = &bytes[written..];
        }
        let mut headers = Vec::new();
        while headers.len() < 8192 {
            stream.set_read_timeout(Some(remaining()?)).ok()?;
            let mut byte = [0];
            stream.read_exact(&mut byte).ok()?;
            headers.push(byte[0]);
            if headers.ends_with(b"\r\n\r\n") {
                let headers = std::str::from_utf8(&headers).ok()?;
                let mut lines = headers.split("\r\n");
                let status = lines.next()?;
                if !status.starts_with("HTTP/1.1 ") && !status.starts_with("HTTP/1.0 ") {
                    return None;
                }
                return lines
                    .filter_map(|line| line.split_once(':'))
                    .any(|(name, value)| {
                        name.eq_ignore_ascii_case("x-swarm-pages-pid")
                            && value.trim() == expected_pid
                    })
                    .then_some(());
            }
        }
        None
    };
    probe().is_some()
}

fn spawn(root: &Path, record: &Path, deadline: Instant) -> Result<String> {
    use std::os::unix::process::CommandExt;
    let log_path = root.join("pages.log");
    let log = owner_log(&log_path)?;
    let _ = fs::remove_file(record);
    let mut child = Process::new(std::env::current_exe()?)
        .args(["pages", "--foreground"])
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .process_group(0)
        .spawn()?;
    while Instant::now() < deadline {
        if child.try_wait()?.is_some() {
            let output = fs::read_to_string(&log_path).unwrap_or_default();
            return Err(format!(
                "{} (log: {})",
                output
                    .lines()
                    .last()
                    .unwrap_or("swarm pages: the shelf exited"),
                log_path.display()
            )
            .into());
        }
        if let Some((url, _)) =
            read_record(record).filter(|(url, pid)| answers(url, *pid, deadline))
        {
            return Ok(url);
        }
        pause(deadline);
    }
    Err(format!(
        "swarm pages: the shelf did not answer within 5 s; see {}",
        log_path.display()
    )
    .into())
}

fn pause(deadline: Instant) {
    std::thread::sleep(
        deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(100)),
    );
}

fn stop(record: &Path) -> Result<()> {
    let deadline = Instant::now() + WAIT;
    let Some((url, pid)) = read_record(record).filter(|(url, pid)| answers(url, *pid, deadline))
    else {
        let _ = fs::remove_file(record);
        println!("no shelf runs");
        return Ok(());
    };
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    // SAFETY: the record has a positive pid and its URL just echoed that pid in a header (ADR 0088).
    if unsafe { kill(pid, 15) } != 0 {
        return Err(format!(
            "swarm pages: could not stop pid {pid}: {}",
            std::io::Error::last_os_error()
        )
        .into());
    }
    let port = url_port(&url).unwrap();
    loop {
        if std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok() {
            let _ = fs::remove_file(record);
            println!("stopped");
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("swarm pages: port {port} is still in use after 5 s").into());
        }
        pause(deadline);
    }
}

#[cfg(test)]
mod tests;
