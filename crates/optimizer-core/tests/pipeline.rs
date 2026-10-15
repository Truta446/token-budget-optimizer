use optimizer_core::*;
use proptest::prelude::*;

fn doc(id: &str, content: &str) -> Document {
    Document::new(id, content)
}

fn request(query: &str, max_tokens: usize, documents: Vec<Document>) -> OptimizeRequest {
    OptimizeRequest {
        query: query.into(),
        max_tokens,
        documents,
        ..Default::default()
    }
}

fn auth_corpus() -> Vec<Document> {
    vec![
        doc(
            "src/auth.ts",
            "import jwt from 'jsonwebtoken';\n\nexport function verifyToken(header: string) {\n  if (!header) throw new HttpError(401, 'missing token');\n  const token = header.replace('Bearer ', '');\n  return jwt.verify(token, process.env.JWT_SECRET);\n}\n\nexport function hashPassword(pw: string) {\n  return bcrypt.hash(pw, 12);\n}\n",
        ),
        doc(
            "src/middleware.ts",
            "export function authMiddleware(req, res, next) {\n  try {\n    req.user = verifyToken(req.headers.authorization);\n    next();\n  } catch (e) {\n    res.status(401).json({ error: 'unauthorized' });\n  }\n}\n",
        ),
        doc(
            "src/database.ts",
            "export const pool = new Pool({ connectionString: process.env.DATABASE_URL, max: 10 });\n\nexport async function query(sql, params) {\n  return pool.query(sql, params);\n}\n",
        ),
        doc(
            "README.md",
            "# Demo app\n\nRun `npm start` to launch the server on port 3000.\n",
        ),
    ]
}

#[test]
fn selects_relevant_code_and_drops_unrelated() {
    let req = OptimizeRequest {
        chunking: ChunkingOptions::for_budget(150),
        ..request(
            "Why is authentication returning 401 for verifyToken?",
            150,
            auth_corpus(),
        )
    };
    assert_eq!(req.chunking.target_tokens, 37);
    let r = optimize(&req).unwrap();
    assert!(r.stats.optimized_tokens <= 150);
    assert!(r.context.contains("HttpError(401"));
    assert!(r.context.contains("res.status(401)"));
    assert!(!r.context.contains("DATABASE_URL"));
    assert!(r.query_terms.contains(&"401".to_string()));
    let db = r
        .discarded
        .iter()
        .find(|d| d.location.document_id == "src/database.ts")
        .unwrap();
    assert_eq!(db.reason, DiscardReason::NoQueryMatch);
    assert!(r.selected.iter().all(|s| s.reason == SelectionReason::Relevant));
    assert!(r.selected.iter().all(|s| !s.score.matched_terms.is_empty()));
}

#[test]
fn final_token_count_is_exact() {
    let r = optimize(&request("token 401", 80, auth_corpus())).unwrap();
    assert_eq!(
        count_tokens(&r.context, TokenizerKind::O200kBase),
        r.stats.optimized_tokens
    );
    let r = optimize(&OptimizeRequest {
        tokenizer: TokenizerKind::Cl100kBase.into(),
        ..request("token 401", 80, auth_corpus())
    })
    .unwrap();
    assert_eq!(
        count_tokens(&r.context, TokenizerKind::Cl100kBase),
        r.stats.optimized_tokens
    );
}

#[test]
fn empty_inputs() {
    let r = optimize(&request("anything", 100, vec![])).unwrap();
    assert_eq!(r.context, "");
    assert_eq!(r.stats.original_tokens, 0);
    assert_eq!(r.stats.reduction_percent, 0.0);

    let r = optimize(&request(
        "anything",
        100,
        vec![doc("empty", ""), doc("blank", "  \n\n ")],
    ))
    .unwrap();
    assert_eq!(r.context, "");
    assert_eq!(r.stats.total_chunks, 0);
}

#[test]
fn empty_query_fills_in_source_order() {
    let docs = vec![doc("a", "alpha line\n"), doc("b", "beta line\n")];
    for query in ["", "   ", "what is the"] {
        let r = optimize(&request(query, 1000, docs.clone())).unwrap();
        assert!(r.query_terms.is_empty());
        assert_eq!(
            r.context,
            "[Source: a, lines 1-1]\nalpha line\n\n[Source: b, lines 1-1]\nbeta line"
        );
        assert!(r.selected.iter().all(|s| s.reason == SelectionReason::Fill));
    }
}

