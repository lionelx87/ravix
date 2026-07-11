use ogma::submodule::breadcrumb_label;
use std::path::{Path, PathBuf};

#[test]
fn a_root_repo_shows_just_its_name() {
    let label = breadcrumb_label(&[], Path::new("/home/dev/myrepo"));

    assert_eq!(label, "myrepo");
}

#[test]
fn one_level_deep_shows_parent_then_sub() {
    let ancestors = vec![PathBuf::from("/home/dev/app")];

    let label = breadcrumb_label(&ancestors, Path::new("/home/dev/app/vendor/libcore"));

    assert_eq!(label, "app › libcore");
}

#[test]
fn nested_submodules_join_with_a_separator() {
    let ancestors = vec![
        PathBuf::from("/home/dev/app"),
        PathBuf::from("/home/dev/app/vendor/lib"),
    ];

    let label = breadcrumb_label(&ancestors, Path::new("/home/dev/app/vendor/lib/inner"));

    assert_eq!(label, "app › lib › inner");
}
