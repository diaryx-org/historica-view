//! What only the browser does: fetch the next page's data, move the address
//! bar, and copy a command.

use dioxus::core::{Runtime, ScopeId};
use dioxus::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen_futures::JsFuture;

use crate::{PAGE_API, Page};

/// Show the page at `path`, fetching its data, and record it in the
/// browser's history when `push`.
///
/// Where the data cannot be had, the browser goes to `path` the ordinary way,
/// and the worker answers it as it answers any page load.
pub(crate) fn go(mut shown: Signal<Page>, path: String, push: bool) {
    spawn(async move {
        let Some(window) = web_sys::window() else {
            return;
        };
        match fetch(&path).await {
            Some(page) => {
                if let Some(document) = window.document() {
                    document.set_title(&page.title());
                }
                if push && let Ok(history) = window.history() {
                    let _ =
                        history.push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&path));
                }
                shown.set(page);
                window.scroll_to_with_x_and_y(0.0, 0.0);
            }
            None => {
                let _ = window.location().assign(&path);
            }
        }
    });
}

async fn fetch(path: &str) -> Option<Page> {
    let window = web_sys::window()?;
    let url = format!(
        "{PAGE_API}?path={}",
        String::from(js_sys::encode_uri_component(path))
    );
    let response: web_sys::Response = JsFuture::from(window.fetch_with_str(&url))
        .await
        .ok()?
        .dyn_into()
        .ok()?;
    if !response.ok() {
        return None;
    }
    let text = JsFuture::from(response.text().ok()?)
        .await
        .ok()?
        .as_string()?;
    serde_json::from_str(&text).ok()
}

/// Follow the browser's back and forward buttons.
pub(crate) fn use_history(shown: Signal<Page>) {
    use_hook(move || {
        let runtime = Runtime::current();
        let listener = Closure::<dyn FnMut(web_sys::PopStateEvent)>::new(move |_| {
            let Some(path) = web_sys::window().and_then(|window| window.location().pathname().ok())
            else {
                return;
            };
            runtime.in_scope(ScopeId::ROOT, || go(shown, path, false));
        });
        if let Some(window) = web_sys::window() {
            let _ = window
                .add_event_listener_with_callback("popstate", listener.as_ref().unchecked_ref());
        }
        // The page lives as long as the tab does.
        listener.forget();
    });
}

/// Put `text` on the clipboard, and say so for a moment.
pub(crate) fn copy(text: String, mut copied: Signal<bool>) {
    spawn(async move {
        let Some(window) = web_sys::window() else {
            return;
        };
        let written = window.navigator().clipboard().write_text(&text);
        if JsFuture::from(written).await.is_ok() {
            copied.set(true);
        }
    });
}

/// Take over the page the worker rendered: read the data it was rendered
/// from, and render the same components from it.
pub fn start() -> Option<Page> {
    let document = web_sys::window()?.document()?;
    let data = document.get_element_by_id(crate::DATA)?.text_content()?;
    serde_json::from_str(&data).ok()
}
