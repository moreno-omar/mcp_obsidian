//! Behaviour of the vault layer on its own: indexing, and the four queries.
//!
//! These tests exist so the module split in `src/vault/` can be re-checked
//! after any refactor: they exercise the public API exactly as the CLI and the
//! MCP tools do, against the fixture in `samples/vault`.

use mcp_obsidian::vault::{Direction, Hit, Note, Vault};

/// The fixture vault, addressed the same way the CLI addresses it.
fn sample_vault() -> Vault {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/samples/vault");
    Vault::load(path).expect("sample vault should load")
}

/// Note paths of a hit list, in ranked order.
fn paths(vault: &Vault, hits: &[Hit]) -> Vec<String> {
    hits.iter()
        .map(|hit| vault.notes()[hit.index].rel_path.clone())
        .collect()
}

/// Notes found by a tag query.
fn tagged(vault: &Vault, tags: &[&str], match_all: bool) -> Vec<String> {
    let tags: Vec<String> = tags.iter().map(|tag| tag.to_string()).collect();
    paths(vault, &vault.search_by_tag(&tags, match_all, 10))
}

/// Notes found by a term query.
fn terms(vault: &Vault, terms: &[&str], match_all: bool) -> Vec<String> {
    let terms: Vec<String> = terms.iter().map(|term| term.to_string()).collect();
    paths(vault, &vault.search_by_term(&terms, match_all, 10))
}

/// Notes found by a graph walk.
fn linked(vault: &Vault, note: &str, depth: usize, direction: Direction) -> Vec<String> {
    let hits = vault
        .get_linked_notes(note, depth, direction, 10)
        .unwrap_or_else(|error| panic!("{note} should resolve: {error}"));
    paths(vault, &hits)
}

fn note<'v>(vault: &'v Vault, name: &str) -> &'v Note {
    vault
        .get_note(name)
        .unwrap_or_else(|error| panic!("{name} should resolve: {error}"))
}

// ---------------------------------------------------------------------------
// Indexing
// ---------------------------------------------------------------------------

#[test]
fn index_reports_the_expected_shape() {
    let vault = sample_vault();
    let stats = vault.stats();

    assert_eq!(stats.notes, 4);
    assert_eq!(stats.tags, 11);
    assert_eq!(stats.headings, 12);
    assert_eq!(stats.links, 10);
    assert_eq!(stats.unresolved_links, 0);
    assert!(vault.unresolved_links().is_empty());
}

#[test]
fn index_is_sorted_by_path_for_reproducible_output() {
    let vault = sample_vault();
    let paths: Vec<&str> = vault.notes().iter().map(|n| n.rel_path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "index.md",
            "notes/retrieval-design.md",
            "projects/alpha.md",
            "projects/beta-note.md",
        ]
    );
    assert!(vault.root().is_absolute());
}

#[test]
fn index_collects_frontmatter_body_tags_headings_and_links() {
    let vault = sample_vault();
    let index = note(&vault, "index");
    let retrieval = note(&vault, "retrieval-design");

    // Frontmatter `tags:` list plus an inline `#tag` from the body.
    assert!(index.tags.contains("moc"));
    assert!(retrieval.tags.contains("okf"));
    assert!(retrieval.tags.contains("markdown"));
    // Nested tag from a frontmatter list.
    assert!(note(&vault, "alpha").tags.contains("project/alpha"));
    // Headings, in document order.
    assert_eq!(index.headings, vec!["Vault Index", "Notes"]);
    // Wikilink targets de-duplicated; `[[x#Section]]` counts as `x`.
    assert_eq!(
        index.links.iter().cloned().collect::<Vec<_>>(),
        vec!["alpha", "beta-note", "retrieval-design"]
    );
}

#[test]
fn index_renders_a_stable_one_line_per_note_listing() {
    let vault = sample_vault();

    let rendered = vault.render_index(0);
    assert!(rendered.starts_with("vault: "));
    assert!(rendered.contains("notes: 4 | tags: 11 | headings: 12 | links: 10 | unresolved links: 0"));
    assert!(rendered.contains("index.md"));
    assert_eq!(rendered.trim_end(), rendered);

    let truncated = vault.render_index(2);
    assert!(truncated.contains("… 2 more notes"));
    assert!(!truncated.contains("projects/alpha.md"));
}

// ---------------------------------------------------------------------------
// Tag search
// ---------------------------------------------------------------------------

#[test]
fn tag_search_also_matches_nested_tags() {
    let vault = sample_vault();

    assert_eq!(
        tagged(&vault, &["project"], false),
        vec!["projects/alpha.md", "projects/beta-note.md"]
    );
    assert_eq!(tagged(&vault, &["project/alpha"], false), vec!["projects/alpha.md"]);
    assert_eq!(tagged(&vault, &["moc"], false), vec!["index.md"]);
    assert!(tagged(&vault, &["nope"], false).is_empty());
}

