//! Agent output format for LLM coding agent consumption.
//!
//! See TIA-AGENT-001 through TIA-AGENT-007.
//!
//! Produces structured JSON with selection, per-test reasons, confidence,
//! changed units, summary stats, exclusion reasons for skipped tests,
//! and coverage gap surfacing.

use crate::engine::Selection;
use crate::store::Store;

/// Agent-format output (TIA-AGENT-001).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentOutput {
    /// Protocol version identifier.
    pub format: String,
    /// Summary statistics.
    pub summary: SummaryStats,
    /// Content units that changed (from the change set).
    pub changed_units: Vec<ChangedUnit>,
    /// Selected tests with reason chains.
    pub selected: Vec<SelectedTestInfo>,
    /// Tests that were candidates but not selected, with exclusion reasons.
    #[serde(default)]
    pub skipped: Vec<SkippedTestInfo>,
    /// Symbols that changed but have no covering test (TIA-AGENT-006).
    #[serde(default)]
    pub coverage_gaps: Vec<CoverageGap>,
}

/// Summary statistics for the agent output.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SummaryStats {
    /// Number of changed content units.
    pub changed_count: usize,
    /// Number of selected tests.
    pub selected_count: usize,
    /// Number of candidate tests (all tests that could be affected).
    pub candidate_count: usize,
    /// Whether any coverage gaps were detected.
    pub has_coverage_gaps: bool,
}

/// A content unit that changed in the selection.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChangedUnit {
    /// Store ID of the content unit.
    pub id: u32,
    /// File path relative to project root.
    pub path: String,
    /// Symbol name (if applicable).
    pub symbol: Option<String>,
    /// Content unit kind (source, config, etc.).
    pub kind: String,
    /// Whether the unit was unresolved (cold-start or missing file).
    pub unresolved: bool,
}

/// A selected test with its reason chain.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SelectedTestInfo {
    /// Store ID of the test item.
    pub id: u32,
    /// Adapter-assigned node ID (human-readable).
    pub node_id: Option<String>,
    /// Confidence in the selection decision (0.0–1.0).
    pub confidence: f64,
    /// Minimum distance from a changed unit (hops).
    pub distance: Option<u32>,
    /// Whether this test is in the always-run set.
    pub always_run: bool,
    /// Reason chain: witness edges explaining why this test was selected.
    pub reason_chain: Vec<ReasonEdge>,
}

/// A step in the reason chain for a test selection.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReasonEdge {
    /// Content unit ID this edge points to.
    pub content_unit_id: u32,
    /// Origin of the dependency edge.
    pub origin: String,
    /// Path of the content unit (resolved from store).
    pub path: Option<String>,
}

/// A test that was a candidate but not selected.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SkippedTestInfo {
    /// Store ID of the test item.
    pub id: u32,
    /// Adapter-assigned node ID.
    pub node_id: Option<String>,
    /// Human-readable reason for exclusion.
    pub exclusion_reason: String,
}

/// A coverage gap: a changed symbol with no covering test.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CoverageGap {
    /// Symbol name that changed.
    pub symbol: String,
    /// File path where the symbol is defined.
    pub file: String,
    /// Content unit ID.
    pub changed_unit_id: u32,
}

impl AgentOutput {
    /// Build the agent output from a selection result and store context.
    pub fn from_selection(
        store: &Store,
        selection: &Selection,
        changed_units: &[ChangedUnit],
        test_node_ids: &std::collections::HashMap<u32, String>,
        candidate_ids: &[u32],
    ) -> miette::Result<Self> {
        let selected_count = selection.tests.len();
        let candidate_count = candidate_ids.len();

        // Build selected tests with reason chains
        let selected: Vec<SelectedTestInfo> = selection
            .tests
            .iter()
            .map(|t| {
                let reason_chain = t
                    .witness
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(|w| ReasonEdge {
                        content_unit_id: w.content_unit,
                        origin: format!("{:?}", w.origin).to_lowercase(),
                        path: None, // resolved below
                    })
                    .collect();

                SelectedTestInfo {
                    id: t.id,
                    node_id: test_node_ids.get(&t.id).cloned(),
                    confidence: t.confidence,
                    distance: t.distance,
                    always_run: t.confidence >= 1.0 && t.distance.is_none(),
                    reason_chain,
                }
            })
            .collect();

        // Resolve paths for reason chain edges
        let mut output = Self {
            format: "testaruda-agent-v1".to_string(),
            summary: SummaryStats {
                changed_count: changed_units.len(),
                selected_count,
                candidate_count,
                has_coverage_gaps: false,
            },
            changed_units: changed_units.to_vec(),
            selected,
            skipped: Vec::new(),
            coverage_gaps: Vec::new(),
        };

        // Resolve paths for reason chain edges
        for sel in &mut output.selected {
            for edge in &mut sel.reason_chain {
                edge.path = store.get_content_unit_path(edge.content_unit_id).ok();
            }
        }

        // Compute skipped tests: candidates not in selected set
        let selected_ids: std::collections::HashSet<u32> =
            selection.tests.iter().map(|t| t.id).collect();
        for &cid in candidate_ids {
            if !selected_ids.contains(&cid) {
                output.skipped.push(SkippedTestInfo {
                    id: cid,
                    node_id: test_node_ids.get(&cid).cloned(),
                    exclusion_reason: "not in transitive closure of change set".to_string(),
                });
            }
        }

        // Compute coverage gaps (TIA-AGENT-006)
        // A gap exists when a changed symbol has no test that depends on it
        let changed_symbols: Vec<&ChangedUnit> = changed_units
            .iter()
            .filter(|cu| cu.symbol.is_some())
            .collect();

        for cu in &changed_symbols {
            // Check if any test depends on this content unit
            let has_coverage = store.has_test_for_content_unit(cu.id)?;
            if !has_coverage {
                output.coverage_gaps.push(CoverageGap {
                    symbol: cu.symbol.clone().unwrap_or_default(),
                    file: cu.path.clone(),
                    changed_unit_id: cu.id,
                });
            }
        }

        output.summary.has_coverage_gaps = !output.coverage_gaps.is_empty();

        Ok(output)
    }

