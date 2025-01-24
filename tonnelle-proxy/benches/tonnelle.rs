use criterion::{criterion_group, criterion_main, Criterion};
use reqwest::blocking::ClientBuilder;
use tonnelle_proxy::start_bench_proxy;

mod flamegraph;

async fn bench_full_request() {
    let addr = start_bench_proxy();
    println!("Proxy started on {}", addr);
    let client = ClientBuilder::new()
        .proxy(reqwest::Proxy::http(format!("http://{}", addr)).unwrap())
        .build()
        .unwrap();
    let response = client
        .get("http://ifconfig.me")
        .send()
        .expect("Failed to send request");
    assert_eq!(response.status(), 200);
}

fn bench_tonnelle_request(c: &mut Criterion) {
    c.bench_function("full_request", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter_with_large_drop(|| async {
                bench_full_request().await;
            });
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().with_profiler(flamegraph::FlamegraphProfiler::new(100));
    targets = bench_tonnelle_request
}
criterion_main!(benches);