#[test]
fn tag_search_normalises_the_query() {
    let vault = sample_vault();
    // Leading `#`, case and inner whitespace are all normalised away.
    assert_eq!(tagged(&vault, &["#Project"], false), tagged(&vault, &["project"], false));
    assert!(tagged(&vault, &["   "], false).is_empty());
}

#[test]
fn tag_search_requires_every_tag_when_match_all() {
    let vault = sample_vault();

    assert_eq!(
        tagged(&vault, &["project", "active"], true),
        vec!["projects/alpha.md"]
    );
    assert!(tagged(&vault, &["project", "moc"], true).is_empty());
    // Without `match_all`, either tag is enough.
    assert_eq!(
        tagged(&vault, &["project", "moc"], false),
        vec![
            "index.md",
            "projects/alpha.md",
            "projects/beta-note.md"
        ]
    );
}

#[test]
fn tag_search_reports_why_each_note_matched() {
    let vault = sample_vault();
    let hits = vault.search_by_tag(&["project".to_string()], false, 10);

    assert!(hits.iter().all(|hit| hit.reason == "tag match: project"));
    assert!(hits.iter().all(|hit| hit.score == 10));

    let multi = vault.search_by_tag(&["project".to_string(), "active".to_string()], true, 10);
    assert_eq!(multi[0].reason, "tag match: project, active");
    assert_eq!(multi[0].score, 20);
}

// ---------------------------------------------------------------------------
// Term search
// ---------------------------------------------------------------------------

#[test]
fn term_search_covers_title_tags_headings_frontmatter_and_body() {
    let vault = sample_vault();

    assert_eq!(
        terms(&vault, &["embeddings"], false),
        vec!["notes/retrieval-design.md", "index.md"] // heading beats body
    );
    assert_eq!(terms(&vault, &["backlog"], false), vec!["projects/beta-note.md"]); // tag
    assert_eq!(terms(&vault, &["in-progress"], false), vec!["projects/alpha.md"]); // frontmatter
    assert_eq!(terms(&vault, &["unknownword"], false), Vec::<String>::new());
}

#[test]
fn term_search_ranks_stronger_fields_higher() {
    let vault = sample_vault();
    // "Token budget" is a heading of retrieval-design (3+3) and appears in the
    // body of index.md (1+1), so the heading match wins.
    assert_eq!(
        terms(&vault, &["token", "budget"], false),
        vec!["notes/retrieval-design.md", "index.md"]
    );
}

#[test]
fn term_search_match_all_drops_partial_matches() {
    let vault = sample_vault();

    assert_eq!(
        terms(&vault, &["token", "budget"], true),
        vec!["notes/retrieval-design.md", "index.md"]
    );
    assert!(terms(&vault, &["backlog", "moc"], true).is_empty());
    assert!(terms(&vault, &[""], false).is_empty());
}

#[test]
fn search_limit_caps_the_result_count() {
    let vault = sample_vault();
    let all = vault.search_by_tag(&["project".to_string()], false, 0);
    let one = vault.search_by_tag(&["project".to_string()], false, 1);

    assert_eq!(all.len(), 2);
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].index, all[0].index);
}

// ---------------------------------------------------------------------------
// Graph search
// ---------------------------------------------------------------------------

#[test]
fn graph_search_walks_outgoing_links_and_backlinks() {
    let vault = sample_vault();

    assert_eq!(
        linked(&vault, "index", 1, Direction::Outgoing),
        vec![
            "notes/retrieval-design.md",
            "projects/alpha.md",
            "projects/beta-note.md"
        ]
    );
    assert_eq!(
        linked(&vault, "projects/alpha", 1, Direction::Outgoing),
        vec!["index.md", "notes/retrieval-design.md"]
    );
    assert_eq!(
        linked(&vault, "alpha", 1, Direction::Backlinks),
        vec![
            "index.md",
            "notes/retrieval-design.md",
            "projects/beta-note.md"
        ]
    );
}

#[test]
fn graph_search_merges_both_directions_without_duplicates() {
    let vault = sample_vault();
    let both = linked(&vault, "index", 1, Direction::Both);

    assert_eq!(both.len(), 3);
    let mut deduped = both.clone();
    deduped.sort();
    deduped.dedup();
    assert_eq!(deduped, both);
}

