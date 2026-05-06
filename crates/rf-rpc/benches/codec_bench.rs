use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rf_rpc::codec;
use rf_rpc::types::{Action, Request, Response, RpcResult};
use std::collections::HashMap;

fn bench_encode_request(c: &mut Criterion) {
    let req = Request {
        id: "bench-req-001".into(),
        action: Action::Execute {
            command: "systemctl status nginx".into(),
            env: [("LANG".into(), "en_US.UTF-8".into())]
                .into_iter()
                .collect(),
            workdir: Some("/opt/app".into()),
        },
        timeout_ms: Some(30000),
        reason: None,
    };

    c.bench_function("encode_request", |b| {
        b.iter(|| codec::encode(&req).expect("encode"));
    });
}

fn bench_decode_request(c: &mut Criterion) {
    let req = Request {
        id: "bench-req-001".into(),
        action: Action::Execute {
            command: "systemctl status nginx".into(),
            env: HashMap::new(),
            workdir: None,
        },
        timeout_ms: Some(30000),
        reason: None,
    };
    let bytes = codec::encode(&req).expect("encode");

    let mut group = c.benchmark_group("codec");
    group.throughput(Throughput::Bytes(bytes.len() as u64));
    group.bench_function("decode_request", |b| {
        b.iter(|| codec::decode::<Request>(&bytes).expect("decode"));
    });
    group.finish();
}

fn bench_encode_response(c: &mut Criterion) {
    let resp = Response {
        id: "bench-resp-001".into(),
        result: RpcResult::Success {
            stdout:
                "● nginx.service - A high performance web server\n   Active: active (running)\n"
                    .into(),
            stderr: String::new(),
            exit_code: 0,
            duration_ms: 105,
        },
    };

    c.bench_function("encode_response", |b| {
        b.iter(|| codec::encode(&resp).expect("encode"));
    });
}

fn bench_roundtrip(c: &mut Criterion) {
    let req = Request {
        id: "rt-001".into(),
        action: Action::Execute {
            command: "ls -la /tmp".into(),
            env: [
                ("HOME".into(), "/root".into()),
                ("PATH".into(), "/usr/bin:/bin".into()),
            ]
            .into_iter()
            .collect(),
            workdir: Some("/tmp".into()),
        },
        timeout_ms: Some(5000),
        reason: None,
    };

    c.bench_function("roundtrip_request", |b| {
        b.iter(|| {
            let bytes = codec::encode(&req).expect("encode");
            codec::decode::<Request>(&bytes).expect("decode")
        });
    });
}

criterion_group!(
    benches,
    bench_encode_request,
    bench_decode_request,
    bench_encode_response,
    bench_roundtrip
);
criterion_main!(benches);
