//! What only the browser does: fetch the next page's data, move the address
//! bar, copy a command, wait for a toast, and call better-auth.

use std::pin::Pin;
use std::time::Duration;

use dioxus::core::{Runtime, ScopeId};
use dioxus::prelude::*;
use unit::Unit;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::Closure;
use wasm_bindgen_futures::JsFuture;

use crate::{PAGE_API, Page};

/// Whether links are fetches: from the moment the browser takes over.
pub(crate) const TAKEN_OVER: bool = true;

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

/// The page shown, fetched again where it is: the account page after a
/// change to the account.
pub(crate) async fn refresh(mut shown: Signal<Page>) {
    let Some(path) = web_sys::window().and_then(|window| window.location().pathname().ok()) else {
        return;
    };
    if let Some(page) = fetch(&path).await {
        shown.set(page);
    }
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

/// A call to better-auth under `/api/auth/`: its JSON answer, or the
/// message it failed with. `Null` for `body` is a `GET`.
pub(crate) async fn auth(path: &str, body: serde_json::Value) -> Result<serde_json::Value, String> {
    let window = web_sys::window().ok_or("no window")?;
    let init = web_sys::RequestInit::new();
    init.set_credentials(web_sys::RequestCredentials::SameOrigin);
    if !body.is_null() {
        init.set_method("POST");
        let headers = web_sys::Headers::new().map_err(|_| "no headers")?;
        let _ = headers.set("content-type", "application/json");
        init.set_headers(&headers);
        init.set_body(&wasm_bindgen::JsValue::from_str(&body.to_string()));
    }
    let response: web_sys::Response =
        JsFuture::from(window.fetch_with_str_and_init(&format!("/api/auth/{path}"), &init))
            .await
            .map_err(|_| "the hub did not answer".to_owned())?
            .dyn_into()
            .map_err(|_| "not a response")?;
    let text = match response.text() {
        Ok(text) => JsFuture::from(text)
            .await
            .ok()
            .and_then(|text| text.as_string())
            .unwrap_or_default(),
        Err(_) => String::new(),
    };
    let answer: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    if !response.ok() {
        let message = answer["message"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{} {}", response.status(), response.status_text()));
        return Err(message);
    }
    Ok(answer)
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

/// Give unit the browser's clock, so that a toast goes when it should.
pub(crate) fn use_timer(unit: Unit) {
    use_hook(move || unit.set_timer(sleep));
}

fn sleep(time: Duration) -> Pin<Box<dyn Future<Output = ()>>> {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                &resolve,
                i32::try_from(time.as_millis()).unwrap_or(i32::MAX),
            );
        }
    });
    Box::pin(async move {
        let _ = JsFuture::from(promise).await;
    })
}

/// Take the query out of the address bar, so that a reload does not read
/// a mailed link's token or error again.
pub(crate) fn forget_query() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let location = window.location();
    if location.search().is_ok_and(|search| !search.is_empty())
        && let (Ok(path), Ok(history)) = (location.pathname(), window.history())
    {
        let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&path));
    }
}

/// Go to `url`, away from the hub: GitHub's sign-in.
pub(crate) fn open(url: &str) {
    if let Some(window) = web_sys::window() {
        let _ = window.location().assign(url);
    }
}

/// Put `text` on the clipboard: whether it went.
pub(crate) async fn copy(text: &str) -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let written = window.navigator().clipboard().write_text(text);
    JsFuture::from(written).await.is_ok()
}

/// Take over the page the worker rendered: read the data it was rendered
/// from, and render the same components from it.
pub fn start() -> Option<Page> {
    let document = web_sys::window()?.document()?;
    let data = document.get_element_by_id(crate::DATA)?.text_content()?;
    serde_json::from_str(&data).ok()
}
