[`trillium`](https://docs.rs/trillium) handler that rewrites html using
[`lol-html`](https://docs.rs/lol-html) through the [`lol-async`](https://docs.rs/lol-async) library.

`HtmlRewriter` wraps another handler and transforms its response body as it streams to the client,
using [`lol-html`](https://docs.rs/lol-html)'s CSS-selector-based rewriting API. It's well suited to
sitting in front of a [`Proxy`](https://docs.rs/trillium-proxy) or other handlers that you don't
have direct control over, letting you inject tags, rewrite attributes, or strip elements from an
outbound HTML body without buffering the whole response.

```rust
use trillium_html_rewriter::{
    HtmlRewriter, Settings,
    html::{element, html_content::ContentType},
};

let handler = (
    |conn: trillium::Conn| async move {
        conn.with_response_header("content-type", "text/html")
            .with_status(200)
            .with_body("<html><body><p>body</p></body></html>")
    },
    HtmlRewriter::new(|| {
        Settings::new_send().append_element_content_handler(element!("body", |el| {
            el.prepend("<h1>title</h1>", ContentType::Html);
            Ok(())
        }))
    }),
);

use trillium_testing::{TestServer, block_on};

block_on(async move {
    let app = TestServer::new(handler).await;
    app.get("/")
        .await
        .assert_ok()
        .assert_body("<html><body><h1>title</h1><p>body</p></body></html>");
});
```

## Behavior

* **Only HTML responses are rewritten.** Before touching anything, the handler checks the outgoing
  `Content-Type`: a response is rewritten only if its mime subtype is `html`
  (e.g. `text/html`). Responses with any other content type — or no `Content-Type` at all — pass
  through untouched, so it's safe to place in front of a handler that serves a mix of HTML, JSON,
  and binary data.

* **It transforms the response in `before_send`, so composition order is forgiving.** The rewriting
  runs in the handler's
  [`before_send`](https://docs.rs/trillium/latest/trillium/trait.Handler.html#method.before_send)
  phase, which trillium runs for every handler in a tuple regardless of whether an earlier handler
  halted. So `HtmlRewriter` rewrites whatever HTML body the rest of the tuple produced. An example
  using trillium-proxy:

  ```rust,no_run
  use trillium_html_rewriter::{HtmlRewriter, html::{Settings, element, html_content::ContentType}};
  use trillium_proxy::Proxy;
  use trillium_smol::ClientConfig;

  trillium_smol::run((
      Proxy::new(ClientConfig::default(), "http://example.com"),
      HtmlRewriter::new(|| {
          Settings::new_send().append_element_content_handler(element!("body", |el| {
              el.prepend("<h1>rewritten</h1>", ContentType::Html);
              Ok(())
          }))
      })
  ));
  ```

* **The response becomes a stream of unknown length.** Because rewriting is streaming, the rewritten
  body's length isn't known up front, so any `Content-Length` header is removed and the response is
  sent with chunked transfer encoding (HTTP/1.1) or the equivalent for HTTP/2 and HTTP/3.

## Constructing settings

`HtmlRewriter::new` takes a `Fn() -> Settings`, not a `Settings` value, because `lol-html`'s content
handlers are single-use: the closure is invoked once per rewritten response to build a fresh set of
handlers. Use [`Settings::new_send()`](https://docs.rs/lol-async) as the base (its handlers are
`Send`, as required to move between trillium's tasks) and fill in `element_content_handlers` /
`document_content_handlers`. See the [`lol-html` docs](https://docs.rs/lol-html) for the full
rewriting API — [`element!`](https://docs.rs/lol-html/latest/lol_html/macro.element.html),
[`comments!`](https://docs.rs/lol-html/latest/lol_html/macro.comments.html),
[`text!`](https://docs.rs/lol-html/latest/lol_html/macro.text.html), and the
[`Element`](https://docs.rs/lol-html/latest/lol_html/html_content/struct.Element.html) API for
inspecting and mutating matched elements.
