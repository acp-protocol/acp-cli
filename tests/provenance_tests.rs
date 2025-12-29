//! RFC-0003/RFC-0005: Provenance tracking tests
//!
//! Tests for annotation provenance parsing, caching, and command functionality.

use acp::cache::{
    AnnotationProvenance, LowConfidenceEntry, ProvenanceStats, ProvenanceSummary, SourceCounts,
};
use acp::commands::ConfidenceFilter;
use acp::parse::SourceOrigin;

// =============================================================================
// T7.1: Parser Tests - SourceOrigin
// =============================================================================

mod parser_tests {
    use super::*;

    #[test]
    fn test_source_origin_from_str() {
        assert_eq!(
            "explicit".parse::<SourceOrigin>().unwrap(),
            SourceOrigin::Explicit
        );
        assert_eq!(
            "converted".parse::<SourceOrigin>().unwrap(),
            SourceOrigin::Converted
        );
        assert_eq!(
            "heuristic".parse::<SourceOrigin>().unwrap(),
            SourceOrigin::Heuristic
        );
        assert_eq!(
            "refined".parse::<SourceOrigin>().unwrap(),
            SourceOrigin::Refined
        );
        assert_eq!(
            "inferred".parse::<SourceOrigin>().unwrap(),
            SourceOrigin::Inferred
        );

        // Case insensitive
        assert_eq!(
            "EXPLICIT".parse::<SourceOrigin>().unwrap(),
            SourceOrigin::Explicit
        );
        assert_eq!(
            "Heuristic".parse::<SourceOrigin>().unwrap(),
            SourceOrigin::Heuristic
        );

        // Invalid
        assert!("invalid".parse::<SourceOrigin>().is_err());
    }

    #[test]
    fn test_source_origin_as_str() {
        assert_eq!(SourceOrigin::Explicit.as_str(), "explicit");
        assert_eq!(SourceOrigin::Converted.as_str(), "converted");
        assert_eq!(SourceOrigin::Heuristic.as_str(), "heuristic");
        assert_eq!(SourceOrigin::Refined.as_str(), "refined");
        assert_eq!(SourceOrigin::Inferred.as_str(), "inferred");
    }

    #[test]
    fn test_source_origin_default() {
        let origin: SourceOrigin = Default::default();
        assert_eq!(origin, SourceOrigin::Explicit);
    }

    #[test]
    fn test_source_origin_serialization() {
        let origin = SourceOrigin::Heuristic;
        let json = serde_json::to_string(&origin).unwrap();
        assert_eq!(json, "\"heuristic\"");

        let parsed: SourceOrigin = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, SourceOrigin::Heuristic);
    }

    #[test]
    fn test_source_origin_all_variants_serialize() {
        for origin in [
            SourceOrigin::Explicit,
            SourceOrigin::Converted,
            SourceOrigin::Heuristic,
            SourceOrigin::Refined,
            SourceOrigin::Inferred,
        ] {
            let json = serde_json::to_string(&origin).unwrap();
            let parsed: SourceOrigin = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, origin);
        }
    }
}

// =============================================================================
// T7.2: Cache Types Tests - AnnotationProvenance
// =============================================================================

mod cache_types_tests {
    use super::*;

