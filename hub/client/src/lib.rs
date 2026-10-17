//! The hub in the browser.
//!
//! The worker sends a rendered page with the data it was rendered from beside
//! it. This reads that data, clears what the worker rendered, and renders the
//! same components in its place, which from then on fetch each next page's
//! data instead of loading it. Where the data is missing or unreadable, the
//! worker's page stays as it is: every link in it is an ordinary link.

#![forbid(unsafe_code)]

use dioxus::prelude::*;
use historica_hub_app::{App, ROOT};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    let Some(page) = historica_hub_app::start() else {
        return;
    };
    // Rendering afresh rather than hydrating, so the worker's markup goes
    // first; the client's is the same markup, drawn from the same data.
    if let Some(root) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(ROOT))
    {
        root.set_inner_html("");
    }
    let dom = VirtualDom::new(App).with_root_context(page);
    dioxus_web::launch::launch_virtual_dom(dom, dioxus_web::Config::new().rootname(ROOT));
}