    /// Build a specific-test query response (TIA-AGENT-004).
    pub fn test_query(
        test_id: u32,
        selection: &Selection,
        test_node_ids: &std::collections::HashMap<u32, String>,
    ) -> serde_json::Value {
        let selected = selection.tests.iter().find(|t| t.id == test_id);

        match selected {
            Some(t) => {
                let chain: Vec<serde_json::Value> = t
                    .witness
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(|w| {
                        serde_json::json!({
                            "content_unit": w.content_unit,
                            "origin": format!("{:?}", w.origin).to_lowercase(),
                        })
                    })
                    .collect();

                serde_json::json!({
                    "test_id": test_id,
                    "node_id": test_node_ids.get(&test_id),
                    "selected": true,
                    "confidence": t.confidence,
                    "distance": t.distance,
                    "reason_chain": chain,
                    "always_run": t.confidence >= 1.0 && t.distance.is_none(),
                })
            }
            None => {
                // Test was not selected — find exclusion reason
                serde_json::json!({
                    "test_id": test_id,
                    "node_id": test_node_ids.get(&test_id),
                    "selected": false,
                    "exclusion_reason": "not in affected set of current change",
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{Origin, SelectedTest, WitnessEdge};

    #[test]
    fn test_agent_output_format_serialization() {
        let output = AgentOutput {
            format: "testaruda-agent-v1".to_string(),
            summary: SummaryStats {
                changed_count: 2,
                selected_count: 1,
                candidate_count: 3,
                has_coverage_gaps: false,
            },
            changed_units: vec![
                ChangedUnit {
                    id: 1,
                    path: "src/lib.rs".to_string(),
                    symbol: None,
                    kind: "source".to_string(),
                    unresolved: false,
                },
                ChangedUnit {
                    id: 2,
                    path: "src/main.rs".to_string(),
                    symbol: Some("run".to_string()),
                    kind: "source".to_string(),
                    unresolved: false,
                },
            ],
            selected: vec![SelectedTestInfo {
                id: 10,
                node_id: Some("my_module::test_foo(Test)".to_string()),
                confidence: 1.0,
                distance: Some(0),
                always_run: false,
                reason_chain: vec![ReasonEdge {
                    content_unit_id: 1,
                    origin: "static".to_string(),
                    path: Some("src/lib.rs".to_string()),
                }],
            }],
            skipped: vec![SkippedTestInfo {
                id: 11,
                node_id: Some("my_module::test_bar(Test)".to_string()),
                exclusion_reason: "not in transitive closure of change set".to_string(),
            }],
            coverage_gaps: Vec::new(),
        };

        let json = serde_json::to_string_pretty(&output).unwrap();
        assert!(json.contains("\"format\": \"testaruda-agent-v1\""));
        assert!(json.contains("\"selected_count\": 1"));
        assert!(json.contains("\"candidate_count\": 3"));
        assert!(json.contains("\"reason_chain\""));
        assert!(json.contains("\"exclusion_reason\""));
    }

    #[test]
    fn test_test_query_selected() {
        let sel = Selection {
            changed_count: 1,
            selected_count: 1,
            tests: vec![SelectedTest {
                id: 42,
                confidence: 0.95,
                distance: Some(1),
                witness: Some(vec![WitnessEdge {
                    content_unit: 5,
                    origin: Origin::Runtime,
                }]),
            }],
        };
        let mut node_ids = std::collections::HashMap::new();
        node_ids.insert(42, "mod::test(Test)".to_string());

        let result = AgentOutput::test_query(42, &sel, &node_ids);
        assert_eq!(result["selected"], true);
        assert_eq!(result["test_id"], 42);
        assert_eq!(result["confidence"], 0.95);
        assert!(result["reason_chain"].as_array().unwrap().len() > 0);
    }

    #[test]
    fn test_test_query_not_selected() {
        let sel = Selection {
            changed_count: 0,
            selected_count: 0,
            tests: Vec::new(),
        };
        let node_ids = std::collections::HashMap::new();

        let result = AgentOutput::test_query(99, &sel, &node_ids);
        assert_eq!(result["selected"], false);
        assert_eq!(result["test_id"], 99);
        assert!(result["exclusion_reason"].as_str().is_some());
    }

    #[test]
    fn test_coverage_gap_serialization() {
        let gap = CoverageGap {
            symbol: "uncovered_fn".to_string(),
            file: "src/uncovered.rs".to_string(),
            changed_unit_id: 7,
        };
        let json = serde_json::to_string(&gap).unwrap();
        assert!(json.contains("\"symbol\":\"uncovered_fn\""));
        assert!(json.contains("\"changed_unit_id\":7"));
    }

    #[test]
    fn test_empty_agent_output_fields() {
        let output = AgentOutput {
            format: "testaruda-agent-v1".to_string(),
            summary: SummaryStats {
                changed_count: 0,
                selected_count: 0,
                candidate_count: 0,
                has_coverage_gaps: false,
            },
            changed_units: Vec::new(),
            selected: Vec::new(),
            skipped: Vec::new(),
            coverage_gaps: Vec::new(),
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"skipped\":[]"));
        assert!(json.contains("\"coverage_gaps\":[]"));
        assert!(json.contains("\"changed_units\":[]"));
        assert!(json.contains("\"selected\":[]"));
    }
}