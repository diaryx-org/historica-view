//! Who a request is from, as the auth worker answers.
//!
//! The auth worker (`../auth/`) holds the accounts, their sessions and their
//! push keys, and the hub's worker reaches it through the `AUTH` service
//! binding alone. It is asked one of two things: whose session a browser's
//! cookie is, which better-auth answers at `/api/auth/get-session`, and whose
//! push key a bearer token is, at `/internal/key`.

use serde::Deserialize;
use wasm_bindgen::JsValue;
use worker::{Env, Headers, Method, Request, RequestInit, Result};

use crate::AUTH;

/// An account, as a request names it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Account {
    pub id: String,
    /// The account's handle, which its repositories are under; `None` until
    /// it chooses one.
    pub username: Option<String>,
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
    let Some(cookie) = request.headers().get("cookie")? else {
        return Ok(None);
    };
    let headers = Headers::new();
    headers.set("cookie", &cookie)?;
    let mut init = RequestInit::new();
    init.with_method(Method::Get).with_headers(headers);
    let mut answer = env
        .service(AUTH)?
        .fetch("https://auth/api/auth/get-session", Some(init))
        .await?;
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
