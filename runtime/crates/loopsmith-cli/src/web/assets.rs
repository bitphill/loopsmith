//! The compiled frontend, served out of the binary.
//!
//! Three files, compiled in with `include_str!`/`include_bytes!` and served
//! from memory. Nothing is read from disk at run time, which is the whole
//! point: someone who ran `brew install loopsmith` has no checkout, and a UI
//! that only works next to its own source is a UI most users never see.
//!
//! Vite is configured to emit these three fixed names rather than the usual
//! content-hashed ones. Hashed filenames and `include_str!` do not mix — the
//! literal path would have to change on every build.

use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

const INDEX_HTML: &str = include_str!("dist/index.html");
const APP_JS: &str = include_str!("dist/app.js");
const APP_CSS: &str = include_str!("dist/app.css");

/// The mark, and the favicon, and the tour's illustration — one file for all
/// three. It lives in `templates/` beside the examples for the same reason they
/// do: `assets/` sits above the package root, so an `include_bytes!` pointed at
/// it compiles in this checkout and fails for everyone installing from a
/// registry. `tools/sync-logo.sh` keeps the copy honest.
///
/// The copy has an alpha channel the one in `assets/` does not. The published
/// logo is RGB on a flat near-white field, which is right for a README on
/// GitHub and wrong for a dark UI, where it renders as a white tile.
const MARK_PNG: &[u8] = include_bytes!("../../templates/loopsmith-mark.png");

pub fn router() -> Router {
    Router::new()
        .route("/", get(index))
        .route("/app.js", get(js))
        .route("/app.css", get(css))
        .route("/logo.png", get(mark))
        // Browsers ask for this before any script runs, so it is answered
        // directly rather than left to the SPA fallback — which would hand back
        // an HTML document with an image content type.
        .route("/favicon.png", get(mark))
        // Anything else is the single-page app's own route. Serving the shell
        // means a reload on a deep link works instead of 404ing.
        .fallback(get(index))
}

async fn index() -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            // The bundle changes with every build and is served from a binary
            // that may be replaced under a running browser. Caching the shell
            // is how someone ends up staring at last week's UI.
            (header::CACHE_CONTROL, "no-store"),
        ],
        INDEX_HTML,
    )
        .into_response()
}

async fn js() -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/javascript; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        APP_JS,
    )
        .into_response()
}

async fn css() -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/css; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        APP_CSS,
    )
        .into_response()
}

async fn mark() -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "image/png"),
            // Unlike the bundle, the mark does not change between builds.
            (header::CACHE_CONTROL, "public, max-age=86400"),
        ],
        MARK_PNG,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frontend_was_actually_built_before_this_binary_was() {
        // A placeholder dist compiles perfectly well and serves an empty page,
        // which fails silently in the browser and nowhere else. These bounds
        // are low enough not to be brittle and high enough that the
        // placeholder cannot pass.
        assert!(
            INDEX_HTML.contains("<div id=\"root\""),
            "index.html is not the built shell"
        );
        assert!(
            APP_JS.len() > 20_000,
            "app.js is {} bytes — the frontend was not built. \
             Run `npm --prefix runtime/crates/loopsmith-cli/web run build`.",
            APP_JS.len()
        );
        assert!(
            APP_CSS.len() > 2_000,
            "app.css is {} bytes — the stylesheet was not built",
            APP_CSS.len()
        );
    }

    #[test]
    fn the_shell_loads_the_two_assets_this_module_serves() {
        assert!(INDEX_HTML.contains("/app.js"), "shell must load the bundle");
        assert!(INDEX_HTML.contains("/app.css"), "shell must load the styles");
    }

    #[test]
    fn the_mark_is_a_png_with_an_alpha_channel() {
        assert_eq!(&MARK_PNG[..8], b"\x89PNG\r\n\x1a\n", "not a PNG");
        // IHDR colour type lives at byte 25. 6 is RGBA; 2 is RGB, which is the
        // published logo — flat near-white field and all — and renders as a
        // white tile on the dark theme.
        assert_eq!(MARK_PNG[25], 6, "the mark must carry alpha, not a white field");
        assert!(MARK_PNG.len() > 4_000, "suspiciously small for a 256px mark");
    }
}
