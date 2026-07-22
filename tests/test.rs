use trillium::Conn;
use trillium_html_rewriter::{
    HtmlRewriter, Settings,
    html::{element, html_content::ContentType},
};
use trillium_testing::{TestServer, harness, test};

async fn html(conn: Conn) -> Conn {
    conn.with_response_header("content-type", "text/html")
        .with_status(200)
        .with_body("<html><head></head><body></body></html>")
}

/// A zero-argument settings function applies the same rewrite to every response.
#[test(harness)]
async fn static_settings() {
    let handler = (
        html,
        HtmlRewriter::new(|| {
            Settings::new_send().append_element_content_handler(element!("body", |el| {
                el.prepend("<h1>rewritten</h1>", ContentType::Html);
                Ok(())
            }))
        }),
    );

    let app = TestServer::new(handler).await;

    app.get("/")
        .await
        .assert_ok()
        .assert_body(r#"<html><head></head><body><h1>rewritten</h1></body></html>"#);
}

/// The settings function sees the conn whose response is being rewritten, so two requests to the
/// same handler can be rewritten differently. The closure parameter needs no type annotation.
#[test(harness)]
async fn settings_may_depend_on_the_conn() {
    let handler = (
        html,
        HtmlRewriter::new_with_conn(|conn| {
            let path = conn.path().to_string();
            Settings::new_send().append_element_content_handler(element!("head", move |el| {
                el.prepend(
                    &format!(r#"<meta name="path" content="{path}">"#),
                    ContentType::Html,
                );
                Ok(())
            }))
        }),
    );

    let app = TestServer::new(handler).await;

    app.get("/one")
        .await
        .assert_ok()
        .assert_body(r#"<html><head><meta name="path" content="/one"></head><body></body></html>"#);

    app.get("/two")
        .await
        .assert_ok()
        .assert_body(r#"<html><head><meta name="path" content="/two"></head><body></body></html>"#);
}

/// The settings function is async and may borrow the conn across an await.
#[test(harness)]
async fn settings_may_await_while_borrowing_the_conn() {
    async fn lookup_meta_content(path: &str) -> String {
        futures_lite::future::yield_now().await;
        format!("looked up {path}")
    }

    let handler = (
        html,
        HtmlRewriter::new_async(async |conn: &Conn| {
            let content = lookup_meta_content(conn.path()).await;
            Settings::new_send().append_element_content_handler(element!("head", move |el| {
                el.prepend(
                    &format!(r#"<meta name="lookup" content="{content}">"#),
                    ContentType::Html,
                );
                Ok(())
            }))
        }),
    );

    let app = TestServer::new(handler).await;

    app.get("/one").await.assert_ok().assert_body(
        r#"<html><head><meta name="lookup" content="looked up /one"></head><body></body></html>"#,
    );
}

/// Captured state is supported by returning a `'static` future from a plain closure.
#[test(harness)]
async fn settings_function_may_use_captured_state() {
    let site_name = String::from("example");

    let handler = (
        html,
        HtmlRewriter::new_async(move |conn: &Conn| {
            let site_name = site_name.clone();
            let path = conn.path().to_string();
            async move {
                futures_lite::future::yield_now().await;
                Settings::new_send().append_element_content_handler(element!("head", move |el| {
                    el.prepend(
                        &format!(r#"<meta name="site" content="{site_name}{path}">"#),
                        ContentType::Html,
                    );
                    Ok(())
                }))
            }
        }),
    );

    let app = TestServer::new(handler).await;

    app.get("/one").await.assert_ok().assert_body(
        r#"<html><head><meta name="site" content="example/one"></head><body></body></html>"#,
    );
}