    #[test]
    fn test_annotation_provenance_full_serialization() {
        let prov = AnnotationProvenance {
            value: "Test value".to_string(),
            source: SourceOrigin::Heuristic,
            confidence: Some(0.75),
            needs_review: true,
            reviewed: false,
            reviewed_at: None,
            generated_at: Some("2025-12-23T00:00:00Z".to_string()),
            generation_id: Some("gen-20251223-001".to_string()),
        };

        let json = serde_json::to_string_pretty(&prov).unwrap();
        assert!(json.contains("\"value\": \"Test value\""));
        assert!(json.contains("\"source\": \"heuristic\""));
        assert!(json.contains("\"confidence\": 0.75"));
        assert!(json.contains("\"needsReview\": true"));

        // Roundtrip
        let parsed: AnnotationProvenance = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.value, "Test value");
        assert_eq!(parsed.source, SourceOrigin::Heuristic);
        assert_eq!(parsed.confidence, Some(0.75));
        assert!(parsed.needs_review);
        assert!(!parsed.reviewed);
    }

    #[test]
    fn test_annotation_provenance_explicit_skips_source() {
        let prov = AnnotationProvenance {
            value: "Test".to_string(),
            source: SourceOrigin::Explicit,
            confidence: None,
            needs_review: false,
            reviewed: false,
            reviewed_at: None,
            generated_at: None,
            generation_id: None,
        };

        let json = serde_json::to_string(&prov).unwrap();
        // Explicit source should be skipped in serialization
        assert!(!json.contains("\"source\""));
    }

    #[test]
    fn test_annotation_provenance_minimal() {
        // Test with minimal fields (only required value)
        let json = r#"{"value": "minimal"}"#;
        let parsed: AnnotationProvenance = serde_json::from_str(json).unwrap();

        assert_eq!(parsed.value, "minimal");
        assert_eq!(parsed.source, SourceOrigin::Explicit); // Default
        assert_eq!(parsed.confidence, None);
        assert!(!parsed.needs_review);
        assert!(!parsed.reviewed);
    }

    #[test]
    fn test_annotation_provenance_with_review() {
        let prov = AnnotationProvenance {
            value: "Reviewed annotation".to_string(),
            source: SourceOrigin::Converted,
            confidence: Some(0.85),
            needs_review: false,
            reviewed: true,
            reviewed_at: Some("2025-12-23T12:00:00Z".to_string()),
            generated_at: Some("2025-12-23T00:00:00Z".to_string()),
            generation_id: Some("gen-20251223-001".to_string()),
        };

        let json = serde_json::to_string(&prov).unwrap();
        // reviewed:true should be serialized (not skipped)
        assert!(json.contains("reviewed"));
        assert!(json.contains("reviewedAt") || json.contains("reviewed_at"));

        let parsed: AnnotationProvenance = serde_json::from_str(&json).unwrap();
        assert!(parsed.reviewed);
        assert!(parsed.reviewed_at.is_some());
    }
}

// =============================================================================
// T7.2: Cache Types Tests - ProvenanceStats
// =============================================================================

mod provenance_stats_tests {
    use super::*;

    #[test]
    fn test_provenance_stats_default_is_empty() {
        let stats = ProvenanceStats::default();
        assert!(stats.is_empty());
        assert_eq!(stats.summary.total, 0);
    }

    #[test]
    fn test_provenance_stats_not_empty_when_has_data() {
        let mut stats = ProvenanceStats::default();
        stats.summary.total = 1;
        assert!(!stats.is_empty());
    }

    #[test]
    fn test_provenance_stats_serialization() {
        let mut stats = ProvenanceStats::default();
        stats.summary.total = 100;
        stats.summary.by_source.explicit = 50;
        stats.summary.by_source.heuristic = 30;
        stats.summary.by_source.converted = 20;
        stats.summary.needs_review = 10;
        stats.summary.reviewed = 90;

        let json = serde_json::to_string_pretty(&stats).unwrap();
        assert!(json.contains("\"total\": 100"));
        assert!(json.contains("\"explicit\": 50"));
        assert!(json.contains("\"heuristic\": 30"));
        assert!(json.contains("\"needsReview\": 10"));

        // Roundtrip
        let parsed: ProvenanceStats = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.summary.total, 100);
        assert_eq!(parsed.summary.by_source.explicit, 50);
        assert_eq!(parsed.summary.by_source.heuristic, 30);
        assert_eq!(parsed.summary.needs_review, 10);
        assert_eq!(parsed.summary.reviewed, 90);
    }

    #[test]
    fn test_source_counts_all_fields() {
        let counts = SourceCounts {
            explicit: 10,
            converted: 20,
            heuristic: 30,
            refined: 40,
            inferred: 50,
        };

        let json = serde_json::to_string(&counts).unwrap();
        let parsed: SourceCounts = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.explicit, 10);
        assert_eq!(parsed.converted, 20);
        assert_eq!(parsed.heuristic, 30);
        assert_eq!(parsed.refined, 40);
        assert_eq!(parsed.inferred, 50);
    }

    #[test]
    fn test_low_confidence_entry() {
        let entry = LowConfidenceEntry {
            target: "src/lib.rs:my_function".to_string(),
            annotation: "@acp:summary".to_string(),
            confidence: 0.45,
            value: "Process data".to_string(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let parsed: LowConfidenceEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.target, "src/lib.rs:my_function");
        assert_eq!(parsed.annotation, "@acp:summary");
        assert_eq!(parsed.confidence, 0.45);
        assert_eq!(parsed.value, "Process data");
    }

    #[test]
    fn test_provenance_stats_with_low_confidence() {
        let mut stats = ProvenanceStats::default();
        stats.summary.total = 10;
        stats.low_confidence.push(LowConfidenceEntry {
            target: "test.rs:func".to_string(),
            annotation: "@acp:summary".to_string(),
            confidence: 0.3,
            value: "Test".to_string(),
        });

        let json = serde_json::to_string_pretty(&stats).unwrap();
        assert!(json.contains("\"lowConfidence\""));
        assert!(json.contains("\"confidence\": 0.3"));

        let parsed: ProvenanceStats = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.low_confidence.len(), 1);
        assert_eq!(parsed.low_confidence[0].confidence, 0.3);
    }
}

