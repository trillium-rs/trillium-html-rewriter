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

/*!
trillium handler that rewrites html using lol-html and the
[`lol_async`] library.

this crate currently requires configuring the runtime,
unfortunately. use one of the `"async-std"`, `"smol"`, or `"tokio"`
features. do not use the default feature, it may change at any time.

```
use trillium_html_rewriter::{
    html::{element, html_content::ContentType, Settings},
    HtmlRewriter,
};

let handler = (
    |conn: trillium::Conn| async move {
        conn.with_header(("content-type", "text/html"))
            .with_status(200)
            .with_body("<html><body><p>body</p></body></html>")
    },
    HtmlRewriter::new(|| Settings {
        element_content_handlers: vec![element!("body", |el| {
            el.prepend("<h1>title</h1>", ContentType::Html);
            Ok(())
        })],
         ..Settings::default()
    }),
);

# async_global_executor::block_on(async move {
use trillium_testing::prelude::*;

let conn = async_global_executor::spawn(async move {
    get("/").run_async(&handler).await
}).await;

assert_ok!(conn, "<html><body><h1>title</h1><p>body</p></body></html>");
# });
*/

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
            dbg!("HERE");
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
