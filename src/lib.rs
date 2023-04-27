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

use cfg_if::cfg_if;
pub use lol_async::html;
use lol_async::{html::Settings, rewrite};
use mime::Mime;
use std::{future::Future, str::FromStr};
use trillium::{
    async_trait, Body, Conn, Handler,
    KnownHeaderName::{ContentLength, ContentType},
};

/**
trillium handler for html rewriting
*/
pub struct HtmlRewriter {
    settings: Box<dyn Fn() -> Settings<'static, 'static> + Send + Sync + 'static>,
}

impl std::fmt::Debug for HtmlRewriter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HtmlRewriter").finish()
    }
}

fn spawn_local(fut: impl Future + 'static) {
    cfg_if! {
        if #[cfg(feature = "async-std")] {
            async_std_crate::task::spawn_local(fut);
        } else if #[cfg(feature = "smol")] {
            async_global_executor::spawn_local(fut).detach();
        } else if #[cfg(feature = "tokio")] {
            tokio_crate::task::spawn_local(fut);
        } else {
            async_global_executor::spawn_local(fut).detach();
        }
    }
}

#[async_trait]
impl Handler for HtmlRewriter {
    async fn run(&self, mut conn: Conn) -> Conn {
        let html = conn
            .headers_mut()
            .get_str(ContentType)
            .and_then(|c| Mime::from_str(c).ok())
            .map(|m| m.subtype() == "html")
            .unwrap_or_default();

        if html && conn.inner().response_body().is_some() {
            let body = conn.inner_mut().take_response_body().unwrap();
            let (fut, reader) = rewrite(body, (self.settings)());
            spawn_local(fut);
            conn.headers_mut().remove(ContentLength); // we no longer know the content length, if we ever did
            conn.with_body(Body::new_streaming(reader, None))
        } else {
            conn
        }
    }
}

impl HtmlRewriter {
    /**
    construct a new html rewriter from the provided `fn() ->
    Settings`. See [`lol_async::html::Settings`] for more information.
     */
    pub fn new(f: impl Fn() -> Settings<'static, 'static> + Send + Sync + 'static) -> Self {
        Self {
            settings: Box::new(f)
                as Box<dyn Fn() -> Settings<'static, 'static> + Send + Sync + 'static>,
        }
    }
}
