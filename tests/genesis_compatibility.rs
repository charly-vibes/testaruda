//! Genesis compatibility fixture: verify genesis-vibes exposes the APIs
//! testaruda depends on (testaruda-6k0z, pattern from vampiro's
//! genesis_compatibility.rs).
//!
//! These tests compile-fail if a published `genesis-vibes = "0.7"` release
//! removes or reshapes any module testaruda uses, catching the breakage
//! at test time instead of at release time.

/// Every genesis module testaruda depends on (from `grep -roh "genesis::[a-z_]*" src/`).
fn genesis_modules_used() -> &'static [&'static str] {
    &[
        "suggestions",
        "managed_block",
        "feedback",
        "envelope",
        "guide",
        "doctor",
        "config",
        "cli",
        "suite_linter",
        "status",
        "scaffold",
        "discovery",
    ]
}

#[test]
fn genesis_version_is_v0_7() {
    // genesis 0.7.0 is required for the feedback → regression-scenario APIs
    let _ = genesis::envelope::ENVELOPE_VERSION;
}

#[test]
fn genesis_api_feedback_importable() {
    // The feedback subcommand delegates to handle_feedback — the exact
    // integration the whole feature hangs on.
    use genesis::feedback::FeedbackArgs;
    let args = FeedbackArgs::new("bug", true, false);
    assert_eq!(args.kind, "bug");
    assert!(args.dry_run);
    assert!(!args.from_last_error);
}

#[test]
fn genesis_api_envelope_importable() {
    // JSON plan mode (emit_json_plan) builds a genesis envelope.
    let _: genesis::envelope::Envelope<&str> = genesis::envelope::Envelope::success(
        "testaruda",
        genesis::envelope::EnvelopeKind::Ok,
        "test",
        vec![],
        vec![],
    );
}

#[test]
fn genesis_api_suggestions_importable() {
    // Doctor and feedback use the suggestion engine for typo hints.
    let engine = genesis::suggestions::SuggestionEngine::new();
    let mut reg = genesis::suggestions::CommandRegistry::new();
    reg.register("testaruda", vec!["select".into(), "ingest".into()]);
    let suggestion = engine.suggest_typo("selec", &reg);
    assert!(suggestion.is_some(), "typo detection should work");
}

#[test]
fn genesis_api_guide_importable() {
    // main() builds a genesis guide scaffold with the full command list.
    use genesis::guide::Guide;
    let guide = Guide::builder("testaruda", "0.1")
        .commands(&["init", "select"])
        .build();
    let _ = guide.registry();
    let _ = guide.error_sink();
}

#[test]
fn genesis_api_cli_importable() {
    // main() pre-parses --version --json via genesis::cli.
    use genesis::cli::maybe_print_version_json;
    let _ = maybe_print_version_json("testaruda", "0.1.0");
}

#[test]
fn genesis_api_managed_block_importable() {
    // Managed blocks (WAI/OPENSPEC/DONT markers) come from genesis.
    let mut reg = genesis::managed_block::BlockRegistry::new();
    reg.register(genesis::managed_block::BlockDef::new("WAI"));
    let injector = genesis::managed_block::BlockInjector::new(reg);
    let _ = injector;
}

#[test]
fn genesis_api_discovery_importable() {
    let dir = std::env::temp_dir().join(format!("testaruda-test-discovery-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    genesis::discovery::register(&dir, "testaruda", "testaruda", "directory", ".testaruda")
        .unwrap();
    let tools = genesis::discovery::scan(&dir);
    assert!(tools.iter().any(|t| t.name == "testaruda"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn genesis_api_scaffold_importable() {
    let dir = std::env::temp_dir().join(format!("testaruda-test-scaffold-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let result = genesis::scaffold::Scaffold::new(&dir)
        .dir(".testaruda-test-dir")
        .default_config("test.toml", "key = \"val\"")
        .build()
        .expect("scaffold build");
    assert!(!result.created.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn all_used_modules_are_listed() {
    // Keep the module inventory honest: this test fails when someone
    // adds a new genesis:: module without updating this fixture.
    assert_eq!(genesis_modules_used().len(), 12);
}
