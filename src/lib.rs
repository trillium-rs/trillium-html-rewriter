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
    future::{Future, ready},
    pin::Pin,
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
/// replaces the response body with a streaming rewrite driven by the [`Settings`] returned from
/// the settings function passed to [`new`](HtmlRewriter::new),
/// [`new_with_conn`](HtmlRewriter::new_with_conn), or [`new_async`](HtmlRewriter::new_async).
/// Responses with any other content type (or none) are passed through unchanged.
pub struct HtmlRewriter {
    settings: Arc<dyn ErasedSettingsFn>,
}

/// An async function from [`&Conn`](Conn) to [`Settings`], as accepted by
/// [`HtmlRewriter::new_async`].
///
/// This trait is implemented for any `async` closure or `async fn` that takes a `&Conn` and
/// returns `Settings<'static, 'static>`, as well as for plain closures that return such a future.
/// The lifetime parameter allows the future to borrow the `Conn` across `.await` points; because
/// of that, the bound in [`HtmlRewriter::new_async`] is the higher-ranked
/// `F: for<'a> SettingsFn<'a>`.
///
/// You should not need to implement or name this trait directly — write one of the closure forms
/// documented on [`HtmlRewriter::new_async`].
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not an async settings builder",
    label = "expected an async function from `&Conn` to `Settings<'static, 'static>`",
    note = "if the settings don't require awaiting anything, use `HtmlRewriter::new` or \
            `HtmlRewriter::new_with_conn` instead of `HtmlRewriter::new_async`",
    note = "write an async closure with an annotated parameter: `async |conn: &Conn| {{ .. }}`, \
            not `|conn| async move {{ .. }}` — and the `&Conn` annotation is required for \
            inference",
    note = "async closures that capture state don't implement `Fn`; to use captured state, write \
            a plain closure returning an async block that owns its data: `move |conn: &Conn| {{ \
            let data = data.clone(); async move {{ .. }} }}`",
    note = "the returned future must be `Send`"
)]
pub trait SettingsFn<'a>: Send + Sync + 'static {
    /// The future returned by [`call`](SettingsFn::call).
    type Fut: Future<Output = Settings<'static, 'static>> + Send + 'a;

    /// Build the [`Settings`] that will rewrite the response on this [`Conn`].
    fn call(&self, conn: &'a Conn) -> Self::Fut;
}

impl<'a, F, Fut> SettingsFn<'a> for F
where
    F: Fn(&'a Conn) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Settings<'static, 'static>> + Send + 'a,
{
    type Fut = Fut;

    fn call(&self, conn: &'a Conn) -> Fut {
        self(conn)
    }
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Object-safe form of [`SettingsFn`] for storage in [`HtmlRewriter`].
trait ErasedSettingsFn: Send + Sync {
    fn call<'a>(&'a self, conn: &'a Conn) -> BoxFuture<'a, Settings<'static, 'static>>;
}

impl<F> ErasedSettingsFn for F
where
    F: for<'a> SettingsFn<'a>,
{
    fn call<'a>(&'a self, conn: &'a Conn) -> BoxFuture<'a, Settings<'static, 'static>> {
        Box::pin(SettingsFn::call(self, conn))
    }
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
            let settings = self.settings.call(&conn).await;
            let reader = rewrite(body, settings);
            conn.response_headers_mut().remove(ContentLength); // we no longer know the content length, if we ever did
            conn.with_body(Body::new_streaming(reader, None))
        } else {
            conn
        }
    }
}

impl HtmlRewriter {
    /// Construct a new html rewriter that applies the same rewrite to every response.
    ///
    /// A function — rather than a `Settings` value — is required because `lol-html`'s content
    /// handlers are single-use; it is invoked once per rewritten response to produce a fresh set
    /// of handlers. Build the settings with [`Settings::new_send()`] as the base (its handlers are
    /// `Send`, as required here) and populate `element_content_handlers` /
    /// `document_content_handlers`. See [`lol_async::html::Settings`] and the
    /// [`lol-html`](https://docs.rs/lol-html) docs for the full rewriting API.
    ///
    /// ```
    /// # use trillium_html_rewriter::{HtmlRewriter, Settings, html::{element, html_content::ContentType}};
    /// HtmlRewriter::new(|| {
    ///     Settings::new_send().append_element_content_handler(element!("body", |el| {
    ///         el.prepend(r#"<script src="/analytics.js"></script>"#, ContentType::Html);
    ///         Ok(())
    ///     }))
    /// });
    /// ```
    ///
    /// To vary the rewrite based on the request or response, see
    /// [`new_with_conn`](Self::new_with_conn); to await async work while building the settings,
    /// see [`new_async`](Self::new_async).
    pub fn new(f: impl Fn() -> Settings<'static, 'static> + Send + Sync + 'static) -> Self {
        Self {
            settings: Arc::new(move |_: &Conn| ready(f())),
        }
    }

