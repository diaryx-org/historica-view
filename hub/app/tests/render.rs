//! The pages, rendered as the worker renders them.

use dioxus::prelude::*;
use historica_hub_app::{App, Head, Listed, Overview, Page, Repo, Task, TaskDetail, embedded};
use historica_view::workspace::Readme;

fn render(page: Page) -> String {
    let mut dom = VirtualDom::new(App).with_root_context(page);
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

fn repo() -> Repo {
    Repo {
        slug: "historica".into(),
        origin: "https://hub.example.org".into(),
        revisions: 3,
        head: Head {
            id: "2686b27369fed20b8cb8999c19396fc6f195ce36d68048848a7432ec61ffaa46".into(),
            title: "Open a task".into(),
            author: "Ada Lovelace".into(),
            when: "2026-10-16T18:00:00Z".into(),
        },
    }
}

fn task(title: &str, status: &str, closed: bool) -> Task {
    Task {
        file: format!("file-{title}"),
        path: format!("docs/tasks/{title}.md"),
        title: title.into(),
        status: status.into(),
        closed,
        created: Some("2026-10-01T00:00:00Z".into()),
        updated: None,
        author: Some("ada".into()),
        description: None,
    }
}

#[test]
fn home_lists_the_repositories() {
    let html = render(Page::Home {
        repositories: vec![
            Listed {
                slug: "historica".into(),
            },
            Listed {
                slug: "prov".into(),
            },
        ],
    });
    assert!(html.contains(r#"href="/historica""#), "{html}");
    assert!(html.contains(r#"href="/prov""#), "{html}");
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
            open_tasks: 4,
            closed_tasks: 10,
        },
    });
    assert!(
        html.contains("<h1>Historica</h1>"),
        "the README goes in as HTML: {html}"
    );
    assert!(html.contains("historica fetch https://hub.example.org/historica/offer.txt"));
    assert!(html.contains("git clone https://hub.example.org/historica.git"));
    assert!(html.contains("4 open tasks"), "{html}");
    assert!(html.contains("2686b27369"), "{html}");
}

#[test]
fn a_task_list_shows_one_side_and_counts_both() {
    let tasks = vec![
        task("open-one", "open", false),
        task("in-progress-one", "in-progress", false),
        task("done-one", "done", true),
    ];
    let open = render(Page::Tasks {
        repo: repo(),
        tasks: tasks.clone(),
        closed: false,
    });
    assert!(
        open.contains("2 Open") && open.contains("1 Closed"),
        "{open}"
    );
    assert!(
        open.contains("open-one") && !open.contains("done-one"),
        "{open}"
    );
    let closed = render(Page::Tasks {
        repo: repo(),
        tasks,
        closed: true,
    });
    assert!(
        closed.contains("done-one") && !closed.contains("open-one"),
        "{closed}"
    );
}

#[test]
fn a_task_page_has_its_body_and_its_file() {
    let html = render(Page::Task {
        repo: repo(),
        task: TaskDetail {
            task: task("a-merge-agrees-by-text", "open", false),
            html: "<p><strong>Done when</strong> it does.</p>".into(),
        },
    });
    assert!(html.contains("<strong>Done when</strong>"), "{html}");
    assert!(
        html.contains("docs/tasks/a-merge-agrees-by-text.md"),
        "{html}"
    );
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
