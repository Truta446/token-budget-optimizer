//! Engine-only benchmarks (no N-API boundary).
//!
//!     cargo bench -p optimizer-core                      # sequential
//!     cargo bench -p optimizer-core --features parallel  # rayon above PARALLEL_MIN_BYTES
//!
//! Corpora are generated from a fixed seed, so every run sees identical input.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use optimizer_core::{ChunkingOptions, Document, OptimizeRequest, Tokenizer, TokenizerKind, optimize};

const WORDS: &[&str] = &[
    "request", "response", "token", "session", "cache", "database", "query", "retry", "timeout", "config",
    "deploy", "build", "server", "client", "user", "account", "billing", "invoice", "payment", "webhook",
    "event", "queue", "worker", "job", "schedule", "log", "metric", "alert", "error", "status", "header",
    "body", "json", "schema", "field", "value", "record", "table",
];

/// xorshift64*: tiny, deterministic, good enough for synthetic text.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

fn corpus(docs: usize, lines: usize) -> Vec<Document> {
    let mut rng = Rng(42);
    (0..docs)
        .map(|d| {
            let mut content = String::new();
            for l in 0..lines {
                let n = 4 + (rng.next() % 10) as usize;
                let words: Vec<&str> = (0..n)
                    .map(|_| WORDS[(rng.next() % WORDS.len() as u64) as usize])
                    .collect();
                if l % 6 == 0 {
                    content.push_str(&format!("function {}{l}({}) {{\n", words[0], words[1]));
                } else {
                    content.push_str(&format!("  // {}\n", words.join(" ")));
                }
            }
            Document::new(format!("src/module-{d}.ts"), content)
        })
        .collect()
}

fn request(documents: Vec<Document>, max_tokens: usize) -> OptimizeRequest {
    OptimizeRequest {
        query: "retry timeout when the cache misses and the queue worker fails".into(),
        max_tokens,
        chunking: ChunkingOptions::for_budget(max_tokens),
        documents,
        ..Default::default()
    }
}

fn bench_tokenizer(c: &mut Criterion) {
    let text = corpus(1, 20_000).remove(0).content;
    let tok = Tokenizer::new(TokenizerKind::O200kBase);
    tok.count("warm up");
    let mut group = c.benchmark_group("tokenizer");
    group.throughput(Throughput::Bytes(text.len() as u64));
    group.bench_function("count/o200k_base", |b| b.iter(|| tok.count(black_box(&text))));
    group.bench_function("token_starts/o200k_base", |b| {
        b.iter(|| tok.token_starts(black_box(&text)))
    });
    group.finish();
}

fn bench_optimize(c: &mut Criterion) {
    let mut group = c.benchmark_group("optimize");
    group.sample_size(20);
    for (docs, lines) in [(10, 200), (100, 200), (1000, 50), (10, 5000)] {
        let req = request(corpus(docs, lines), 8_000);
        let bytes: usize = req.documents.iter().map(|d| d.content.len()).sum();
        group.throughput(Throughput::Bytes(bytes as u64));
        group.bench_with_input(
            BenchmarkId::new("corpus", format!("{docs}x{lines}")),
            &req,
            |b, req| b.iter(|| optimize(black_box(req)).unwrap()),
        );
    }
    group.finish();
}

fn bench_budgets(c: &mut Criterion) {
    let documents = corpus(100, 200);
    let mut group = c.benchmark_group("budget");
    group.sample_size(20);
    for max_tokens in [500, 8_000, 128_000] {
        let req = request(documents.clone(), max_tokens);
        group.bench_with_input(BenchmarkId::from_parameter(max_tokens), &req, |b, req| {
            b.iter(|| optimize(black_box(req)).unwrap())
        });
    }
    group.finish();
}

criterion_group!(benches, bench_tokenizer, bench_optimize, bench_budgets);
criterion_main!(benches);
