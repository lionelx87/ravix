use std::time::Duration;

use git2::{Oid, Repository, RepositoryInitOptions, Signature, Time};
use tempfile::TempDir;

use ravix::app::{Action, App};

fn settle(app: &mut App) {
    app.update(Action::Tick(Duration::from_millis(200)));
}

fn commit(repo: &Repository, base: Option<Oid>, path: &str, message: &str) -> Oid {
    let signature = Signature::new("Dev", "dev@example.com", &Time::new(1000, 0)).unwrap();
    let blob = repo.blob(b"content\n").unwrap();
    let base_tree = base.map(|oid| repo.find_commit(oid).unwrap().tree().unwrap());
    let mut builder = repo.treebuilder(base_tree.as_ref()).unwrap();
    builder.insert(path, blob, 0o100644).unwrap();
    let tree = repo.find_tree(builder.write().unwrap()).unwrap();
    let parents: Vec<_> = base
        .into_iter()
        .map(|oid| repo.find_commit(oid).unwrap())
        .collect();
    let parent_refs: Vec<_> = parents.iter().collect();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parent_refs,
    )
    .unwrap()
}

fn distinct_file_repo(dir: &TempDir) {
    let mut opts = RepositoryInitOptions::new();
    opts.initial_head("main");
    let repo = Repository::init_opts(dir.path(), &opts).unwrap();

    let root = commit(&repo, None, "a.txt", "add a");
    let second = commit(&repo, Some(root), "b.txt", "add b");
    commit(&repo, Some(second), "c.txt", "add c");
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
        .unwrap();
}

fn panel_paths(app: &App) -> Vec<String> {
    app.panel()
        .expect("a panel is open")
        .changed_files
        .iter()
        .map(|file| file.path.clone())
        .collect()
}

#[test]
fn the_open_panel_follows_the_selection_to_a_new_commit() {
    let dir = TempDir::new().unwrap();
    distinct_file_repo(&dir);
    let mut app = App::open(dir.path()).unwrap();

    app.update(Action::OpenPanel);
    settle(&mut app);
    assert_eq!(
        panel_paths(&app),
        vec!["c.txt".to_string()],
        "the freshly opened panel shows the HEAD commit's files"
    );

    app.update(Action::SelectNext);
    settle(&mut app);
    assert_eq!(
        panel_paths(&app),
        vec!["b.txt".to_string()],
        "moving to the next commit refreshes the file list"
    );
    assert_eq!(
        app.panel().unwrap().commit_index,
        app.selected(),
        "the panel tracks the selected commit index"
    );

    app.update(Action::SelectPrev);
    settle(&mut app);
    assert_eq!(
        panel_paths(&app),
        vec!["c.txt".to_string()],
        "moving back restores the previous commit's file list"
    );
}
