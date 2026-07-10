pub fn fuzzy_filter(query: &str, labels: &[&str]) -> Vec<usize> {
    if query.is_empty() {
        return (0..labels.len()).collect();
    }
    let mut scored: Vec<(usize, i32)> = labels
        .iter()
        .enumerate()
        .filter_map(|(index, label)| score(query, label).map(|value| (index, value)))
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.into_iter().map(|(index, _)| index).collect()
}

fn score(query: &str, label: &str) -> Option<i32> {
    let query: Vec<char> = query.to_lowercase().chars().collect();
    let label: Vec<char> = label.to_lowercase().chars().collect();
    let mut matched = 0;
    let mut score = 0;
    let mut previous: Option<usize> = None;
    for (position, &character) in label.iter().enumerate() {
        if matched < query.len() && character == query[matched] {
            score += 1;
            if position == 0 || !label[position - 1].is_alphanumeric() {
                score += 10;
            }
            if previous.is_some_and(|previous| position == previous + 1) {
                score += 5;
            }
            previous = Some(position);
            matched += 1;
        }
    }
    (matched == query.len()).then_some(score)
}
