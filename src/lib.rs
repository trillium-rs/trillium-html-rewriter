#![forbid(unsafe_code)]
#![deny(
    clippy::dbg_macro,
    missing_copy_implementations,
    rustdoc::missing_crate_level_docs,
    missing_debug_implementations,
    missing_docs,
    nonstandard_style,
    unused_qualifications
)]
#![doc = include_str!("../README.md")]

use lol_async::rewrite;
pub use lol_async::{Settings, html};
use mime::Mime;
use std::{
    fmt::{self, Debug, Formatter},
    str::FromStr,
    sync::Arc,
};
use trillium::{
    Body, Conn, Handler,
    KnownHeaderName::{ContentLength, ContentType},
};

/// A trillium [`Handler`] that rewrites HTML response bodies with
/// [`lol-html`](https://docs.rs/lol-html), using [`lol-async`](https://docs.rs/lol-async).
///
/// It wraps the response produced by other handlers: in [`before_send`](Handler::before_send) it
/// inspects the outgoing `Content-Type` and, if the mime subtype is `html` (e.g. `text/html`),
/// replaces the response body with a streaming rewrite driven by the [`Settings`] returned from the
/// closure passed to [`HtmlRewriter::new`]. Responses with any other content type (or none) are
/// passed through unchanged.
pub struct HtmlRewriter {
    settings: Arc<dyn Fn() -> Settings<'static, 'static> + Send + Sync + 'static>,
}

impl Debug for HtmlRewriter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("HtmlRewriter").finish()
    }
}

impl Handler for HtmlRewriter {
    async fn before_send(&self, mut conn: Conn) -> Conn {
        let html = conn
            .response_headers()
            .get_str(ContentType)
            .and_then(|c| Mime::from_str(c).ok())
            .map(|m| m.subtype() == "html")
            .unwrap_or_default();

        if html && let Some(body) = conn.take_response_body() {
            let reader = rewrite(body, (self.settings)());
            conn.response_headers_mut().remove(ContentLength); // we no longer know the content length, if we ever did
            conn.with_body(Body::new_streaming(reader, None))
        } else {
            conn
        }
    }
}

impl HtmlRewriter {
    /// Construct a new html rewriter from a closure that builds [`Settings`].
    ///
    /// The closure — rather than a `Settings` value — is required because `lol-html`'s content
    /// handlers are single-use; it is invoked once per rewritten response to produce a fresh set of
    /// handlers. Build the settings with [`Settings::new_send()`] as the base (its handlers are
    /// `Send`, as required here) and populate `element_content_handlers` /
    /// `document_content_handlers`. See [`lol_async::html::Settings`] and the
    /// [`lol-html`](https://docs.rs/lol-html) docs for the full rewriting API.
    pub fn new(f: impl Fn() -> Settings<'static, 'static> + Send + Sync + 'static) -> Self {
        Self {
            settings: Arc::new(f)
                as Arc<dyn Fn() -> Settings<'static, 'static> + Send + Sync + 'static>,
        }
    }
}
