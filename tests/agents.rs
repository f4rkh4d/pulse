use std::time::Duration;

use pulse::agents::{derive_mood, face_for, format_line, Agent, Mood, Species};
use pulse::service::Status;

#[test]
fn all_species_have_unique_happy_faces() {
    let faces = [
        Species::Goblin,
        Species::Cat,
        Species::Ghost,
        Species::Robot,
        Species::Blob,
    ]
    .map(|s| face_for(s, Mood::Happy));
    let mut set: Vec<_> = faces.iter().collect();
    set.sort();
    set.dedup();
    assert_eq!(set.len(), 5);
}

#[test]
fn happy_when_running_clean() {
    let m = derive_mood(Status::Running, 0, false, None, false);
    assert_eq!(m, Mood::Happy);
}

#[test]
fn dizzy_when_slow() {
    let m = derive_mood(Status::Running, 0, true, None, false);
    assert_eq!(m, Mood::Dizzy);
}

#[test]
fn sleepy_on_stop() {
    let m = derive_mood(Status::Stopped, 0, false, None, false);
    assert_eq!(m, Mood::Sleepy);
}

#[test]
fn sleepy_on_long_idle_while_running() {
    let m = derive_mood(
        Status::Running,
        0,
        false,
        Some(Duration::from_secs(600)),
        false,
    );
    assert_eq!(m, Mood::Sleepy);
}

#[test]
fn format_line_substitutes_name() {
    assert_eq!(format_line("{name} alive", "web"), "web alive");
}

#[test]
fn agent_transitions_only_emit_on_change() {
    let mut a = Agent::new(Species::Robot);
    a.mood = Mood::Happy;
    // happy -> happy: no emit
    assert_eq!(a.update_mood(Status::Running, 0, false, None, false), None);
    // happy -> dead: emit
    assert!(a
        .update_mood(Status::Crashed, 0, false, None, false)
        .is_some());
}