// =============================================================================
// T7.3: Confidence Filter Tests
// =============================================================================

mod confidence_filter_tests {
    use super::*;

    #[test]
    fn test_parse_less_than() {
        let filter = ConfidenceFilter::parse("<0.7").unwrap();
        assert!(filter.matches(0.5));
        assert!(filter.matches(0.69));
        assert!(!filter.matches(0.7));
        assert!(!filter.matches(0.8));
    }

    #[test]
    fn test_parse_less_or_equal() {
        let filter = ConfidenceFilter::parse("<=0.7").unwrap();
        assert!(filter.matches(0.5));
        assert!(filter.matches(0.7));
        assert!(!filter.matches(0.71));
    }

    #[test]
    fn test_parse_greater_than() {
        let filter = ConfidenceFilter::parse(">0.8").unwrap();
        assert!(!filter.matches(0.7));
        assert!(!filter.matches(0.8));
        assert!(filter.matches(0.81));
        assert!(filter.matches(0.9));
    }

    #[test]
    fn test_parse_greater_or_equal() {
        let filter = ConfidenceFilter::parse(">=0.9").unwrap();
        assert!(!filter.matches(0.89));
        assert!(filter.matches(0.9));
        assert!(filter.matches(1.0));
    }

    #[test]
    fn test_parse_equal() {
        let filter = ConfidenceFilter::parse("=0.5").unwrap();
        assert!(filter.matches(0.5));
        assert!(filter.matches(0.5001)); // Within tolerance
        assert!(!filter.matches(0.51));
    }

    #[test]
    fn test_parse_with_whitespace() {
        let filter = ConfidenceFilter::parse("  <0.7  ").unwrap();
        assert!(filter.matches(0.5));
    }

    #[test]
    fn test_parse_invalid_no_operator() {
        assert!(ConfidenceFilter::parse("0.5").is_err());
    }

    #[test]
    fn test_parse_invalid_not_number() {
        assert!(ConfidenceFilter::parse("<abc").is_err());
    }

    #[test]
    fn test_parse_invalid_empty() {
        assert!(ConfidenceFilter::parse("").is_err());
    }

    #[test]
    fn test_boundary_values() {
        let filter = ConfidenceFilter::parse("<1.0").unwrap();
        assert!(filter.matches(0.0));
        assert!(filter.matches(0.99));
        assert!(!filter.matches(1.0));

        let filter = ConfidenceFilter::parse(">=0.0").unwrap();
        assert!(filter.matches(0.0));
        assert!(filter.matches(0.5));
        assert!(filter.matches(1.0));
    }
}

// =============================================================================
// T7.4: ProvenanceSummary Tests
// =============================================================================

mod provenance_summary_tests {
    use super::*;

    #[test]
    fn test_provenance_summary_default() {
        let summary = ProvenanceSummary::default();
        assert_eq!(summary.total, 0);
        assert_eq!(summary.needs_review, 0);
        assert_eq!(summary.reviewed, 0);
        assert!(summary.average_confidence.is_empty());
    }

