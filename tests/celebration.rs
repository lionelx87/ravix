use ravix::celebrate::{Event, Intensity, next_intensity, should_celebrate};

#[test]
fn the_dial_cycles_full_subtle_off() {
    assert_eq!(next_intensity(Intensity::Full), Intensity::Subtle);
    assert_eq!(next_intensity(Intensity::Subtle), Intensity::Off);
    assert_eq!(next_intensity(Intensity::Off), Intensity::Full);
}

#[test]
fn off_suppresses_every_celebration() {
    for event in [Event::Commit, Event::Merge, Event::Push, Event::Checkout] {
        assert!(!should_celebrate(event, Intensity::Off));
    }
}

#[test]
fn celebrated_events_fire_at_subtle_and_full() {
    for intensity in [Intensity::Full, Intensity::Subtle] {
        assert!(should_celebrate(Event::Commit, intensity));
        assert!(should_celebrate(Event::Merge, intensity));
        assert!(should_celebrate(Event::Rebase, intensity));
        assert!(should_celebrate(Event::Pull, intensity));
        assert!(should_celebrate(Event::Push, intensity));
        assert!(should_celebrate(Event::Checkout, intensity));
    }
}

#[test]
fn low_weight_events_never_celebrate() {
    assert!(!should_celebrate(Event::Fetch, Intensity::Full));
    assert!(!should_celebrate(Event::Stash, Intensity::Full));
}
