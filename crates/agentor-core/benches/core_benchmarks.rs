use criterion::{black_box, criterion_group, criterion_main, Criterion};

use agentor_core::{Message, Role};
use uuid::Uuid;

fn bench_message_creation(c: &mut Criterion) {
    let session_id = Uuid::new_v4();

    c.bench_function("Message::user", |b| {
        b.iter(|| Message::user(black_box("Hello, world!"), black_box(session_id)));
    });

    c.bench_function("Message::new", |b| {
        b.iter(|| {
            Message::new(
                black_box(Role::Assistant),
                black_box("This is a response from the assistant."),
                black_box(session_id),
            )
        });
    });
}

fn bench_message_serialization(c: &mut Criterion) {
    let session_id = Uuid::new_v4();
    let msg = Message::user(
        "Hello, this is a test message for benchmarking.",
        session_id,
    );
    let json = serde_json::to_string(&msg).expect("serialize");

    c.bench_function("Message serialize", |b| {
        b.iter(|| serde_json::to_string(black_box(&msg)));
    });

    c.bench_function("Message deserialize", |b| {
        b.iter(|| serde_json::from_str::<Message>(black_box(&json)));
    });
}

fn bench_message_batch(c: &mut Criterion) {
    let session_id = Uuid::new_v4();

    c.bench_function("create 1000 messages", |b| {
        b.iter(|| {
            let mut messages = Vec::with_capacity(1000);
            for i in 0..1000 {
                messages.push(Message::user(format!("Message number {i}"), session_id));
            }
            black_box(messages)
        });
    });
}

criterion_group!(
    benches,
    bench_message_creation,
    bench_message_serialization,
    bench_message_batch,
);
criterion_main!(benches);
