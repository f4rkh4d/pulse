use pulse::discover::{parse_compose, parse_package_json, parse_procfile, render_draft};

#[test]
fn package_json_picks_watch_scripts() {
    let raw = r#"{
  "scripts": {
    "dev": "next dev",
    "build": "next build",
    "watch": "vite",
    "test:watch": "jest --watch"
  }
}"#;
    let s = parse_package_json(raw);
    let names: Vec<_> = s.iter().map(|x| x.name.as_str()).collect();
    assert!(names.contains(&"dev"));
    assert!(names.contains(&"watch"));
    assert!(names.contains(&"test:watch"));
    assert!(!names.contains(&"build"));
}

#[test]
fn malformed_package_json_is_ignored() {
    assert!(parse_package_json("not json").is_empty());
}

#[test]
fn compose_reads_service_names() {
    let raw = r#"
version: "3"
services:
  web:
    image: nginx
  db:
    image: postgres:16
    ports:
      - "5432:5432"
"#;
    let s = parse_compose(raw);
    let names: Vec<_> = s.iter().map(|x| x.name.clone()).collect();
    assert!(names.contains(&"web".to_string()));
    assert!(names.contains(&"db".to_string()));
}

#[test]
fn procfile_skips_blank_and_comments() {
    let raw = "# comment\n\nweb: puma\nworker: sidekiq\n";
    let s = parse_procfile(raw);
    assert_eq!(s.len(), 2);
}

#[test]
fn rendered_draft_roundtrips_through_config() {
    let sugg = parse_procfile("api: cargo run\nweb: npm run dev\n");
    let draft = render_draft(&sugg);
    let cfg = pulse::config::parse(&draft).unwrap();
    assert_eq!(cfg.services.len(), 2);
}

#[test]
fn empty_draft_still_informative() {
    let out = render_draft(&[]);
    assert!(out.contains("nothing found"));
}
