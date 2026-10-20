//! Tests for server images.

use super::*;
use crate::serve::test_support::*;

#[tokio::test]
async fn a_pasted_image_is_saved_private_and_only_an_image_type_is_taken() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    let typed = |kind: &str| {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(header::CONTENT_TYPE, kind.parse().unwrap());
        headers
    };
    let saved = chat_image(
        State(fixture.app.clone()),
        typed("image/png"),
        axum::body::Bytes::from_static(b"\x89PNG"),
    )
    .await
    .unwrap()
    .0;
    let path = Path::new(saved["path"].as_str().unwrap());
    assert!(path.starts_with(fixture.app.root.join("uploads")));
    assert_eq!(path.extension().unwrap(), "png");
    assert_eq!(std::fs::read(path).unwrap(), b"\x89PNG");
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let refused = chat_image(
        State(fixture.app.clone()),
        typed("text/html"),
        axum::body::Bytes::from_static(b"<script>"),
    )
    .await
    .unwrap_err();
    assert_eq!(refused.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    let empty = chat_image(
        State(fixture.app.clone()),
        typed("image/png"),
        axum::body::Bytes::new(),
    )
    .await
    .unwrap_err();
    assert_eq!(empty.0, StatusCode::BAD_REQUEST);

    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let read = |name: &str| {
        chat_image_read(
            State(fixture.app.clone()),
            Query(ImageName { name: name.into() }),
        )
    };
    let shown = read(&name).await.unwrap();
    assert_eq!(shown.headers()[header::CONTENT_TYPE], "image/png");
    // Any other name, such as a path out of uploads, is refused before a read.
    for name in ["../swarm.db", "choices.json", "x.png", "../uploads/a.png"] {
        assert_eq!(
            read(name).await.unwrap_err().0,
            StatusCode::BAD_REQUEST,
            "{name}"
        );
    }
    let gone = format!("{}.png", uuid::Uuid::now_v7());
    assert_eq!(read(&gone).await.unwrap_err().0, StatusCode::NOT_FOUND);
}