#[test]
fn zero_and_tiny_budgets() {
    for max in [0, 1, 3] {
        let r = optimize(&request("token", max, auth_corpus())).unwrap();
        assert!(r.stats.optimized_tokens <= max);
        assert!(r.selected.is_empty(), "nothing fits in {max} tokens with headers");
        assert!(
            r.discarded
                .iter()
                .any(|d| d.reason == DiscardReason::BudgetExceeded)
        );
    }
}

#[test]
fn removes_exact_and_normalized_duplicates_with_metadata() {
    let body = "export const RETRIES = 3;\nexport const TIMEOUT_MS = 5000;\n";
    let docs = vec![
        doc("a.ts", body),
        doc("b.ts", body),
        doc(
            "c.ts",
            "export const RETRIES = 3;\n    export const TIMEOUT_MS =   5000;\n",
        ),
        doc(
            "d.ts",
            "export const RETRIES = 5;\nexport const TIMEOUT_MS = 5000;\n",
        ),
    ];
    let r = optimize(&request("retries timeout", 1000, docs)).unwrap();
    let reason = |id: &str| {
        r.discarded
            .iter()
            .find(|d| d.location.document_id == id)
            .map(|d| (d.reason, d.duplicate_of.clone()))
    };
    assert_eq!(
        reason("b.ts"),
        Some((DiscardReason::ExactDuplicate, Some("a.ts#0".into())))
    );
    assert_eq!(
        reason("c.ts"),
        Some((DiscardReason::NormalizedDuplicate, Some("a.ts#0".into())))
    );
    assert_eq!(reason("d.ts"), None, "different value must be kept");
    assert!(r.context.contains("RETRIES = 5"));
    assert_eq!(r.stats.duplicate_chunks, 2);
}

#[test]
fn unicode_and_emoji_documents() {
    let docs = vec![
        doc(
            "pt.md",
            "A autenticação falhou porque o token expirou. 🔒\nTente novamente.\n",
        ),
        doc(
            "ja.md",
            "認証エラーが発生しました。トークンを確認してください。\n",
        ),
        doc("emoji.txt", &"🚀🔥✨".repeat(300)),
    ];
    let r = optimize(&request("autenticacao token", 200, docs.clone())).unwrap();
    assert!(r.context.contains("autenticação falhou"));
    let r = optimize(&request("認証エラー", 200, docs.clone())).unwrap();
    assert!(r.context.contains("認証エラーが発生しました"));
    let r = optimize(&request("", 50, docs)).unwrap();
    assert!(r.stats.optimized_tokens <= 50);
}

#[test]
fn validation_errors() {
    let bad = |req: OptimizeRequest| optimize(&req).unwrap_err();
    let e = bad(request("q", 10, vec![doc("", "x")]));
    assert_eq!(
        e,
        OptimizeError::InvalidInput {
            field: "documents[0].id".into(),
            message: "must be a non-empty string".into()
        }
    );
    let e = bad(request("q", 10, vec![doc("a", "x"), doc("a", "y")]));
    assert!(e.to_string().contains("duplicate id"));
    let e = bad(request("q", 10, vec![doc("a", "x").with_weight(f64::NAN)]));
    assert!(e.to_string().contains("documents[0].weight"));
    let e = bad(OptimizeRequest {
        chunking: ChunkingOptions {
            target_tokens: 10,
            overlap_tokens: 10,
        },
        ..request("q", 10, vec![])
    });
    assert!(e.to_string().contains("overlapTokens"));
    let e = bad(OptimizeRequest {
        chunking: ChunkingOptions {
            target_tokens: 0,
            overlap_tokens: 0,
        },
        ..request("q", 10, vec![])
    });
    assert!(e.to_string().contains("targetTokens"));
    let e = bad(OptimizeRequest {
        selection: SelectionOptions {
            redundancy_penalty: 1.5,
            ..Default::default()
        },
        ..request("q", 10, vec![])
    });
    assert_eq!(e.code(), "TBO_INVALID_INPUT");
}

#[test]
fn large_document_is_handled_and_budget_respected() {
    let mut content = String::new();
    for i in 0..20_000 {
        content.push_str(&format!(
            "fn handler_{i}(req: Request) -> Response {{ log(\"request {i}\"); ok() }}\n"
        ));
    }
    content.push_str("fn refresh_session_token() { /* rotates the session token */ }\n");
    let r = optimize(&request(
        "refresh session token",
        300,
        vec![doc("big.rs", &content)],
    ))
    .unwrap();
    assert!(r.stats.original_tokens > 400_000);
    assert!(r.stats.optimized_tokens <= 300);
    assert!(r.context.contains("refresh_session_token"));
}

