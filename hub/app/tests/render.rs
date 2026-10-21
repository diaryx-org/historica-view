//! The pages, rendered as the worker renders them.

use dioxus::prelude::*;
use historica_hub_app::{
    App, Head, Key, Listed, Me, Overview, Page, Repo, embedded, stylesheet, stylesheet_version,
};
use historica_view::workspace::Readme;

fn render(page: Page) -> String {
    let mut dom = VirtualDom::new(App).with_root_context(page);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

fn repo() -> Repo {
    Repo {
        owner: "ada".into(),
        slug: "historica".into(),
        private: false,
        forked_from: None,
        origin: "https://hub.example.org".into(),
        revisions: 3,
        head: Head {
            id: "2686b27369fed20b8cb8999c19396fc6f195ce36d68048848a7432ec61ffaa46".into(),
            title: "Describe the hub".into(),
            author: "Ada Lovelace".into(),
            when: "2026-10-16T18:00:00Z".into(),
        },
    }
}

#[test]
fn home_lists_the_repositories() {
    let html = render(Page::Home {
        repositories: vec![
            Listed {
                owner: "ada".into(),
                slug: "historica".into(),
            },
            Listed {
                owner: "grace".into(),
                slug: "historica".into(),
            },
        ],
    });
    assert!(html.contains(r#"href="/ada/historica""#), "{html}");
    assert!(html.contains(r#"href="/grace/historica""#), "{html}");
}

#[test]
fn an_overview_has_the_readme_and_both_copy_commands() {
    let html = render(Page::Overview {
        repo: repo(),
        overview: Overview {
            title: Some("Historica".into()),
            description: Some("Readable version control".into()),
            readme: Some(Readme {
                file: "f".into(),
                path: "README.md".into(),
                html: "<h1>Historica</h1><p>The readable files are the authority.</p>".into(),
            }),
        },
    });
    assert!(
        html.contains("<h1>Historica</h1>"),
        "the README goes in as HTML: {html}"
    );
    assert!(html.contains("historica fetch https://hub.example.org/ada/historica/offer.txt"));
    assert!(html.contains("git clone https://hub.example.org/ada/historica.git"));
    assert!(
        html.contains(r#"href="/ada/historica/browse/#history/2686b"#),
        "{html}"
    );
    assert!(!html.contains("Private"), "{html}");
    assert!(html.contains("3 revisions"), "{html}");
    assert!(html.contains("2686b27369"), "{html}");
}

#[test]
fn page_data_cannot_close_its_script_element() {
    let page = Page::Missing {
        why: "</script><script>alert(1)</script>".into(),
    };
    let data = embedded(&page);
    assert!(!data.contains("</script>"), "{data}");
    let back: Page = serde_json::from_str(&data).expect("still JSON");
    assert_eq!(back, page);
}

#[test]
fn a_private_fork_says_so_and_links_its_parent() {
    let html = render(Page::Overview {
        repo: Repo {
            owner: "grace".into(),
            private: true,
            forked_from: Some("ada/historica".into()),
            ..repo()
        },
        overview: Overview {
            title: None,
            description: None,
            readme: None,
        },
    });
    assert!(html.contains("Private"), "{html}");
    assert!(html.contains(r#"href="/ada/historica""#), "{html}");
    assert!(html.contains("git clone https://hub.example.org/grace/historica.git"));
}

#[test]
fn every_page_is_unit_s_without_the_stylesheet_inline() {
    let html = render(Page::Missing { why: "x".into() });
    assert!(html.contains("unit-root hub"), "{html}");
    assert!(!html.contains("<style"), "{html}");
    // The served stylesheet has unit's theme and the hub's own.
    assert!(stylesheet().contains("--unit-accent"));
    assert!(stylesheet().contains(".hub-header"));
    assert_eq!(stylesheet_version().len(), 16);
}

fn account(me: Option<Me>, reset: Option<String>) -> Page {
    Page::Account {
        me,
        origin: "https://hub.example.org".into(),
        reset,
        problem: None,
    }
}

#[test]
fn signed_out_the_account_page_asks_for_a_password_a_manager_can_fill() {
    let html = render(account(None, None));
    assert!(html.contains("Sign in"), "{html}");
    assert!(html.contains("Create account"), "{html}");
    assert!(
        html.contains(r#"autocomplete="current-password""#),
        "{html}"
    );
    assert!(html.contains(r#"type="email""#), "{html}");
    assert!(!html.contains("Push keys"), "{html}");
}

#[test]
fn signed_in_the_account_page_lists_the_keys_and_never_a_key() {
    let html = render(account(
        Some(Me {
            email: "ada@example.org".into(),
            handle: Some("ada".into()),
            keys: vec![Key {
                id: "k1".into(),
                name: Some("laptop".into()),
                start: Some("hub_ab".into()),
            }],
        }),
        None,
    ));
    assert!(html.contains("Signed in as ada@example.org."), "{html}");
    assert!(html.contains("/ada/&#60;name&#62;"), "{html}");
    assert!(html.contains("laptop"), "{html}");
    assert!(html.contains("hub_ab…"), "{html}");
    assert!(html.contains("Make a push key"), "{html}");
}

#[test]
fn a_reset_link_asks_for_the_new_password_alone() {
    let html = render(account(None, Some("t0ken".into())));
    assert!(html.contains("Choose a new password"), "{html}");
    assert!(html.contains(r#"autocomplete="new-password""#), "{html}");
    assert!(!html.contains("Create account"), "{html}");
}
