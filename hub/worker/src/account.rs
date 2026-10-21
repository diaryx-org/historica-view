//! Who a request is from, as the auth worker answers.
//!
//! The auth worker (`../auth/`) holds the accounts, their sessions and their
//! push keys, and the hub's worker reaches it through the `AUTH` service
//! binding alone. It is asked one of two things: whose session a browser's
//! cookie is, which better-auth answers at `/api/auth/get-session`, and whose
//! push key a bearer token is, at `/internal/key`.

use historica_hub_app::Key;
use serde::Deserialize;
use wasm_bindgen::JsValue;
use worker::{Env, Headers, Method, Request, RequestInit, Response, Result};

use crate::AUTH;

/// An account, as a request names it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Account {
    pub id: String,
    /// The account's handle, which its repositories are under; `None` until
    /// it chooses one.
    pub username: Option<String>,
    /// The account's address, which a session's answer has and a push key's
    /// does not.
    #[serde(default)]
    pub email: Option<String>,
}

/// Who sent `request`: a push key as its bearer token, or else a session
/// cookie. `None` for neither, or for one that is not this hub's.
pub async fn of(env: &Env, request: &Request) -> Result<Option<Account>> {
    let bearer = request
        .headers()
        .get("authorization")?
        .and_then(|value| value.strip_prefix("Bearer ").map(str::to_owned));
    match bearer {
        Some(key) => by_key(env, &key).await,
        None => by_session(env, request).await,
    }
}

/// Whose session `request`'s cookie is.
pub async fn by_session(env: &Env, request: &Request) -> Result<Option<Account>> {
    let Some(mut answer) = as_session(env, request, "get-session").await? else {
        return Ok(None);
    };
    if answer.status_code() != 200 {
        return Ok(None);
    }
    #[derive(Deserialize)]
    struct Session {
        user: Account,
    }
    let session: Option<Session> = answer.json().await.unwrap_or(None);
    Ok(session.map(|session| session.user))
}

/// The push keys of the account whose session `request`'s cookie is: what
/// better-auth lists, which is never a key itself.
pub async fn keys(env: &Env, request: &Request) -> Result<Vec<Key>> {
    let Some(mut answer) = as_session(env, request, "api-key/list").await? else {
        return Ok(Vec::new());
    };
    if answer.status_code() != 200 {
        return Ok(Vec::new());
    }
    // A list, or a list under `apiKeys`, by better-auth's version.
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Listed {
        Bare(Vec<Key>),
        Wrapped {
            #[serde(rename = "apiKeys")]
            api_keys: Vec<Key>,
        },
    }
    Ok(match answer.json().await {
        Ok(Listed::Bare(keys) | Listed::Wrapped { api_keys: keys }) => keys,
        Err(_) => Vec::new(),
    })
}

/// A `GET` of better-auth's `path` under `/api/auth/`, with `request`'s
/// cookie: `None` with no cookie to send.
async fn as_session(env: &Env, request: &Request, path: &str) -> Result<Option<Response>> {
    let Some(cookie) = request.headers().get("cookie")? else {
        return Ok(None);
    };
    let headers = Headers::new();
    headers.set("cookie", &cookie)?;
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let answer = env
        .service(AUTH)?
        .fetch(format!("https://auth/api/auth/{path}"), Some(init))
        .await?;
    Ok(Some(answer))
}

/// Whose push key `key` is.
pub async fn by_key(env: &Env, key: &str) -> Result<Option<Account>> {
    let headers = Headers::new();
    headers.set("content-type", "application/json")?;
    let body = serde_json::json!({ "key": key }).to_string();
    let mut init = RequestInit::new();
    init.with_method(Method::Post)
        .with_headers(headers)
        .with_body(Some(JsValue::from_str(&body)));
    let mut answer = env
        .service(AUTH)?
        .fetch("https://auth/internal/key", Some(init))
        .await?;
    match answer.status_code() {
        200 => Ok(Some(answer.json().await?)),
        401 => Ok(None),
        status => {
            let why = answer.text().await.unwrap_or_default();
            Err(worker::Error::RustError(format!(
                "the hub's accounts answered {status}: {why}"
            )))
        }
    }
}
