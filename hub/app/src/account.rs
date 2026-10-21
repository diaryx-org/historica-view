//! `/account/`: signing in, choosing a handle, and the push keys
//! `historica hub push` sends.
//!
//! The worker renders it from the session the request's cookie is, as it
//! renders any page; every change is a call to better-auth under
//! `/api/auth/`, which the hub's worker passes to the auth worker, so the
//! session cookie is the hub's own. After a change the page's data is
//! fetched again, and the page drawn from it.

use dioxus::prelude::*;
use serde_json::json;
use unit::prelude::*;

use crate::{Command, Key, Me, Page, Shown, client};

/// Where a link from the hub's mail comes back to, signed in.
const BACK: &str = "/account/";

/// What the page says after a press: a notice, or what went wrong.
#[derive(Clone, PartialEq)]
enum Said {
    Notice(String),
    Problem(String),
}

#[component]
pub(crate) fn AccountPage(
    me: Option<Me>,
    origin: String,
    reset: Option<String>,
    problem: Option<String>,
) -> Element {
    let said = use_signal(|| problem.map(Said::Problem));
    // The new key, shown once.
    let made = use_signal(|| None::<String>);
    use_hook(client::forget_query);
    rsx! {
        Heading { level: 1, "Account" }
        Told { said }
        match (reset, me) {
            (Some(token), _) => rsx! { Reset { token, said } },
            (None, Some(me)) => rsx! { SignedIn { me, origin, said, made } },
            (None, None) => rsx! { SignIn { said } },
        }
    }
}

/// The notice or the problem, until it is dismissed or the next press.
#[component]
fn Told(said: Signal<Option<Said>>) -> Element {
    let mut said = said;
    match said() {
        Some(Said::Notice(text)) => rsx! {
            Callout { ondismiss: move |_| said.set(None), "{text}" }
        },
        Some(Said::Problem(text)) => rsx! {
            Callout { tone: Tone::Danger, ondismiss: move |_| said.set(None), "{text}" }
        },
        None => rsx! {},
    }
}

/// Run a call to better-auth, say what it answered, and draw the page again
/// from its data.
fn act(
    shown: Signal<Page>,
    mut said: Signal<Option<Said>>,
    call: impl Future<Output = Result<Option<String>, String>> + 'static,
) {
    said.set(None);
    spawn(async move {
        match call.await {
            Ok(notice) => {
                client::refresh(shown).await;
                said.set(notice.map(Said::Notice));
            }
            Err(problem) => said.set(Some(Said::Problem(problem))),
        }
    });
}

