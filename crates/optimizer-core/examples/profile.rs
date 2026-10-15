//! Rough per-stage timings on a large synthetic document:
//! `cargo run --release -p optimizer-core --example profile`
use std::time::Instant;

use optimizer_core::*;

fn main() {
    let content: String = (0..50_000)
        .map(|i| format!("function handler{i}(req) {{ return respond(req, {i}); }}\n"))
        .collect();
    let tok = Tokenizer::new(TokenizerKind::O200kBase);
    tok.count("warm up");
    let t = Instant::now();
    let starts = tok.token_starts(&content);
    println!("encode once:  {:>7} tokens {:?}", starts.len(), t.elapsed());
    let t = Instant::now();
    let terms: usize = content
        .split_inclusive('\n')
        .map(|l| ranking::analyze(l, true).len())
        .sum();
    println!("analyze:      {terms:>7} terms  {:?}", t.elapsed());
    let req = OptimizeRequest {
        query: "rotate JWT signing keys".into(),
        max_tokens: 300,
        documents: vec![Document::new("big.js", content)],
        chunking: ChunkingOptions::for_budget(300),
        ..Default::default()
    };
    for _ in 0..3 {
        let t = Instant::now();
        let r = optimize(&req).unwrap();
        println!(
            "optimize:     {:>7} chunks {:?}",
            r.stats.total_chunks,
            t.elapsed()
        );
    }
}
