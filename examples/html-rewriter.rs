use trillium_html_rewriter::{
    html::{element, html_content::ContentType, Settings},
    HtmlRewriter,
};
use trillium_proxy::Proxy;
use trillium_rustls::RustlsConfig;
use trillium_smol::ClientConfig;

pub fn main() {
    env_logger::init();
    trillium_smol::run((
        Proxy::new(
            RustlsConfig::<ClientConfig>::default(),
            "http://neverssl.com",
        ),
        HtmlRewriter::new(|| Settings {
            element_content_handlers: vec![element!("body", |el| {
                el.prepend("<h1>rewritten</h1>", ContentType::Html);
                Ok(())
            })],

            ..Settings::default()
        }),
    ));
}
