const NUMBERED_ROWS: usize = 9;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinSwitcherRow {
    pub name: String,
    pub is_head: bool,
    pub number: Option<usize>,
}

pub fn switcher_rows(
    pins: &[String],
    head: Option<&str>,
    history: &[String],
    local_name: impl Fn(&str) -> String,
) -> Vec<PinSwitcherRow> {
    let is_head = |pin: &String| head == Some(local_name(pin).as_str());
    let recency = |pin: &String| history.iter().position(|seen| *seen == local_name(pin));
    let mut ordered: Vec<&String> = pins.iter().collect();
    ordered.sort_by_key(|pin| (!is_head(pin), recency(pin).unwrap_or(usize::MAX)));
    if let Some(head_pins) = ordered.iter().rposition(|pin| is_head(pin)) {
        ordered.drain(1..=head_pins);
    }

    let mut next_number = 1;
    ordered
        .into_iter()
        .map(|pin| {
            let is_head = is_head(pin);
            let number = (!is_head && next_number <= NUMBERED_ROWS).then(|| {
                next_number += 1;
                next_number - 1
            });
            PinSwitcherRow {
                name: pin.clone(),
                is_head,
                number,
            }
        })
        .collect()
}

pub fn serialize(pins: &[String]) -> String {
    pins.iter().map(|pin| format!("{pin}\n")).collect()
}

pub fn parse(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}
