//! `mailimg` URI scheme (spec Part 03 §5.6): serves stored WebP images by integer id. No file path is ever read.
use std::future::Future;

use outlooker_core::dto::error::AppError;
use tauri::http::{header, Method, Response, StatusCode};

fn empty(status: StatusCode) -> Response<Vec<u8>> {
    let mut r = Response::new(Vec::new());
    *r.status_mut() = status;
    r
}

/// 503 for requests that arrive before the services exist.
pub fn unavailable() -> Response<Vec<u8>> {
    empty(StatusCode::SERVICE_UNAVAILABLE)
}

/// `path` is the URI path (`/<id>`): `mailimg://localhost/<id>` on macOS, `http://mailimg.localhost/<id>` on Windows.
fn parse_id(path: &str) -> Option<i64> {
    let s = path.strip_prefix('/').unwrap_or(path);
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<i64>().ok().filter(|id| *id > 0)
}

pub async fn respond<F, Fut>(method: &Method, path: &str, load: F) -> Response<Vec<u8>>
where
    F: FnOnce(i64) -> Fut,
    Fut: Future<Output = Result<Option<Vec<u8>>, AppError>>,
{
    if *method != Method::GET {
        return empty(StatusCode::METHOD_NOT_ALLOWED);
    }
    let Some(id) = parse_id(path) else {
        return empty(StatusCode::BAD_REQUEST);
    };
    match load(id).await {
        Ok(Some(bytes)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "image/webp")
            .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
            .body(bytes)
            .expect("static status and headers are valid"),
        Ok(None) => empty(StatusCode::NOT_FOUND),
        Err(AppError::DbNotReady) => empty(StatusCode::SERVICE_UNAVAILABLE),
        Err(e) => {
            log::error!("mailimg {id}: {e}");
            empty(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use tauri::http::{header, Method, StatusCode};

    const WEBP: &[u8] = b"RIFF\x0c\x00\x00\x00WEBPVP8 ";

    fn store() -> HashMap<i64, Vec<u8>> {
        HashMap::from([(7, WEBP.to_vec())])
    }

    async fn call(method: &Method, path: &str) -> Response<Vec<u8>> {
        let s = store();
        respond(method, path, |id| {
            let hit = s.get(&id).cloned();
            async move { Ok(hit) }
        })
        .await
    }

    #[tokio::test]
    async fn mailimg_post_is_405() {
        let r = call(&Method::POST, "/7").await;
        assert_eq!(r.status(), StatusCode::METHOD_NOT_ALLOWED);
        assert!(r.body().is_empty());
    }

    #[tokio::test]
    async fn mailimg_non_integer_is_400() {
        for path in ["/abc", "/0", "/-1", "/", "/7.5", "/+7", "/7/x", "/99999999999999999999"] {
            assert_eq!(call(&Method::GET, path).await.status(), StatusCode::BAD_REQUEST, "path {path}");
        }
    }

    #[tokio::test]
    async fn mailimg_unknown_id_is_404() {
        let r = call(&Method::GET, "/8").await;
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
        assert!(r.body().is_empty());
    }

    #[tokio::test]
    async fn mailimg_seeded_id_is_200_webp() {
        let r = call(&Method::GET, "/7").await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(r.headers()[header::CONTENT_TYPE], "image/webp");
        assert_eq!(r.headers()[header::CACHE_CONTROL], "max-age=31536000, immutable");
        assert_eq!(r.body().as_slice(), WEBP);
    }

    #[tokio::test]
    async fn mailimg_db_not_ready_is_503() {
        let r = respond(&Method::GET, "/7", |_| async { Err(AppError::DbNotReady) }).await;
        assert_eq!(r.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(unavailable().status(), StatusCode::SERVICE_UNAVAILABLE);
        let r = respond(&Method::GET, "/7", |_| async { Err(AppError::internal("boom")) }).await;
        assert_eq!(r.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(r.body().is_empty());
    }
}