#[test]
fn oversized_chunks_are_discarded_but_smaller_ones_still_fit() {
    let big = "token ".repeat(400);
    let docs = vec![doc("big", &big), doc("small", "token rotation happens hourly\n")];
    let req = OptimizeRequest {
        chunking: ChunkingOptions {
            target_tokens: 1000,
            overlap_tokens: 0,
        },
        ..request("token", 60, docs)
    };
    let r = optimize(&req).unwrap();
    assert!(r.context.contains("token rotation"));
    assert!(
        r.discarded
            .iter()
            .any(|d| d.location.document_id == "big" && d.reason == DiscardReason::BudgetExceeded)
    );
}

#[test]
fn source_metadata_is_preserved() {
    let content = "first line\nsecond line about tokens\nthird line\n";
    let req = OptimizeRequest {
        chunking: ChunkingOptions {
            target_tokens: 4,
            overlap_tokens: 0,
        },
        ..request("tokens", 100, vec![doc("notes.txt", content)])
    };
    let r = optimize(&req).unwrap();
    let s = &r.selected[0];
    assert_eq!(s.location.document_id, "notes.txt");
    assert_eq!(s.location.start_line, 2);
    assert_eq!(&content[s.location.start_byte..s.location.end_byte], s.text);
    assert_eq!(
        r.sections[0].header,
        format!("[Source: notes.txt, lines 2-{}]", s.location.end_line)
    );
}

#[test]
fn document_weight_and_source_bonus_affect_ranking() {
    let docs = vec![
        doc("notes/a.md", "cache invalidation strategy\n"),
        doc("notes/b.md", "cache invalidation strategy notes\n"),
    ];
    let r = optimize(&request("cache invalidation", 1000, docs.clone())).unwrap();
    assert_eq!(
        r.selected[0].location.document_id, "notes/a.md",
        "shorter chunk wins BM25"
    );
    let mut weighted = docs.clone();
    weighted[1].weight = 3.0;
    let r = optimize(&request("cache invalidation", 1000, weighted)).unwrap();
    assert_eq!(r.selected[0].location.document_id, "notes/b.md");
    let named = vec![
        doc("notes/a.md", "cache invalidation strategy\n"),
        doc("cache.md", "cache invalidation strategy notes\n"),
    ];
    let r = optimize(&request("cache invalidation", 1000, named)).unwrap();
    assert_eq!(r.selected[0].location.document_id, "cache.md");
    assert!(r.selected[0].score.source_bonus > 0.0);
}

#[test]
fn custom_format_is_accounted_for() {
    let req = OptimizeRequest {
        format: FormatOptions {
            header_template: "<document source=\"{id}\" lines=\"{startLine}-{endLine}\">".into(),
            separator: "\n</document>\n\n".into(),
        },
        chunking: ChunkingOptions {
            target_tokens: 30,
            overlap_tokens: 0,
        },
        ..request("token", 60, auth_corpus())
    };
    let r = optimize(&req).unwrap();
    assert!(r.context.starts_with("<document source="));
    assert!(r.stats.optimized_tokens <= 60);
    assert!(r.stats.formatting_tokens > 0);
}

fn arb_docs() -> impl Strategy<Value = Vec<Document>> {
    let line = prop_oneof![
        "[a-z]{1,8}( [a-z]{1,8}){0,10}",
        "\\PC{0,30}",
        Just("token auth 401".to_string()),
        Just("🚀 ação 認証".to_string()),
    ];
    prop::collection::vec(prop::collection::vec(line, 0..25), 0..6).prop_map(|docs| {
        docs.into_iter()
            .enumerate()
            .map(|(i, lines)| Document::new(format!("doc-{i}.txt"), lines.join("\n")))
            .collect()
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(96))]

    #[test]
    fn never_exceeds_budget_and_is_deterministic(
        docs in arb_docs(),
        query in "(token|auth|401|ação|認証|[a-z]{2,6}| ){0,6}",
        max in 0usize..400,
        target in 8usize..120,
        overlap_pct in 0usize..50,
        order in prop_oneof![Just(ContextOrder::Relevance), Just(ContextOrder::Source)],
        include_unmatched in any::<bool>(),
    ) {
        let req = OptimizeRequest {
            chunking: ChunkingOptions { target_tokens: target, overlap_tokens: target * overlap_pct / 100 },
            order,
            selection: SelectionOptions { include_unmatched, ..Default::default() },
            ..request(&query, max, docs)
        };
        let a = optimize(&req).unwrap();
        let b = optimize(&req).unwrap();
        prop_assert!(a.stats.optimized_tokens <= max);
        prop_assert_eq!(count_tokens(&a.context, TokenizerKind::O200kBase), a.stats.optimized_tokens);
        prop_assert_eq!(&a.context, &b.context);
        prop_assert_eq!(&a.selected, &b.selected);
        prop_assert_eq!(&a.discarded, &b.discarded);
        // Every chunk is accounted for exactly once.
        prop_assert_eq!(a.selected.len() + a.discarded.len(), a.stats.total_chunks);
        // Selected text is verbatim source text.
        for s in &a.selected {
            let content = &req.documents[s.location.document_index].content;
            prop_assert_eq!(&content[s.location.start_byte..s.location.end_byte], s.text.as_str());
        }
    }
}