#[test]
fn graph_search_is_bounded_by_depth_and_reports_the_hop() {
    let vault = sample_vault();

    // Alpha links to index and retrieval-design; beta-note is only reachable
    // from index, so it appears at hop 2 with a halved score.
    let one_hop = vault
        .get_linked_notes("alpha", 1, Direction::Outgoing, 10)
        .unwrap();
    let two_hops = vault
        .get_linked_notes("alpha", 2, Direction::Outgoing, 10)
        .unwrap();

    assert_eq!(
        paths(&vault, &one_hop),
        vec!["index.md", "notes/retrieval-design.md"]
    );
    assert!(one_hop.iter().all(|hit| hit.score == 10));
    assert!(one_hop.iter().all(|hit| hit.reason.ends_with("at hop 1")));

    assert_eq!(two_hops.len(), 3);
    assert!(
        two_hops
            .iter()
            .any(|hit| hit.reason == "link of 'index.md' at hop 2" && hit.score == 5)
    );
    assert!(two_hops.iter().all(|hit| hit.reason.starts_with("link of '")));
}

#[test]
fn graph_search_rejects_an_unknown_note() {
    let vault = sample_vault();
    let error = vault
        .get_linked_notes("does-not-exist", 1, Direction::Both, 10)
        .unwrap_err();
    assert!(error.to_string().contains("no note matches"));
}

#[test]
fn direction_parses_the_cli_strings() {
    for spelling in ["out", "outgoing", "links", "OUTGOING"] {
        assert_eq!(spelling.parse::<Direction>().unwrap(), Direction::Outgoing);
    }
    for spelling in ["in", "back", "backlinks"] {
        assert_eq!(spelling.parse::<Direction>().unwrap(), Direction::Backlinks);
    }
    for spelling in ["both", "all"] {
        assert_eq!(spelling.parse::<Direction>().unwrap(), Direction::Both);
    }
    assert!("sideways".parse::<Direction>().is_err());
}

// ---------------------------------------------------------------------------
// Note lookup: every way a note can be addressed
// ---------------------------------------------------------------------------

#[test]
fn note_lookup_accepts_every_address_type() {
    let vault = sample_vault();

    let cases = [
        ("projects/alpha", "projects/alpha.md"),           // vault-relative path
        ("projects/alpha.md", "projects/alpha.md"),        // path with extension
        ("/projects/alpha", "projects/alpha.md"),          // leading slash
        ("alpha", "projects/alpha.md"),                    // file stem
        ("ALPHA", "projects/alpha.md"),                    // case insensitive
        ("Alpha", "projects/alpha.md"),                    // frontmatter title
        ("home", "index.md"),                              // frontmatter alias
        ("Home", "index.md"),                              // alias, any case
        ("notes/retrieval-design.md", "notes/retrieval-design.md"),
        (r"notes\retrieval-design", "notes/retrieval-design.md"), // windows separators
        ("retrieval-design#Token budget", "notes/retrieval-design.md"), // heading suffix
        ("retrieval-design|Design", "notes/retrieval-design.md"),       // wikilink alias
        ("beta", "projects/beta-note.md"),                 // fuzzy substring
    ];

    for (address, expected) in cases {
        assert_eq!(note(&vault, address).rel_path, expected, "address: {address}");
    }
}

#[test]
fn note_lookup_reports_unknown_and_ambiguous_names() {
    let vault = sample_vault();

    let unknown = vault.get_note("does-not-exist").unwrap_err();
    assert!(unknown.to_string().contains("no note matches"));

    // "project" is only a partial match for two paths, so it must not guess.
    let ambiguous = vault.get_note("project").unwrap_err();
    let message = ambiguous.to_string();
    assert!(message.contains("ambiguous"));
    assert!(message.contains("projects/alpha.md"));
    assert!(message.contains("projects/beta-note.md"));
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

#[test]
fn render_note_keeps_the_metadata_header_and_truncates_the_body() {
    let vault = sample_vault();
    let index = note(&vault, "home");

    let rendered = vault.render_note(index, 40);
    assert!(rendered.starts_with("# Vault Index\n"));
    assert!(rendered.contains("- path: `index.md`"));
    assert!(rendered.contains("- tags: #index, #moc"));
    assert!(rendered.contains("- headings: Vault Index | Notes"));
    assert!(rendered.ends_with("…[truncated]"));

    // A zero budget means "do not truncate".
    let full = vault.render_note(index, 0);
    assert!(!full.contains("…[truncated]"));
    assert!(full.contains("Entry point for this sample vault."));
}

#[test]
fn render_hits_explains_each_match_and_survives_an_empty_result() {
    let vault = sample_vault();
    let hits = vault.search_by_tag(&["project".to_string()], false, 10);
    let rendered = vault.render_hits(&hits, 120);

    assert!(rendered.contains("## Alpha"));
    assert!(rendered.contains("- score: 10 (tag match: project)"));
    assert!(rendered.contains("- backlinks: "));
    assert_eq!(
        vault.render_hits(&[], 120),
        "No notes matched the query."
    );
}

#[test]
fn identical_queries_produce_identical_output() {
    let vault = sample_vault();
    let first = vault.render_hits(&vault.search_by_term(&["project".to_string()], false, 10), 200);
    let second = vault.render_hits(&vault.search_by_term(&["project".to_string()], false, 10), 200);

    assert_eq!(first, second);
}