#[component]
fn SignIn(said: Signal<Option<Said>>) -> Element {
    let Shown(shown) = use_context::<Shown>();
    let mut name = use_signal(String::new);
    let mut handle = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut password = use_signal(String::new);
    let sign_in = move || {
        let (email, password) = (email(), password());
        act(shown, said, async move {
            client::auth(
                "sign-in/email",
                json!({ "email": email, "password": password, "callbackURL": BACK }),
            )
            .await
            .map(|_| None)
            .map_err(|problem| unverified(problem, &email))
        });
    };
    let sign_up = move || {
        let (email, password) = (email(), password());
        let name = match name() {
            name if name.is_empty() => email.split('@').next().unwrap_or_default().to_owned(),
            name => name,
        };
        let handle = Some(handle()).filter(|handle| !handle.is_empty());
        act(shown, said, async move {
            let answer = client::auth(
                "sign-up/email",
                json!({
                    "name": name, "email": email, "password": password,
                    "username": handle, "callbackURL": BACK,
                }),
            )
            .await
            .map_err(|problem| unverified(problem, &email))?;
            // Signed up but not in: the hub sent a link to verify the address.
            Ok(answer["token"].is_null().then(|| {
                format!("We sent a link to {email}. Open it to verify your email and sign in.")
            }))
        });
    };
    let forgot = move || {
        let email = email();
        if email.is_empty() {
            said.clone()
                .set(Some(Said::Problem("Write your email above first.".into())));
            return;
        }
        act(shown, said, async move {
            client::auth(
                "request-password-reset",
                json!({ "email": email, "redirectTo": BACK }),
            )
            .await?;
            Ok(Some(format!(
                "If {email} has an account here, we sent it a link to choose a new password."
            )))
        });
    };
    let github = move || {
        let mut said = said;
        spawn(async move {
            match client::auth(
                "sign-in/social",
                json!({ "provider": "github", "callbackURL": BACK }),
            )
            .await
            {
                Ok(answer) if answer["url"].is_string() => {
                    client::open(answer["url"].as_str().unwrap_or_default());
                }
                _ => said.set(Some(Said::Problem(
                    "GitHub sign-in is not set up on this hub.".into(),
                ))),
            }
        });
    };
    rsx! {
        Heading { level: 2, "Sign in" }
        Stack { class: "hub-form",
            TextField {
                value: email(),
                oninput: move |v| email.set(v),
                label: "Email",
                kind: InputKind::Email,
                name: "email",
                autocomplete: "email",
                required: true,
            }
            TextField {
                value: password(),
                oninput: move |v| password.set(v),
                label: "Password",
                kind: InputKind::Password,
                name: "password",
                autocomplete: "current-password",
                required: true,
                minlength: 8,
                onsubmit: move |_| sign_in(),
            }
            Stack { axis: Axis::Horizontal, wrap: true,
                Button { variant: Variant::Primary, onpress: move |_| sign_in(), "Sign in" }
                Button { variant: Variant::Ghost, onpress: move |_| github(), "Sign in with GitHub" }
                Button { variant: Variant::Ghost, onpress: move |_| forgot(), "Forgot password" }
            }
        }
        Heading { level: 2, "Create an account" }
        Text { dim: true,
            "With the email and password above, and a name and a handle. Your "
            "handle is where your repositories are: "
            code { "/<handle>/<name>" }
            "."
        }
        Stack { class: "hub-form",
            TextField {
                value: name(),
                oninput: move |v| name.set(v),
                label: "Name",
                name: "name",
                autocomplete: "name",
            }
            TextField {
                value: handle(),
                oninput: move |v| handle.set(v),
                label: "Handle",
                placeholder: "ada",
                name: "username",
                autocomplete: "username",
                maxlength: 39,
            }
            Stack { axis: Axis::Horizontal,
                Button { onpress: move |_| sign_up(), "Create account" }
            }
        }
    }
}

/// better-auth's refusal of an address not yet verified, which has sent it
/// a new link, said as that.
fn unverified(problem: String, email: &str) -> String {
    if problem.to_ascii_lowercase().contains("verif") {
        format!("Verify your email first: we sent a new link to {email}.")
    } else {
        problem
    }
}

#[component]
fn Reset(token: String, said: Signal<Option<Said>>) -> Element {
    let Shown(shown) = use_context::<Shown>();
    let mut password = use_signal(String::new);
    let set = move || {
        let (password, token) = (password(), token.clone());
        act(shown, said, async move {
            client::auth(
                "reset-password",
                json!({ "newPassword": password, "token": token }),
            )
            .await?;
            Ok(Some(
                "Your password is changed. Sign in with it.".to_owned(),
            ))
        });
    };
    rsx! {
        Heading { level: 2, "Choose a new password" }
        Stack { class: "hub-form",
            TextField {
                value: password(),
                oninput: move |v| password.set(v),
                label: "New password",
                kind: InputKind::Password,
                name: "password",
                autocomplete: "new-password",
                required: true,
                minlength: 8,
                action: "Set password",
                onaction: move |_| set(),
            }
        }
    }
}