#[test]
fn neighbor_expansion_keeps_surrounding_text_without_displacing_matches() {
    let content = "## Failed payments\nWhen a card payment fails we retry after 3, 5 and 7 days.\nAfter the final retry the workspace is suspended\nuntil the balance is paid.\n\n## Unrelated\nColors are configurable.\n";
    let base = OptimizeRequest {
        chunking: ChunkingOptions {
            target_tokens: 16,
            overlap_tokens: 0,
        },
        selection: SelectionOptions {
            neighbor_hops: 2,
            ..Default::default()
        },
        ..request("card payment fails", 200, vec![doc("billing.md", content)])
    };
    let without = optimize(&OptimizeRequest {
        selection: SelectionOptions {
            neighbor_hops: 0,
            ..Default::default()
        },
        ..base.clone()
    })
    .unwrap();
    assert!(!without.context.contains("until the balance is paid"));
    let with = optimize(&base).unwrap();
    assert!(with.context.contains("until the balance is paid"));
    assert!(
        with.selected
            .iter()
            .any(|s| s.reason == SelectionReason::Neighbor)
    );
    assert!(with.stats.optimized_tokens <= 200);
    // Every chunk is reported exactly once.
    assert_eq!(
        with.selected.len() + with.discarded.len(),
        with.stats.total_chunks
    );
}

#[test]
fn stemming_matches_inflected_forms_and_can_be_disabled() {
    let docs = vec![doc("a.md", "The job failed twice and retries were exhausted.\n")];
    let r = optimize(&request("why does the job fail? retry", 100, docs.clone())).unwrap();
    assert_eq!(r.selected.len(), 1);
    assert_eq!(r.selected[0].score.matched_terms, vec!["job", "fail", "retry"]);
    let r = optimize(&OptimizeRequest {
        ranking: RankingOptions {
            stemming: false,
            ..Default::default()
        },
        ..request("fail retry", 100, docs)
    })
    .unwrap();
    assert!(r.selected.is_empty());
}

#[test]
fn header_fields_render_and_are_budgeted() {
    let req = OptimizeRequest {
        format: FormatOptions {
            header_template: "[{meta.title}]({meta.url})".into(),
            separator: "\n\n".into(),
        },
        ..request(
            "token rotation",
            60,
            vec![
                doc("a", "token rotation happens hourly\n")
                    .with_header_field("title", "Rotation")
                    .with_header_field("url", "https://docs.example.com/rotation"),
            ],
        )
    };
    let r = optimize(&req).unwrap();
    assert!(
        r.context
            .starts_with("[Rotation](https://docs.example.com/rotation)\n")
    );
    assert_eq!(
        count_tokens(&r.context, TokenizerKind::O200kBase),
        r.stats.optimized_tokens
    );

    let bad = OptimizeRequest {
        documents: vec![doc("a", "x").with_header_field("bad key", "v")],
        ..req.clone()
    };
    assert!(optimize(&bad).unwrap_err().to_string().contains("headerFields"));
}

#[test]
fn large_inputs_are_deterministic_across_runs() {
    // Above PARALLEL_MIN_BYTES, so the parallel path runs when the feature is on.
    let documents: Vec<Document> = (0..40)
        .map(|d| {
            let content: String = (0..400)
                .map(|i| format!("line {i} of doc {d}: cache {} retry {}\n", i % 7, (i * d) % 11))
                .collect();
            doc(&format!("doc-{d}.txt"), &content)
        })
        .collect();
    assert!(documents.iter().map(|d| d.content.len()).sum::<usize>() > PARALLEL_MIN_BYTES);
    let req = request("cache retry 3", 2_000, documents);
    let a = optimize(&req).unwrap();
    for _ in 0..3 {
        let b = optimize(&req).unwrap();
        assert_eq!(a.context, b.context);
        assert_eq!(a.selected, b.selected);
        assert_eq!(a.discarded, b.discarded);
    }
}
