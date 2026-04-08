fn main() {
    use trillium_html_rewriter::{
        HtmlRewriter, Settings,
        html::{element, html_content::ContentType},
    };
    use trillium_testing::{TestServer, block_on};

    let handler = (
        |conn: trillium::Conn| async move {
            conn.with_response_header("content-type", "text/html")
                .with_status(200)
                .with_body("<html><body><p>body</p></body></html>")
        },
        HtmlRewriter::new(|| Settings {
            element_content_handlers: vec![element!("body", |el| {
                el.prepend("<h1>title</h1>", ContentType::Html);
                Ok(())
            })],
            ..Settings::new_send()
        }),
    );

    block_on(async move {
        let app = TestServer::new(handler).await;
        app.get("/")
            .await
            .assert_ok()
            .assert_body("<html><body><h1>title</h1><p>body</p></body></html>");
    });
}
