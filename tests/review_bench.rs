use std::time::{Duration, Instant};

use ravix::git::Repo;
use ravix::review::{BranchReview, detect_base};

const SLOWEST_SHOWN: usize = 8;

#[test]
#[ignore]
fn opening_a_review_on_every_branch_of_the_bench_repo() {
    let path = std::env::var("RAVIX_BENCH_REPO").expect("set RAVIX_BENCH_REPO");
    let repo = Repo::discover(&path).unwrap();
    let mut timings: Vec<(Duration, Duration, usize, String, Option<String>)> = repo
        .branches()
        .unwrap()
        .into_iter()
        .map(|branch| {
            let started = Instant::now();
            let base = detect_base(&repo, &branch.name);
            let detected = started.elapsed();
            let started = Instant::now();
            let rows = base
                .as_deref()
                .and_then(|base| BranchReview::open(&repo, &branch.name, base))
                .map_or(0, |review| review.rows.len());
            (detected, started.elapsed(), rows, branch.name, base)
        })
        .collect();
    timings.sort_by_key(|(detected, opened, ..)| std::cmp::Reverse(*detected + *opened));

    let total: Duration = timings
        .iter()
        .map(|(detected, opened, ..)| *detected + *opened)
        .sum();
    println!(
        "{} branches, mean {:?}",
        timings.len(),
        total / timings.len().max(1) as u32
    );
    for (detected, opened, rows, branch, base) in timings.iter().take(SLOWEST_SHOWN) {
        println!("{detected:>12?} detect {opened:>12?} open {rows:>5} rows  {branch} vs {base:?}");
    }
}

#[test]
#[ignore]
fn detecting_the_base_of_every_branch_of_the_bench_repo() {
    let path = std::env::var("RAVIX_BENCH_REPO").expect("set RAVIX_BENCH_REPO");
    let repo = Repo::discover(&path).unwrap();
    let mut timings: Vec<(Duration, String, Option<String>)> = repo
        .branches()
        .unwrap()
        .into_iter()
        .map(|branch| {
            let started = Instant::now();
            let base = detect_base(&repo, &branch.name);
            (started.elapsed(), branch.name, base)
        })
        .collect();
    timings.sort_by_key(|(detected, ..)| std::cmp::Reverse(*detected));
    let total: Duration = timings.iter().map(|(detected, ..)| *detected).sum();
    println!(
        "{} branches, mean {:?}",
        timings.len(),
        total / timings.len().max(1) as u32
    );
    for (detected, branch, base) in timings.iter().take(SLOWEST_SHOWN) {
        println!("{detected:>12?} detect  {branch} vs {base:?}");
    }
}
