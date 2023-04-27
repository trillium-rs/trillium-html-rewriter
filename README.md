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
        conn.with_header("content-type", "text/html")
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
