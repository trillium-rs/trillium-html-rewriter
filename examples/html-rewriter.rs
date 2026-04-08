use trillium_html_rewriter::{
    HtmlRewriter,
    html::{Settings, element, html_content::ContentType},
};
use trillium_proxy::Proxy;
use trillium_rustls::RustlsConfig;
use trillium_smol::ClientConfig;

pub fn main() {
    env_logger::init();
    let client_config = RustlsConfig::<ClientConfig>::default();
    trillium_smol::run((
        Proxy::new(client_config, "https://httpbin.org").without_halting(),
        HtmlRewriter::new(|| Settings {
            element_content_handlers: vec![element!("body", |el| {
                el.prepend("<h1>rewritten</h1>", ContentType::Html);
                Ok(())
            })],

            ..Settings::new_send()
        }),
    ));
}
