pub fn parse_progress(current: &str, total: &str) -> Option<(usize, usize)> {
    let current = current.trim().parse().ok()?;
    let total = total.trim().parse().ok()?;
    if total == 0 || current > total {
        return None;
    }
    Some((current, total))
}