    /// Construct a new html rewriter from a function that builds [`Settings`] for a given
    /// [`Conn`].
    ///
    /// The function receives the conn whose response is about to be rewritten, so the rewrite can
    /// depend on the request path, headers, or [state](Conn::state). Like all three constructors,
    /// the function is invoked once per rewritten response — see [`new`](Self::new) for why a
    /// function is required and how to build the settings — and it only runs for responses that
    /// are actually rewritten, so no work is done for non-html responses.
    ///
    /// Data read from the conn must be *moved* into the content handlers, which outlive the conn
    /// borrow:
    ///
    /// ```
    /// # use trillium_html_rewriter::{HtmlRewriter, Settings, html::{element, html_content::ContentType}};
    /// HtmlRewriter::new_with_conn(|conn| {
    ///     let path = conn.path().to_string();
    ///     Settings::new_send().append_element_content_handler(element!("head", move |el| {
    ///         el.prepend(&format!(r#"<link rel="canonical" href="{path}">"#), ContentType::Html);
    ///         Ok(())
    ///     }))
    /// });
    /// ```
    ///
    /// To await async work while building the settings, see [`new_async`](Self::new_async).
    pub fn new_with_conn(
        f: impl Fn(&Conn) -> Settings<'static, 'static> + Send + Sync + 'static,
    ) -> Self {
        Self {
            settings: Arc::new(move |conn: &Conn| ready(f(conn))),
        }
    }

    /// Construct a new html rewriter from an async function that builds [`Settings`] for a given
    /// [`Conn`].
    ///
    /// Like [`new_with_conn`](Self::new_with_conn), the function receives the conn whose response
    /// is about to be rewritten; because it is async, it can also await while borrowing the conn,
    /// so the settings can incorporate the result of async work such as a database query or an
    /// http request. Like all three constructors, the function is invoked once per rewritten
    /// response — see [`new`](Self::new) for why a function is required and how to build the
    /// settings.
    ///
    /// # Supported forms
    ///
    /// The parameter type annotation `: &Conn` is required on closures — inference cannot supply
    /// it through the higher-ranked [`SettingsFn`] bound.
    ///
    /// An async closure (or equivalently a named `async fn(&Conn) -> Settings<'static, 'static>`)
    /// may borrow the conn across `.await`, but must not capture its environment:
    ///
    /// ```
    /// # use trillium_html_rewriter::{HtmlRewriter, Settings, html::{element, html_content::ContentType}};
    /// # use trillium::Conn;
    /// # async fn canonical_url(path: &str) -> String { format!("https://example.com{path}") }
    /// HtmlRewriter::new_async(async |conn: &Conn| {
    ///     let url = canonical_url(conn.path()).await;
    ///     Settings::new_send().append_element_content_handler(element!("head", move |el| {
    ///         el.prepend(&format!(r#"<link rel="canonical" href="{url}">"#), ContentType::Html);
    ///         Ok(())
    ///     }))
    /// });
    /// ```
    ///
    /// To use captured state (a client handle, configuration, …), write a plain closure that
    /// clones what it needs — from its environment and from the conn — into an async block it
    /// returns. In this form the future cannot borrow the conn:
    ///
    /// ```
    /// # use trillium_html_rewriter::{HtmlRewriter, Settings, html::{element, html_content::ContentType}};
    /// # use trillium::Conn;
    /// # #[derive(Clone)] struct Client;
    /// # impl Client { async fn fetch_banner(&self, path: String) -> String { path } }
    /// # let client = Client;
    /// HtmlRewriter::new_async(move |conn: &Conn| {
    ///     let client = client.clone();
    ///     let path = conn.path().to_string();
    ///     async move {
    ///         let banner = client.fetch_banner(path).await;
    ///         Settings::new_send().append_element_content_handler(element!("body", move |el| {
    ///             el.prepend(&banner, ContentType::Html);
    ///             Ok(())
    ///         }))
    ///     }
    /// });
    /// ```
    ///
    /// Note that in either form, data destined for the content handlers must be *moved* into
    /// them, as they outlive the settings-building future.
    pub fn new_async<F>(f: F) -> Self
    where
        F: for<'a> SettingsFn<'a>,
    {
        Self {
            settings: Arc::new(f),
        }
    }
}
