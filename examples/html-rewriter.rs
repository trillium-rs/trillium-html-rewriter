use trillium_html_rewriter::{
    HtmlRewriter,
    html::{Settings, element, html_content::ContentType},
};
use trillium_proxy::Proxy;
use trillium_smol::ClientConfig;

pub fn main() {
    env_logger::init();
    trillium_smol::run((
        Proxy::new(ClientConfig::default(), "http://httpbin.org"),
        HtmlRewriter::new(|| {
            Settings::new_send().append_element_content_handler(element!("body", |el| {
                el.prepend("<h1>rewritten</h1>", ContentType::Html);
                Ok(())
            }))
        }),
    ));
}
