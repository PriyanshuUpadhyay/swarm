use super::*;
use std::{fs, os::unix::fs::symlink, time::SystemTime};

const CSP: &str =
    "sandbox allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox allow-downloads";

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("swarm-pages-{}", uuid::Uuid::now_v7()));
        fs::create_dir(&root).unwrap();
        Self(root)
    }

    fn page(&self, folder: &str, html: &str) -> PathBuf {
        let path = self.0.join(folder);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("index.html"), html).unwrap();
        path
    }

    async fn request(&self, method: &str, path: &str, host: Option<&str>) -> Response {
        let mut request = Request::builder().method(method).uri(path);
        if let Some(host) = host {
            request = request.header(header::HOST, host);
        }
        shelf(
            State(Shelf {
                root: self.0.clone(),
                port: 4748,
            }),
            request.body(Body::empty()).unwrap(),
        )
        .await
    }

    async fn get(&self, path: &str) -> Response {
        self.request("GET", path, Some("localhost:4748")).await
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn headers(response: &Response) {
    for (key, expected) in [
        ("content-security-policy", CSP),
        ("x-content-type-options", "nosniff"),
        ("cache-control", "no-cache"),
        ("referrer-policy", "no-referrer"),
    ] {
        assert_eq!(response.headers().get(key).unwrap(), expected, "{key}");
    }
    assert!(
        !response
            .headers()
            .keys()
            .any(|key| key.as_str().starts_with("access-control-"))
    );
}

async fn body(response: Response) -> String {
    String::from_utf8(
        axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}

#[tokio::test]
async fn host_method_and_headers() {
    let fixture = Fixture::new();
    fixture.page("2026-10-21-page", "<title>Page</title>");
    for (method, path, host, expected) in [
        ("GET", "/", Some("foreign.test:4748"), StatusCode::FORBIDDEN),
        ("GET", "/", Some("localhost:4747"), StatusCode::FORBIDDEN),
        ("GET", "/", None, StatusCode::FORBIDDEN),
        (
            "POST",
            "/",
            Some("localhost:4748"),
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        (
            "OPTIONS",
            "/",
            Some("localhost:4748"),
            StatusCode::METHOD_NOT_ALLOWED,
        ),
        (
            "GET",
            "/missing",
            Some("localhost:4748"),
            StatusCode::NOT_FOUND,
        ),
        ("GET", "/", Some("127.0.0.1:4748"), StatusCode::OK),
        (
            "GET",
            "/2026-10-21-page",
            Some("localhost:4748"),
            StatusCode::TEMPORARY_REDIRECT,
        ),
    ] {
        let response = fixture.request(method, path, host).await;
        assert_eq!(response.status(), expected, "{method} {path} {host:?}");
        headers(&response);
        if expected == StatusCode::TEMPORARY_REDIRECT {
            assert_eq!(response.headers()[header::LOCATION], "/2026-10-21-page/");
        }
    }
}

#[tokio::test]
async fn paths_refuse_traversal_links_and_dot_entries() {
    let fixture = Fixture::new();
    let outside = Fixture::new();
    outside.page("secret", "secret");
    fixture.page("page", "page");
    fixture.page("no-index", "remove");
    fs::remove_file(fixture.0.join("no-index/index.html")).unwrap();
    fs::write(fixture.0.join(".env"), "secret").unwrap();
    fs::write(fixture.0.join("page/.hidden"), "secret").unwrap();
    symlink(
        outside.0.join("secret/index.html"),
        fixture.0.join("page/link.html"),
    )
    .unwrap();
    symlink(outside.0.join("secret"), fixture.0.join("linked")).unwrap();
    symlink(
        fixture.0.join("page/index.html"),
        fixture.0.join("page/internal.html"),
    )
    .unwrap();
    for path in [
        "/../x",
        "/%2e%2e/x",
        "/%252e%252e/x",
        "/page/../index.html",
        "/page/./index.html",
        "/.env",
        "/page/.hidden",
        "/page/%2ehidden",
        "/page/link.html",
        "/page/internal.html",
        "/linked/",
        "/no-index/",
        "/bad%zz",
        "//page",
        "/%2fpage",
    ] {
        let response = fixture.get(path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
        headers(&response);
    }
}

#[tokio::test]
async fn files_head_and_percent_decoding() {
    let fixture = Fixture::new();
    fixture.page("page", "<title>Page</title>");
    fs::write(fixture.0.join("page/site.css"), "body {}").unwrap();
    fs::write(fixture.0.join("page/a b.txt"), "space").unwrap();
    let response = fixture.get("/page/").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/html; charset=utf-8"
    );
    assert_eq!(body(response).await, "<title>Page</title>");
    let response = fixture
        .request("HEAD", "/page/site.css", Some("localhost:4748"))
        .await;
    assert_eq!(response.status(), StatusCode::OK);
    headers(&response);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "text/css; charset=utf-8"
    );
    assert_eq!(response.headers()[header::CONTENT_LENGTH], "7");
    assert!(body(response).await.is_empty());
    assert_eq!(body(fixture.get("/page/a%20b.txt").await).await, "space");
    assert!(
        body(fixture.request("HEAD", "/", Some("localhost:4748")).await)
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn index_newest_first_escapes_titles_and_names() {
    let fixture = Fixture::new();
    let old = fixture.page("2026-10-20-old", "<title>Old</title>");
    let new = fixture.page(
        "2026-10-21-new",
        "<TITLE><img src=x onerror='bad'> & \"new\"</TITLE>",
    );
    fixture.page(".tmp-unpublished", "<title>Hidden</title>");
    fixture.page("2026-10-21-a&b", "No title");
    let set_time = |path: &Path, seconds| {
        fs::File::open(path)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)),
            )
            .unwrap();
    };
    set_time(&old, 10);
    set_time(&new, 20);
    let response = fixture.get("/").await;
    assert_eq!(response.status(), StatusCode::OK);
    headers(&response);
    let html = body(response).await;
    assert!(html.find("2026-10-21-new").unwrap() < html.find("2026-10-20-old").unwrap());
    assert!(html.contains("&lt;img src=x onerror=&#39;bad&#39;&gt; &amp; &quot;new&quot;"));
    assert!(html.contains("2026-10-21-a&amp;b"));
    assert!(!html.contains("<img"));
    assert!(!html.contains("Hidden"));
    assert!(html.contains("1970") || html.contains("20"));
}

#[test]
fn publish_rejects_invalid_input_and_links() {
    let fixture = Fixture::new();
    let source = fixture.page("source", "page");
    let shelf = fixture.0.join("shelf");
    for slug in ["", "../x", "UPPER", "a_b", "a/b"] {
        assert!(
            publish(&shelf, &source, slug, "2026-10-21")
                .unwrap_err()
                .contains("slug")
        );
    }
    for date in ["../", "2026/10/21", "2026-10-21/../escape"] {
        assert!(publish(&shelf, &source, "page", date).is_err());
    }
    fs::remove_file(source.join("index.html")).unwrap();
    assert!(
        publish(&shelf, &source, "page", "2026-10-21")
            .unwrap_err()
            .contains("index.html")
    );
    fs::write(source.join("index.html"), "page").unwrap();
    for name in ["link", ".hidden-link"] {
        let link = source.join(name);
        symlink("index.html", &link).unwrap();
        assert_eq!(
            publish(&shelf, &source, "page", "2026-10-21").unwrap_err(),
            format!(
                "swarm pages: {} is a symlink; copy the file",
                link.display()
            )
        );
        fs::remove_file(link).unwrap();
    }
}

#[test]
fn publish_copies_regular_files_skips_dots_and_replaces() {
    let fixture = Fixture::new();
    let source = fixture.page("source", "first");
    let shelf = fixture.0.join("shelf");
    fs::create_dir(source.join("assets")).unwrap();
    fs::write(source.join("assets/a.css"), "style").unwrap();
    fs::write(source.join(".env"), "secret").unwrap();
    fs::create_dir(source.join(".git")).unwrap();
    fs::write(source.join(".git/config"), "secret").unwrap();
    let first = publish(&shelf, &source, "page", "2026-10-21").unwrap();
    assert_eq!(first.folder, "2026-10-21-page");
    assert_eq!(first.hash.len(), 64);
    let target = shelf.join(&first.folder);
    assert_eq!(
        fs::read_to_string(target.join("assets/a.css")).unwrap(),
        "style"
    );
    assert!(!target.join(".env").exists());
    assert!(!target.join(".git").exists());
    assert_eq!(
        publish(&shelf, &source, "page", "2026-10-21").unwrap().hash,
        first.hash
    );
    fs::remove_file(source.join("assets/a.css")).unwrap();
    fs::write(source.join("index.html"), "second").unwrap();
    let second = publish(&shelf, &source, "page", "2026-10-21").unwrap();
    assert_ne!(first.hash, second.hash);
    assert_eq!(
        fs::read_to_string(target.join("index.html")).unwrap(),
        "second"
    );
    assert!(!target.join("assets/a.css").exists());
    assert_eq!(fs::read_dir(&shelf).unwrap().count(), 1);
    fs::remove_dir_all(source).unwrap();
    assert_eq!(
        fs::read_to_string(target.join("index.html")).unwrap(),
        "second"
    );
}

#[test]
fn publish_hash_uses_sorted_paths_and_bytes() {
    let fixture = Fixture::new();
    let first = fixture.page("first", "page");
    let second = fixture.page("second", "page");
    fs::write(first.join("z.txt"), "z").unwrap();
    fs::write(first.join("a.txt"), "a").unwrap();
    fs::write(second.join("a.txt"), "a").unwrap();
    fs::write(second.join("z.txt"), "z").unwrap();
    let shelf = fixture.0.join("shelf");
    let hash = publish(&shelf, &first, "first", "2026-10-21").unwrap().hash;
    assert_eq!(
        publish(&shelf, &second, "second", "2026-10-21")
            .unwrap()
            .hash,
        hash
    );
    fs::rename(second.join("a.txt"), second.join("b.txt")).unwrap();
    assert_ne!(
        publish(&shelf, &second, "second", "2026-10-21")
            .unwrap()
            .hash,
        hash
    );
}
