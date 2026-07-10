use ogma::join::{Ancestry, MergePrediction, MergeTreeResult, classify};

fn clean() -> MergeTreeResult {
    MergeTreeResult {
        conflicted: false,
        files: vec![],
    }
}

#[test]
fn a_source_already_contained_in_head_is_up_to_date() {
    let ancestry = Ancestry {
        source_in_head: true,
        head_in_source: false,
    };

    assert_eq!(classify(ancestry, &clean()), MergePrediction::UpToDate);
}

#[test]
fn head_being_an_ancestor_of_the_source_fast_forwards() {
    let ancestry = Ancestry {
        source_in_head: false,
        head_in_source: true,
    };

    assert_eq!(classify(ancestry, &clean()), MergePrediction::FastForward);
}

#[test]
fn diverged_histories_that_merge_cleanly_are_clean() {
    let ancestry = Ancestry {
        source_in_head: false,
        head_in_source: false,
    };

    assert_eq!(classify(ancestry, &clean()), MergePrediction::Clean);
}

#[test]
fn diverged_histories_with_a_conflict_report_the_files() {
    let ancestry = Ancestry {
        source_in_head: false,
        head_in_source: false,
    };
    let merge_tree = MergeTreeResult {
        conflicted: true,
        files: vec!["src/app.rs".into(), "README.md".into()],
    };

    assert_eq!(
        classify(ancestry, &merge_tree),
        MergePrediction::Conflicts {
            files: vec!["src/app.rs".into(), "README.md".into()],
        }
    );
}
