#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intensity {
    Full,
    Subtle,
    Off,
}

impl Intensity {
    pub fn label(self) -> &'static str {
        match self {
            Intensity::Full => "full",
            Intensity::Subtle => "subtle",
            Intensity::Off => "off",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Commit,
    Merge,
    CherryPick,
    Rebase,
    Pull,
    Push,
    Checkout,
    Fetch,
    Stash,
}

pub fn next_intensity(current: Intensity) -> Intensity {
    match current {
        Intensity::Full => Intensity::Subtle,
        Intensity::Subtle => Intensity::Off,
        Intensity::Off => Intensity::Full,
    }
}

pub fn should_celebrate(event: Event, intensity: Intensity) -> bool {
    if intensity == Intensity::Off {
        return false;
    }
    matches!(
        event,
        Event::Commit
            | Event::Merge
            | Event::CherryPick
            | Event::Rebase
            | Event::Pull
            | Event::Push
            | Event::Checkout
    )
}
