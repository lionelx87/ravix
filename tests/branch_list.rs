use ogma::branches::{BranchEntry, BranchInput, branch_list};

fn plain(name: &str) -> BranchInput {
    BranchInput {
        name: name.to_string(),
        upstream: None,
        ahead: 0,
        behind: 0,
    }
}

#[test]
fn the_current_branch_leads_and_the_rest_follow_alphabetically() {
    let branches = [plain("main"), plain("feature"), plain("bugfix")];

    let list = branch_list(&branches, Some("feature"));

    let names: Vec<&str> = list.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["feature", "bugfix", "main"]);
    assert_eq!(
        list.iter().filter(|entry| entry.is_head).count(),
        1,
        "exactly one entry is HEAD"
    );
    assert!(
        list[0].is_head,
        "the current branch leads and is marked HEAD"
    );
}

#[test]
fn a_detached_head_marks_no_branch_and_keeps_alphabetical_order() {
    let branches = [plain("main"), plain("feature")];

    let list = branch_list(&branches, None);

    let names: Vec<&str> = list.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["feature", "main"]);
    assert!(list.iter().all(|entry| !entry.is_head), "nothing is HEAD");
}

#[test]
fn upstream_and_ahead_behind_pass_through_to_the_entry() {
    let branches = [BranchInput {
        name: "main".to_string(),
        upstream: Some("origin/main".to_string()),
        ahead: 2,
        behind: 3,
    }];

    let list = branch_list(&branches, Some("main"));

    assert_eq!(
        list[0],
        BranchEntry {
            name: "main".to_string(),
            is_head: true,
            upstream: Some("origin/main".to_string()),
            ahead: 2,
            behind: 3,
        }
    );
}
