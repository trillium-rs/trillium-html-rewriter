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

`HtmlRewriter` is constructed from a settings-building function, not a `Settings` value, because
`lol-html`'s content handlers are single-use: the function is invoked once per rewritten response
to build a fresh set of handlers. Three constructors are available, named for what the settings
may depend on:

* `HtmlRewriter::new(|| …)` — the same rewrite for every response
* `HtmlRewriter::new_with_conn(|conn| …)` — settings that depend on the request or response
* `HtmlRewriter::new_async(async |conn: &Conn| …)` — settings that additionally await async work

Use [`Settings::new_send()`](https://docs.rs/lol-async) as the base (its handlers are
`Send`, as required to move between trillium's tasks) and fill in `element_content_handlers` /
`document_content_handlers`. See the [`lol-html` docs](https://docs.rs/lol-html) for the full
rewriting API — [`element!`](https://docs.rs/lol-html/latest/lol_html/macro.element.html),
[`comments!`](https://docs.rs/lol-html/latest/lol_html/macro.comments.html),
[`text!`](https://docs.rs/lol-html/latest/lol_html/macro.text.html), and the
[`Element`](https://docs.rs/lol-html/latest/lol_html/html_content/struct.Element.html) API for
inspecting and mutating matched elements.

## Rewriting per request

The settings function passed to `HtmlRewriter::new_with_conn` receives the
[`Conn`](https://docs.rs/trillium/latest/trillium/struct.Conn.html) whose response is about to be
rewritten, so the rewrite can depend on the request — its path, its headers, or state set by an
earlier handler:

```rust
use trillium_html_rewriter::{
    HtmlRewriter, Settings,
    html::{element, html_content::ContentType},
};

HtmlRewriter::new_with_conn(|conn| {
    let path = conn.path().to_string();
    Settings::new_send().append_element_content_handler(element!("head", move |el| {
        el.prepend(&format!(r#"<link rel="canonical" href="{path}">"#), ContentType::Html);
        Ok(())
    }))
});
```

Data read from the conn must be **moved** into the content handlers, which outlive the conn
borrow, moving into the streaming body.

## Async settings

`HtmlRewriter::new_async` accepts an *async* function, which can await a database query, an http
request, or any other async work while borrowing the conn:

```rust
use trillium::Conn;
use trillium_html_rewriter::{
    HtmlRewriter, Settings,
    html::{element, html_content::ContentType},
};

async fn canonical_url(path: &str) -> String {
    format!("https://example.com{path}")
}

HtmlRewriter::new_async(async |conn: &Conn| {
    let url = canonical_url(conn.path()).await;
    Settings::new_send().append_element_content_handler(element!("head", move |el| {
        el.prepend(&format!(r#"<link rel="canonical" href="{url}">"#), ContentType::Html);
        Ok(())
    }))
});
```

Two things to know about the closure forms `new_async` accepts:

* The `&Conn` parameter annotation is required — type inference cannot supply it.
* An async closure must not capture its environment. To use captured state (a client handle,
  configuration, …), write a plain closure that clones what it needs — from its environment and
  from the conn — into an async block it returns:
  `move |conn: &Conn| { let client = client.clone(); async move { /* … */ } }`. See the
  `HtmlRewriter::new_async` docs for a complete example.

Whichever constructor is used, the settings function runs only for responses that are actually
rewritten, so no work is done for non-HTML responses.