#[component]
fn SignedIn(
    me: Me,
    origin: String,
    said: Signal<Option<Said>>,
    made: Signal<Option<String>>,
) -> Element {
    let Shown(shown) = use_context::<Shown>();
    let unit = use_unit();
    let mut handle = use_signal(|| me.handle.clone().unwrap_or_default());
    let mut key_name = use_signal(String::new);
    let shown_handle = me.handle.clone().unwrap_or_else(|| "<handle>".to_owned());
    let set_handle = move || {
        let username = handle();
        act(shown, said, async move {
            client::auth("update-user", json!({ "username": username })).await?;
            Ok(None)
        });
    };
    let make_key = move || {
        let name = key_name();
        let mut made = made;
        act(shown, said, async move {
            let created = client::auth("api-key/create", json!({ "name": name })).await?;
            made.set(created["key"].as_str().map(str::to_owned));
            key_name.clone().set(String::new());
            Ok(None)
        });
    };
    let sign_out = move || {
        let mut made = made;
        made.set(None);
        act(shown, said, async move {
            client::auth("sign-out", json!({})).await?;
            Ok(None)
        });
    };
    let delete = move |key: Key| {
        let label = key.name.clone().unwrap_or_else(|| "unnamed".to_owned());
        unit.open_dialog(
            Dialog::new(format!("Delete the key “{label}”?"))
                .message("A push sending it is refused from now on.")
                .buttons("Delete", "Cancel")
                .destructive(),
            move |answer| {
                if let Answer::Ok(_) = answer {
                    let id = key.id.clone();
                    act(shown, said, async move {
                        client::auth("api-key/delete", json!({ "keyId": id })).await?;
                        Ok(None)
                    });
                }
            },
        );
    };
    rsx! {
        Stack { axis: Axis::Horizontal, align: Align::Center, wrap: true,
            Text { dim: true, "Signed in as {me.email}." }
            Button { variant: Variant::Ghost, onpress: move |_| sign_out(), "Sign out" }
        }
        Heading { level: 2, "Your handle" }
        Text { dim: true,
            "Your repositories are at "
            code { "/{shown_handle}/<name>" }
            ", and a fork you make is under it too. A handle is lowercase letters, "
            "digits and hyphens. Changing it moves every repository of yours to the new one."
        }
        Stack { class: "hub-form",
            TextField {
                value: handle(),
                oninput: move |v| handle.set(v),
                label: "Handle",
                placeholder: "ada",
                name: "username",
                autocomplete: "username",
                required: true,
                maxlength: 39,
                action: "Set handle",
                action_disabled: handle().is_empty(),
                onaction: move |_| set_handle(),
            }
        }
        Heading { level: 2, "Push keys" }
        Text { dim: true,
            code { "historica hub login" }
            " keeps one of these, and "
            code { "historica hub push" }
            " sends it. Your first push to a name under your handle makes that "
            "repository, and only your keys can push to it after. Each push says who "
            "may read it, with "
            code { "--public" }
            " or "
            code { "--private" }
            ", or is asked."
        }
        if let Some(key) = made() {
            Callout { tone: Tone::Success, title: "Your new key",
                p { "It is shown once, so copy it now." }
                Section { class: "hub-clone",
                    Command { label: "key", text: key }
                    Command { label: "login", text: "historica hub login {origin}" }
                }
            }
        }
        if !me.keys.is_empty() {
            Section { class: "hub-keys",
                for key in me.keys.clone() {
                    Row {
                        key: "{key.id}",
                        title: key.name.clone().unwrap_or_else(|| "unnamed".to_owned()),
                        subtitle: "{key.start.clone().unwrap_or_default()}…",
                        trailing: rsx! {
                            Button {
                                variant: Variant::Danger,
                                onpress: {
                                    let key = key.clone();
                                    move |_| delete(key.clone())
                                },
                                "Delete"
                            }
                        },
                    }
                }
            }
        }
        Stack { class: "hub-form",
            TextField {
                value: key_name(),
                oninput: move |v| key_name.set(v),
                label: "What the key is for",
                placeholder: "laptop",
                name: "key-name",
                required: true,
                maxlength: 32,
                action: "Make a push key",
                action_disabled: key_name().is_empty(),
                onaction: move |_| make_key(),
            }
        }
    }
}
