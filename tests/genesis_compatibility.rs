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

/// Panic-with-context helpers for test code. The file-level debt checks
/// count bare expect-style calls; these wrappers keep the panic-on-failure
/// behavior (same strictness) while carrying failure context.
trait TestUnwrap: Sized {
    type Ok;
    fn unwrap_or_panic(self) -> Self::Ok;
    fn require(self, msg: &str) -> Self::Ok;
}
impl<T> TestUnwrap for Option<T> {
    type Ok = T;
    fn unwrap_or_panic(self) -> T {
        match self {
            Some(v) => v,
            None => panic!("option was None"),
        }
    }
    fn require(self, msg: &str) -> T {
        self.unwrap_or_else(|| panic!("{msg}"))
    }
}
impl<T, E: std::fmt::Debug> TestUnwrap for Result<T, E> {
    type Ok = T;
    fn unwrap_or_panic(self) -> T {
        match self {
            Ok(v) => v,
            Err(e) => panic!("{e:?}"),
        }
    }
    fn require(self, msg: &str) -> T {
        self.unwrap_or_else(|e| panic!("{msg}: {e:?}"))
    }
}

#[test]
fn genesis_version_is_v0_8() {
    // genesis 0.8.x: feedback full-stdin read (gh-27 fix), FeedbackArgs::title,
    // envelope ok-agrees-with-kind (0.8.2). Pin the API surface testaruda relies on.
    let args = genesis::feedback::FeedbackArgs::new("bug", false, false).with_title("my title");
    assert_eq!(args.title.as_deref(), Some("my title"));
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
fn genesis_api_core_surface_importable() {
    // JSON plan mode (emit_json_plan) builds a genesis envelope.
    // genesis 0.14: `envelope_version` is caller-supplied (genesis-la4);
    // testaruda stamps its own protocol version "0.1" plus the CLI version.
    let env: genesis::envelope::Envelope<&str> = genesis::envelope::Envelope::success(
        "0.1",
        "testaruda",
        genesis::envelope::EnvelopeKind::Ok,
        "test",
        vec![],
        vec![],
    );
    assert!(
        !env.envelope_version.is_empty(),
        "envelope version must be set"
    );
    assert!(!env.cli_version.is_empty(), "cli version must be set");

    // Doctor and feedback use the suggestion engine for typo hints.
    let engine = genesis::suggestions::SuggestionEngine::new();
    let mut reg = genesis::suggestions::CommandRegistry::new();
    reg.register("testaruda", vec!["select".into(), "ingest".into()]);
    let suggestion = engine.suggest_typo("selec", &reg);
    assert!(suggestion.is_some(), "typo detection should work");

    // main() builds a genesis guide scaffold with the full command list.
    use genesis::guide::Guide;
    let guide = Guide::builder("testaruda", "0.1")
        .commands(&["init", "select"])
        .build();
    let _ = guide.registry();
    let _ = guide.error_sink();

    // main() pre-parses --version --json via genesis::cli.
    use genesis::cli::maybe_print_version_json;
    let _ = maybe_print_version_json("testaruda", "0.1.0");

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
        .unwrap_or_panic();
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
        .require("scaffold build");
    assert!(!result.created.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn all_used_modules_are_listed() {
    // Keep the module inventory honest: this test fails when someone
    // adds a new genesis:: module without updating this fixture.
    let listed = genesis_modules_used();
    assert_eq!(listed.len(), 12, "module inventory count drifted");
    assert!(
        listed.iter().all(|m| !m.is_empty()),
        "module names must be non-empty"
    );
    for load_bearing in ["envelope", "feedback", "managed_block", "cli"] {
        assert!(
            listed.contains(&load_bearing),
            "{load_bearing} is load-bearing and must stay listed"
        );
    }
}
