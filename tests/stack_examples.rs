//! every sample config in examples/ must parse clean. if you add a new one,
//! add it to the list below. catches regressions in config schema.

use std::fs;
use std::path::PathBuf;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples")
}

fn parse_example(name: &str) {
    let p = examples_dir().join(name);
    let raw = fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {name}: {e}"));
    pulse::config::parse(&raw).unwrap_or_else(|e| panic!("parse {name}: {e}"));
}

#[test]
fn next_postgres_redis_parses() {
    parse_example("next-postgres-redis.toml");
}

#[test]
fn django_celery_parses() {
    parse_example("django-celery.toml");
}

#[test]
fn rails_sidekiq_parses() {
    parse_example("rails-sidekiq.toml");
}

#[test]
fn rust_api_docker_parses() {
    parse_example("rust-api-docker.toml");
}

#[test]
fn default_example_parses() {
    parse_example("pulse.toml");
}
