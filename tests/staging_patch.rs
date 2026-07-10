use ogma::staging::{FileDiff, Hunk, build_patch};

fn sample() -> FileDiff {
    FileDiff {
        old_path: "src/staging.rs".to_string(),
        new_path: "src/staging.rs".to_string(),
        hunks: vec![
            Hunk {
                header: "@@ -12,7 +12,9 @@ fn stage_hunk".to_string(),
                lines: vec![
                    " context".to_string(),
                    "-let patch = legacy();".to_string(),
                    "+let patch = build();".to_string(),
                ],
            },
            Hunk {
                header: "@@ -40,6 +42,10 @@ impl StagingModel".to_string(),
                lines: vec![" tail".to_string(), "+added".to_string()],
            },
        ],
    }
}

#[test]
fn patch_for_one_hunk_has_the_file_header_and_only_that_hunk() {
    let patch = build_patch(&sample(), &[0]);

    assert_eq!(
        patch,
        concat!(
            "diff --git a/src/staging.rs b/src/staging.rs\n",
            "--- a/src/staging.rs\n",
            "+++ b/src/staging.rs\n",
            "@@ -12,7 +12,9 @@ fn stage_hunk\n",
            " context\n",
            "-let patch = legacy();\n",
            "+let patch = build();\n",
        )
    );
}

#[test]
fn patch_for_all_hunks_concatenates_them_in_order() {
    let patch = build_patch(&sample(), &[0, 1]);

    assert_eq!(
        patch,
        concat!(
            "diff --git a/src/staging.rs b/src/staging.rs\n",
            "--- a/src/staging.rs\n",
            "+++ b/src/staging.rs\n",
            "@@ -12,7 +12,9 @@ fn stage_hunk\n",
            " context\n",
            "-let patch = legacy();\n",
            "+let patch = build();\n",
            "@@ -40,6 +42,10 @@ impl StagingModel\n",
            " tail\n",
            "+added\n",
        )
    );
}
