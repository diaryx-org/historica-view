//! What the browser does, as the worker renders it: nothing. The worker
//! renders each page once and sends no events, so none of this is reached
//! there; it is here so that one set of components builds for both.

use dioxus::prelude::*;
use unit::Unit;

use crate::Page;

/// Whether links are fetches: not until the browser takes over.
pub(crate) const TAKEN_OVER: bool = false;

pub(crate) fn go(_shown: Signal<Page>, _path: String, _push: bool) {}

pub(crate) fn use_history(_shown: Signal<Page>) {}

pub(crate) fn use_timer(_unit: Unit) {}

pub(crate) fn forget_query() {}

pub(crate) fn open(_url: &str) {}

pub(crate) async fn copy(_text: &str) -> bool {
    false
}

pub(crate) async fn refresh(_shown: Signal<Page>) {}

pub(crate) async fn auth(
    _path: &str,
    _body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    Err("only a browser signs in".to_owned())
}
