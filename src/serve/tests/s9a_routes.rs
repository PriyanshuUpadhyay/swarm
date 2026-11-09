//! G2 drives the actual HTTP router on TCP; only source selection is test-visible.
use super::*;
use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
};
#[path = "../../../tests/s9a/frozen.rs"]
mod frozen;

struct Server {
    fixture: test_support::Fixture,
    session: String,
    log: PathBuf,
    port: u16,
    token: String,
    stop: Option<oneshot::Sender<()>>,
    task: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn new() -> Self {
        let mut fixture = test_support::Fixture::new();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let app = Arc::get_mut(&mut fixture.app).unwrap();
        app.port = port;
        app.ui = false;
        app.cli = std::env::var_os("S9A_SWARM")
            .expect("built real swarm binary")
            .into();
        let app = fixture.app.clone();
        let (stop, stopped) = oneshot::channel();
        let task = std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                let listener = tokio::net::TcpListener::from_std(listener).unwrap();
                use std::future::IntoFuture;
                tokio::select! {
                    result = axum::serve(listener,runtime::router(app)).into_future() => result.unwrap(),
                    _ = stopped => {},
                }
            });
        });
        Self {
            session: fixture.session.clone(),
            log: fixture.log.clone(),
            port,
            token: fixture.app.token.clone(),
            fixture,
            stop: Some(stop),
            task: Some(task),
        }
    }
    fn connection(&self) -> TcpStream {
        let stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(30000)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_millis(30000)))
            .unwrap();
        stream
    }

    fn request(
        &self,
        method: &str,
        path: &str,
        headers: &str,
        body: &str,
        auth: bool,
    ) -> (u16, String, String) {
        let mut stream = self.connection();
        let token = if auth {
            format!("Authorization: Bearer {}\r\n", self.token)
        } else {
            String::new()
        };
        // One write: `write!` sends each fragment by itself, and a guard answers on the headers. A
        // body that lands after the server closes draws a reset, which can replace the response.
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n{token}{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.port,
            body.len()
        );
        stream.write_all(request.as_bytes()).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        let (headers, body) = response.split_once("\r\n\r\n").unwrap();
        let status = headers.split_whitespace().nth(1).unwrap().parse().unwrap();
        (status, headers.to_owned(), body.to_owned())
    }

    fn sse(&self) -> BufReader<TcpStream> {
        let mut stream = self.connection();
        write!(
            stream,
            "GET /api/stream HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\n\r\n",
            self.port, self.token
        )
        .unwrap();
        let mut reader = BufReader::new(stream);
        let mut first = String::new();
        reader.read_line(&mut first).unwrap();
        assert!(first.contains("200"), "{first}");
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
        }
        reader
    }

    fn append(&self, bytes: &str) {
        OpenOptions::new()
            .append(true)
            .open(&self.log)
            .unwrap()
            .write_all(bytes.as_bytes())
            .unwrap();
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.fixture.app.stopped.store(true, Ordering::Relaxed);
        let _ = self.stop.take().unwrap().send(());
        self.task.take().unwrap().join().unwrap();
    }
}
fn event(reader: &mut BufReader<TcpStream>) -> (String, Value) {
    let mut name = String::new();
    loop {
        let mut line = String::new();
        assert!(reader.read_line(&mut line).unwrap() > 0, "SSE ended");
        if let Some(value) = line.strip_prefix("event: ") {
            name = value.trim().into();
        }
        if let Some(value) = line.strip_prefix("data: ") {
            return (name, serde_json::from_str(value.trim()).unwrap());
        }
    }
}
fn named_event(reader: &mut BufReader<TcpStream>, wanted: &str) -> Value {
    loop {
        let (name, data) = event(reader);
        if name == wanted {
            return data;
        }
    }
}
include!("../../../tests/s9a/routes_all.rs");

// The 15 s heartbeat starts in runtime::start, not in the router this test serves, and OLD
// caught a ping only because its capture ran past 15 s. A ping is not transcript data.
fn drop_pings(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .filter(|item| item["event"] != "ping")
                .map(drop_pings)
                .collect(),
        ),
        Value::Object(map) => {
            Value::Object(map.into_iter().map(|(k, v)| (k, drop_pings(v))).collect())
        }
        other => other,
    }
}

#[test]
fn g2_real_routes_with_the_rust_caller_match_every_old_context() {
    all();
    assert_eq!(drop_pings(result()), drop_pings(frozen::expected(&[])));
}