    #[test]
    fn test_provenance_summary_with_confidence() {
        let mut summary = ProvenanceSummary::default();
        summary.total = 50;
        summary
            .average_confidence
            .insert("heuristic".to_string(), 0.72);
        summary
            .average_confidence
            .insert("converted".to_string(), 0.85);

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("\"averageConfidence\""));

        let parsed: ProvenanceSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.average_confidence.get("heuristic"), Some(&0.72));
        assert_eq!(parsed.average_confidence.get("converted"), Some(&0.85));
    }

    #[test]
    fn test_provenance_summary_serialization_skips_empty() {
        let summary = ProvenanceSummary::default();
        let json = serde_json::to_string(&summary).unwrap();

        // Empty average_confidence should be skipped
        assert!(!json.contains("averageConfidence"));
    }
}

// =============================================================================
// Integration: Full Provenance Workflow
// =============================================================================

mod integration_tests {
    use super::*;
    use acp::cache::GenerationInfo;

    #[test]
    fn test_generation_info() {
        let gen = GenerationInfo {
            id: "gen-20251223-abc1".to_string(),
            timestamp: "2025-12-23T10:00:00Z".to_string(),
            annotations_generated: 25,
            files_affected: 10,
        };

        let json = serde_json::to_string(&gen).unwrap();
        let parsed: GenerationInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.id, "gen-20251223-abc1");
        assert_eq!(parsed.annotations_generated, 25);
        assert_eq!(parsed.files_affected, 10);
    }

    #[test]
    fn test_full_provenance_stats_with_generation() {
        let mut stats = ProvenanceStats::default();
        stats.summary.total = 100;
        stats.summary.by_source.explicit = 40;
        stats.summary.by_source.heuristic = 35;
        stats.summary.by_source.converted = 25;
        stats.summary.needs_review = 15;
        stats.summary.reviewed = 85;

        stats.low_confidence.push(LowConfidenceEntry {
            target: "src/main.rs:main".to_string(),
            annotation: "@acp:summary".to_string(),
            confidence: 0.45,
            value: "Entry point".to_string(),
        });

        stats.last_generation = Some(GenerationInfo {
            id: "gen-20251223-xyz9".to_string(),
            timestamp: "2025-12-23T15:30:00Z".to_string(),
            annotations_generated: 35,
            files_affected: 12,
        });

        // Full roundtrip
        let json = serde_json::to_string_pretty(&stats).unwrap();
        let parsed: ProvenanceStats = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.summary.total, 100);
        assert_eq!(parsed.summary.by_source.explicit, 40);
        assert_eq!(parsed.summary.by_source.heuristic, 35);
        assert_eq!(parsed.summary.needs_review, 15);
        assert_eq!(parsed.low_confidence.len(), 1);
        assert!(parsed.last_generation.is_some());

        let gen = parsed.last_generation.unwrap();
        assert_eq!(gen.id, "gen-20251223-xyz9");
        assert_eq!(gen.annotations_generated, 35);
    }

    #[test]
    fn test_annotation_provenance_lifecycle() {
        // Simulate annotation lifecycle:
        // 1. Generated (needs review)
        // 2. Reviewed (marked as reviewed)

        // Step 1: Generated
        let mut prov = AnnotationProvenance {
            value: "Process user data".to_string(),
            source: SourceOrigin::Heuristic,
            confidence: Some(0.68),
            needs_review: true,
            reviewed: false,
            reviewed_at: None,
            generated_at: Some("2025-12-23T10:00:00Z".to_string()),
            generation_id: Some("gen-20251223-001".to_string()),
        };

        assert!(prov.needs_review);
        assert!(!prov.reviewed);

        // Step 2: Mark as reviewed
        prov.needs_review = false;
        prov.reviewed = true;
        prov.reviewed_at = Some("2025-12-23T14:00:00Z".to_string());

        assert!(!prov.needs_review);
        assert!(prov.reviewed);
        assert!(prov.reviewed_at.is_some());

        // Verify serialization roundtrip preserves the state
        let json = serde_json::to_string(&prov).unwrap();
        let parsed: AnnotationProvenance = serde_json::from_str(&json).unwrap();

        // After roundtrip, reviewed state should be preserved
        assert!(parsed.reviewed);
        assert!(parsed.reviewed_at.is_some());
        assert!(!parsed.needs_review); // was set to false
    }
}
