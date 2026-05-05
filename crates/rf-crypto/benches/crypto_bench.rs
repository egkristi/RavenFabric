use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use rf_crypto::keys::StaticKey;
use rf_crypto::noise::handshake;

fn bench_key_generation(c: &mut Criterion) {
    c.bench_function("StaticKey::generate", |b| {
        b.iter(|| StaticKey::generate());
    });
}

fn bench_handshake(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    c.bench_function("Noise_XX_handshake", |b| {
        b.iter(|| {
            rt.block_on(async {
                let key_a = StaticKey::generate();
                let key_b = StaticKey::generate();
                let (mut client, mut server) = tokio::io::duplex(65536);
                let (r_a, r_b) = tokio::join!(
                    handshake(&mut client, true, &key_a),
                    handshake(&mut server, false, &key_b),
                );
                r_a.expect("handshake A");
                r_b.expect("handshake B");
            });
        });
    });
}

fn bench_secure_channel_throughput(c: &mut Criterion) {
    use rf_crypto::channel::SecureChannel;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    let key_a = StaticKey::generate();
    let key_b = StaticKey::generate();

    let (state_a, state_b, peer_a, peer_b) = rt.block_on(async {
        let (mut client, mut server) = tokio::io::duplex(65536);
        let (r_a, r_b) = tokio::join!(
            handshake(&mut client, true, &key_a),
            handshake(&mut server, false, &key_b),
        );
        let (sa, pa) = r_a.expect("handshake A");
        let (sb, pb) = r_b.expect("handshake B");
        (sa, sb, pa, pb)
    });

    let (client_read, client_write) = tokio::io::duplex(1024 * 1024);
    let (server_read, server_write) = tokio::io::duplex(1024 * 1024);

    let chan_a = SecureChannel::new(server_read, client_write, state_a, peer_a);
    let chan_b = SecureChannel::new(client_read, server_write, state_b, peer_b);

    let payload = vec![0u8; 4096];

    let mut group = c.benchmark_group("SecureChannel");
    group.throughput(Throughput::Bytes(4096));
    group.bench_function("send_recv_4KB", |b| {
        b.iter(|| {
            rt.block_on(async {
                chan_a.send(&payload).await.expect("send");
                chan_b.recv().await.expect("recv");
            });
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_key_generation,
    bench_handshake,
    bench_secure_channel_throughput
);
criterion_main!(benches);
