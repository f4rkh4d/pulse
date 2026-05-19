//! ASCII sentinels that live next to each service in the sidebar. react to
//! service state + probe state. occasionally say something in the status bar.
//!
//! five species: goblin, cat, ghost, robot, blob. pick one via
//! `[service.agent] kind = "cat"`.

use std::time::{Duration, Instant};

use rand::seq::SliceRandom;

use crate::service::Status;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Species {
    Goblin,
    Cat,
    Ghost,
    Robot,
    Blob,
}

impl Species {
    pub fn parse(s: &str) -> Self {
        match s {
            "cat" => Species::Cat,
            "ghost" => Species::Ghost,
            "robot" => Species::Robot,
            "blob" => Species::Blob,
            _ => Species::Goblin,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Species::Goblin => "goblin",
            Species::Cat => "cat",
            Species::Ghost => "ghost",
            Species::Robot => "robot",
            Species::Blob => "blob",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mood {
    Happy,
    Starting,
    Dead,
    Dizzy,
    Sleepy,
    Alert,
}

#[derive(Debug, Clone)]
pub struct Agent {
    pub species: Species,
    pub mood: Mood,
    pub last_mood_change: Instant,
    pub last_line: Option<String>,
}

impl Agent {
    pub fn new(species: Species) -> Self {
        Self {
            species,
            mood: Mood::Starting,
            last_mood_change: Instant::now(),
            last_line: None,
        }
    }

    /// derive mood from service status + probe health + recent traffic.
    pub fn update_mood(
        &mut self,
        status: Status,
        probe_fails: u32,
        slow_probe: bool,
        idle_for: Option<Duration>,
        traffic_spike: bool,
    ) -> Option<Mood> {
        let new = derive_mood(status, probe_fails, slow_probe, idle_for, traffic_spike);
        if new != self.mood {
            let prev = self.mood;
            self.mood = new;
            self.last_mood_change = Instant::now();
            // surface transitions the caller cares about
            match (prev, new) {
                (_, Mood::Dead)
                | (_, Mood::Dizzy)
                | (Mood::Dead, Mood::Happy)
                | (Mood::Dizzy, Mood::Happy)
                | (_, Mood::Alert) => Some(new),
                _ => None,
            }
        } else {
            None
        }
    }

    /// pick a line for the current mood. avoids repeating `last_line` when possible.
    pub fn speak(&mut self, mood: Mood, rng: &mut impl rand::Rng) -> String {
        let pool = lines_for(self.species, mood);
        let choice = pick_fresh(pool, self.last_line.as_deref(), rng);
        self.last_line = Some(choice.to_string());
        choice.to_string()
    }

    pub fn face(&self) -> &'static str {
        face_for(self.species, self.mood)
    }
}

pub fn derive_mood(
    status: Status,
    probe_fails: u32,
    slow_probe: bool,
    idle_for: Option<Duration>,
    traffic_spike: bool,
) -> Mood {
    match status {
        Status::Crashed | Status::CrashedTooMany => Mood::Dead,
        Status::Starting => Mood::Starting,
        Status::Stopped => Mood::Sleepy,
        Status::Running => {
            if probe_fails >= 2 || slow_probe {
                Mood::Dizzy
            } else if traffic_spike {
                Mood::Alert
            } else if idle_for.map(|d| d.as_secs() >= 60).unwrap_or(false) {
                Mood::Sleepy
            } else {
                Mood::Happy
            }
        }
    }
}

fn pick_fresh<'a>(pool: &'a [&'a str], avoid: Option<&str>, rng: &mut impl rand::Rng) -> &'a str {
    if pool.is_empty() {
        return "";
    }
    let filtered: Vec<&&str> = pool.iter().filter(|s| Some(**s) != avoid).collect();
    if filtered.is_empty() {
        return pool.choose(rng).copied().unwrap_or(pool[0]);
    }
    filtered.choose(rng).map(|s| **s).unwrap_or(pool[0])
}

/// faces per (species, mood). kept short so sidebar layout doesn't jitter.
pub fn face_for(species: Species, mood: Mood) -> &'static str {
    match (species, mood) {
        (Species::Goblin, Mood::Happy) => "ᨀ",
        (Species::Goblin, Mood::Starting) => "Ծ",
        (Species::Goblin, Mood::Dead) => "X_X",
        (Species::Goblin, Mood::Dizzy) => "@_@",
        (Species::Goblin, Mood::Sleepy) => "-_-",
        (Species::Goblin, Mood::Alert) => "O_O",

        (Species::Cat, Mood::Happy) => "=^.^=",
        (Species::Cat, Mood::Starting) => "=o.o=",
        (Species::Cat, Mood::Dead) => "=x.x=",
        (Species::Cat, Mood::Dizzy) => "=@.@=",
        (Species::Cat, Mood::Sleepy) => "=-.-=",
        (Species::Cat, Mood::Alert) => "=O.O=",

        (Species::Ghost, Mood::Happy) => "ʘ‿ʘ",
        (Species::Ghost, Mood::Starting) => "ʘᴗʘ",
        (Species::Ghost, Mood::Dead) => "✕‿✕",
        (Species::Ghost, Mood::Dizzy) => "@‿@",
        (Species::Ghost, Mood::Sleepy) => "-‿-",
        (Species::Ghost, Mood::Alert) => "O‿O",

        (Species::Robot, Mood::Happy) => "[o_o]",
        (Species::Robot, Mood::Starting) => "[-_-]",
        (Species::Robot, Mood::Dead) => "[x_x]",
        (Species::Robot, Mood::Dizzy) => "[@_@]",
        (Species::Robot, Mood::Sleepy) => "[z_z]",
        (Species::Robot, Mood::Alert) => "[!_!]",

        (Species::Blob, Mood::Happy) => "(•ᴗ•)",
        (Species::Blob, Mood::Starting) => "(•ω•)",
        (Species::Blob, Mood::Dead) => "(x__x)",
        (Species::Blob, Mood::Dizzy) => "(@_@)",
        (Species::Blob, Mood::Sleepy) => "(-_-)",
        (Species::Blob, Mood::Alert) => "(O_O)",
    }
}

/// per-species, per-mood message pools. each name is swapped in with `{name}`.
pub fn lines_for(species: Species, mood: Mood) -> &'static [&'static str] {
    match (species, mood) {
        (Species::Goblin, Mood::Dead) => &[
            "uh, that service just died",
            "{name} is facedown. again.",
            "rip {name}, gone but not forgotten (it's been 3s)",
            "{name} exploded. smells weird",
        ],
        (Species::Goblin, Mood::Happy) => &[
            "{name} back up. third time's the charm",
            "{name} alive again. don't jinx it",
            "ok {name} is behaving",
        ],
        (Species::Goblin, Mood::Dizzy) => &[
            "{name} probe hit 2s+, you awake?",
            "something's wrong with {name}, it's wobbling",
            "{name} feels slow today",
        ],
        (Species::Goblin, Mood::Alert) => {
            &["{name} getting hammered rn", "oh, {name} woke up suddenly"]
        }
        (Species::Goblin, Mood::Sleepy) => {
            &["{name} hasn't heard anything in a while", "{name} napping"]
        }
        (Species::Goblin, Mood::Starting) => &["{name} booting, give it a sec"],

        (Species::Cat, Mood::Dead) => &[
            "{name} knocked something off the counter. it died",
            "{name} ran away",
            "{name} stopped purring",
        ],
        (Species::Cat, Mood::Happy) => &["{name} purring again", "{name} back, wants treats"],
        (Species::Cat, Mood::Dizzy) => &[
            "{name} batting at invisible bugs",
            "{name} doing the loaf, probe is slow",
        ],
        (Species::Cat, Mood::Alert) => &["{name} ears up, something moved", "{name} on the hunt"],
        (Species::Cat, Mood::Sleepy) => &["{name} curled up, idle", "{name} snoozing"],
        (Species::Cat, Mood::Starting) => &["{name} stretching"],

        (Species::Ghost, Mood::Dead) => &[
            "{name} vanished completely",
            "{name}... gone. double-gone this time",
        ],
        (Species::Ghost, Mood::Happy) => &[
            "{name} materialized again",
            "{name} haunts the port once more",
        ],
        (Species::Ghost, Mood::Dizzy) => &[
            "{name} is flickering",
            "{name} probe feels thin, not quite there",
        ],
        (Species::Ghost, Mood::Alert) => &["{name} senses something", "{name} rattles its chains"],
        (Species::Ghost, Mood::Sleepy) => &["{name} dozing in the walls"],
        (Species::Ghost, Mood::Starting) => &["{name} forming..."],

        (Species::Robot, Mood::Dead) => &[
            "{name}: SEGFAULT. reboot recommended",
            "{name}: exit 1. not graceful",
            "{name}: runtime error detected. flatlined",
        ],
        (Species::Robot, Mood::Happy) => &[
            "{name}: systems nominal",
            "{name}: back online. logs look fine",
        ],
        (Species::Robot, Mood::Dizzy) => &[
            "{name}: latency exceeds threshold",
            "{name}: response time degraded",
        ],
        (Species::Robot, Mood::Alert) => {
            &["{name}: traffic spike detected", "{name}: req/s rising"]
        }
        (Species::Robot, Mood::Sleepy) => &["{name}: idle cycles"],
        (Species::Robot, Mood::Starting) => &["{name}: initializing"],

        (Species::Blob, Mood::Dead) => &["{name} splatted", "{name} fell apart"],
        (Species::Blob, Mood::Happy) => &["{name} reformed, jiggly again"],
        (Species::Blob, Mood::Dizzy) => &["{name} wobbling bad"],
        (Species::Blob, Mood::Alert) => &["{name} shaking, lots of pings"],
        (Species::Blob, Mood::Sleepy) => &["{name} flat, resting"],
        (Species::Blob, Mood::Starting) => &["{name} gathering itself"],
    }
}

pub fn format_line(template: &str, name: &str) -> String {
    template.replace("{name}", name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn mood_flips_on_crash() {
        let m = derive_mood(Status::Crashed, 0, false, None, false);
        assert_eq!(m, Mood::Dead);
    }

    #[test]
    fn dizzy_on_probe_fails() {
        let m = derive_mood(Status::Running, 3, false, None, false);
        assert_eq!(m, Mood::Dizzy);
    }

    #[test]
    fn alert_on_spike() {
        let m = derive_mood(Status::Running, 0, false, None, true);
        assert_eq!(m, Mood::Alert);
    }

    #[test]
    fn sleepy_when_idle_long() {
        let m = derive_mood(
            Status::Running,
            0,
            false,
            Some(Duration::from_secs(120)),
            false,
        );
        assert_eq!(m, Mood::Sleepy);
    }

    #[test]
    fn all_species_have_all_faces() {
        for sp in [
            Species::Goblin,
            Species::Cat,
            Species::Ghost,
            Species::Robot,
            Species::Blob,
        ] {
            for md in [
                Mood::Happy,
                Mood::Dead,
                Mood::Dizzy,
                Mood::Sleepy,
                Mood::Alert,
                Mood::Starting,
            ] {
                assert!(!face_for(sp, md).is_empty());
            }
        }
    }

    #[test]
    fn speak_avoids_repeat_when_pool_has_two() {
        let mut a = Agent::new(Species::Goblin);
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        a.last_line = Some("uh, that service just died".into());
        let line = a.speak(Mood::Dead, &mut rng);
        assert_ne!(line, "uh, that service just died");
    }

    #[test]
    fn species_parse_defaults_to_goblin() {
        assert_eq!(Species::parse("nope"), Species::Goblin);
        assert_eq!(Species::parse("cat"), Species::Cat);
    }

    #[test]
    fn format_line_subs_name() {
        assert_eq!(format_line("{name} died", "api"), "api died");
    }
}
