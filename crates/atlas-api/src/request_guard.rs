//! Keeps web pages on other sites away from this unauthenticated local API.
//!
//! Binding to loopback is not enough, because the user's own browser is on this
//! machine and will send requests on behalf of any page it has open:
//!
//! - DNS rebinding: a page resolves its own domain to 127.0.0.1 and calls the API
//!   as same-origin, which CORS does not stop. The request still carries that
//!   domain in `Host`, so the `Host` check rejects it.
//! - Cross-site requests: a page posts a plain form to `http://localhost:4317`.
//!   CORS hides the response but the request still runs, and `Host` looks fine.
//!   The browser marks it with the page's `Origin`, so the `Origin` check rejects it.
//!
//! - Cross-site GETs: an `<img>` on any page sends no `Origin`, yet can still make
//!   this server build snapshots. Browsers mark it with `Sec-Fetch-Site`.
//!
//! Requests with neither header, such as curl, are allowed. Following a link to the
//! dashboard from another site is a top-level navigation, which runs no script of
//! that site, so it is allowed too.

use atlas_model::ErrorCode;
use axum::extract::Request;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;

use crate::{ApiError, DEV_ORIGINS};

pub async fn require_local_request(request: Request, next: Next) -> Result<Response, ApiError> {
    check_headers(request.headers())?;
    Ok(next.run(request).await)
}

fn check_headers(headers: &HeaderMap) -> Result<(), ApiError> {
    let text = |name| {
        headers
            .get(name)
            .map(|value: &HeaderValue| value.to_str().unwrap_or_default())
    };

    let host = text(header::HOST).unwrap_or_default();
    if !is_local_host(host) {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            ErrorCode::ForbiddenHost,
            "this server only answers requests addressed to localhost",
        ));
    }
    let cross_site = text(HeaderName::from_static("sec-fetch-site")) == Some("cross-site")
        && text(HeaderName::from_static("sec-fetch-mode")) != Some("navigate");
    let untrusted = text(header::ORIGIN).is_some_and(|origin| !is_trusted_origin(origin, host));
    if cross_site || untrusted {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            ErrorCode::ForbiddenOrigin,
            "this server does not accept requests from other sites",
        ));
    }
    Ok(())
}

fn is_local_host(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        Some(bracketed) => bracketed.split_once(']').map(|(name, _)| name),
        None => host.split(':').next(),
    };
    name.is_some_and(|name| {
        name.eq_ignore_ascii_case("localhost") || matches!(name, "127.0.0.1" | "::1")
    })
}

/// True for the Vite dev server and for pages this server itself delivered. `host`
/// has already passed `is_local_host`.
fn is_trusted_origin(origin: &str, host: &str) -> bool {
    DEV_ORIGINS.contains(&origin)
        || origin
            .strip_prefix("http://")
            .is_some_and(|authority| authority.eq_ignore_ascii_case(host))
}

#[cfg(test)]
mod tests {
    use atlas_model::ErrorCode;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode, header};

    use super::{is_local_host, is_trusted_origin};
    use crate::test_support::{app, error_body, request, send};

    #[test]
    fn recognizes_loopback_names_with_and_without_ports() {
        for host in [
            "localhost",
            "LOCALHOST:5173",
            "127.0.0.1",
            "127.0.0.1:4317",
            "[::1]",
            "[::1]:4317",
        ] {
            assert!(is_local_host(host), "{host}");
        }
    }

    #[test]
    fn rejects_lookalikes() {
        for host in [
            "evil.example",
            "localhost.evil.example",
            "127.0.0.1.evil.example",
            "evil.example:4317",
            "[::1",
            "",
        ] {
            assert!(!is_local_host(host), "{host}");
        }
    }

    #[test]
    fn trusts_the_dev_server_and_the_same_origin_only() {
        let host = "127.0.0.1:4317";
        assert!(is_trusted_origin("http://localhost:5173", host));
        assert!(is_trusted_origin("http://127.0.0.1:4317", host));
        for origin in [
            "https://evil.example",
            "http://127.0.0.1:9999",
            "https://127.0.0.1:4317",
            "null",
            "",
        ] {
            assert!(!is_trusted_origin(origin, host), "{origin}");
        }
    }

    #[tokio::test]
    async fn rebinding_requests_are_forbidden() {
        let (app, _) = app();
        let request = Request::builder()
            .uri("/api/v1/health")
            .header(header::HOST, "evil.example:4317")
            .body(Body::empty())
            .unwrap();
        let response = send(app, request).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            error_body(response).await.error.code,
            ErrorCode::ForbiddenHost
        );
    }

    #[tokio::test]
    async fn requests_without_a_host_are_forbidden() {
        let (app, _) = app();
        let request = Request::builder()
            .uri("/api/v1/health")
            .body(Body::empty())
            .unwrap();
        assert_eq!(send(app, request).await.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn cross_site_posts_are_forbidden_even_with_a_local_host() {
        let (app, _) = app();
        let request = request("/api/v1/health")
            .method(Method::POST)
            .header(header::ORIGIN, "https://evil.example")
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::empty())
            .unwrap();
        let response = send(app, request).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            error_body(response).await.error.code,
            ErrorCode::ForbiddenOrigin
        );
    }

    #[tokio::test]
    async fn non_browser_clients_without_an_origin_are_allowed() {
        let (app, _) = app();
        let response = send(app, request("/api/v1/health").body(Body::empty()).unwrap()).await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn a_cross_site_get_with_no_origin_is_rejected() {
        let cross_site = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/health")
            .header(header::HOST, "127.0.0.1:4317")
            .header("sec-fetch-site", "cross-site")
            .header("sec-fetch-mode", "no-cors")
            .body(Body::empty())
            .unwrap();
        let response = send(app().0, cross_site).await;
        assert_eq!(response.status(), StatusCode::FORBIDDEN);

        let navigation = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/health")
            .header(header::HOST, "127.0.0.1:4317")
            .header("sec-fetch-site", "cross-site")
            .header("sec-fetch-mode", "navigate")
            .body(Body::empty())
            .unwrap();
        assert_ne!(
            send(app().0, navigation).await.status(),
            StatusCode::FORBIDDEN
        );
    }
}
