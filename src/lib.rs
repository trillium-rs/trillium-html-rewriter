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

/**
trillium handler for html rewriting
*/
pub struct HtmlRewriter {
    settings: Arc<dyn Fn() -> Settings<'static, 'static> + Send + Sync + 'static>,
}

impl Debug for HtmlRewriter {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("HtmlRewriter").finish()
    }
}

impl Handler for HtmlRewriter {
    async fn run(&self, mut conn: Conn) -> Conn {
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
    /**
    construct a new html rewriter from the provided `fn() -> Settings`. See
    [`lol_async::html::Settings`] for more information.
     */
    pub fn new(f: impl Fn() -> Settings<'static, 'static> + Send + Sync + 'static) -> Self {
        Self {
            settings: Arc::new(f)
                as Arc<dyn Fn() -> Settings<'static, 'static> + Send + Sync + 'static>,
        }
    }
}
