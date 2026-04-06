#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]
use criterion::{black_box, criterion_group, criterion_main, Criterion};

use argentor_core::{Message, Role, ToolCall, ToolResult};
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

fn bench_message_serde_roundtrip(c: &mut Criterion) {
    let session_id = Uuid::new_v4();
    let msg = Message::new(
        Role::Assistant,
        "This is a longer response that simulates real-world assistant output with multiple sentences. \
         It includes various details about a topic and spans several lines of text to provide a \
         realistic payload size for serialization benchmarking.",
        session_id,
    );

    c.bench_function("Message serde roundtrip", |b| {
        b.iter(|| {
            let json = serde_json::to_string(black_box(&msg)).expect("serialize");
            let deserialized: Message =
                serde_json::from_str(black_box(&json)).expect("deserialize");
            black_box(deserialized)
        });
    });
}

fn bench_toolcall_creation(c: &mut Criterion) {
    c.bench_function("ToolCall creation (simple args)", |b| {
        b.iter(|| {
            black_box(ToolCall {
                id: black_box("call_001").to_string(),
                name: black_box("file_read").to_string(),
                arguments: serde_json::json!({ "path": "/tmp/test.txt" }),
            })
        });
    });

    c.bench_function("ToolCall creation (complex args)", |b| {
        b.iter(|| {
            black_box(ToolCall {
                id: black_box("call_002").to_string(),
                name: black_box("shell").to_string(),
                arguments: serde_json::json!({
                    "command": "ls -la /workspace",
                    "timeout": 30,
                    "env": { "HOME": "/root", "PATH": "/usr/bin" },
                    "working_dir": "/workspace"
                }),
            })
        });
    });
}

fn bench_toolcall_json_conversion(c: &mut Criterion) {
    let call = ToolCall {
        id: "call_abc".to_string(),
        name: "http_fetch".to_string(),
        arguments: serde_json::json!({
            "url": "https://api.example.com/data",
            "method": "POST",
            "headers": { "Authorization": "Bearer token123", "Content-Type": "application/json" },
            "body": { "query": "test" }
        }),
    };
    let json = serde_json::to_string(&call).expect("serialize");

    c.bench_function("ToolCall serialize", |b| {
        b.iter(|| serde_json::to_string(black_box(&call)));
    });

    c.bench_function("ToolCall deserialize", |b| {
        b.iter(|| serde_json::from_str::<ToolCall>(black_box(&json)));
    });

    c.bench_function("ToolCall serde roundtrip", |b| {
        b.iter(|| {
            let serialized = serde_json::to_string(black_box(&call)).expect("serialize");
            let deserialized: ToolCall =
                serde_json::from_str(black_box(&serialized)).expect("deserialize");
            black_box(deserialized)
        });
    });
}

fn bench_toolresult_creation(c: &mut Criterion) {
    c.bench_function("ToolResult::success (short)", |b| {
        b.iter(|| black_box(ToolResult::success(black_box("call_001"), black_box("ok"))));
    });

    c.bench_function("ToolResult::success (long content)", |b| {
        let content = "x".repeat(4096);
        b.iter(|| {
            black_box(ToolResult::success(
                black_box("call_002"),
                black_box(content.as_str()),
            ))
        });
    });

    c.bench_function("ToolResult::error (short)", |b| {
        b.iter(|| {
            black_box(ToolResult::error(
                black_box("call_003"),
                black_box("command failed with exit code 1"),
            ))
        });
    });

    c.bench_function("ToolResult::error (long content)", |b| {
        let content = format!(
            "Error: permission denied while accessing /etc/shadow\nStack trace:\n{}",
            "  at frame_N\n".repeat(50)
        );
        b.iter(|| {
            black_box(ToolResult::error(
                black_box("call_004"),
                black_box(content.as_str()),
            ))
        });
    });
}

criterion_group!(
    benches,
    bench_message_creation,
    bench_message_serialization,
    bench_message_batch,
    bench_message_serde_roundtrip,
    bench_toolcall_creation,
    bench_toolcall_json_conversion,
    bench_toolresult_creation,
);
criterion_main!(benches);
