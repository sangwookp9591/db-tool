//! Driver-only timing and sampled RSS / physical footprint, with no app or GUI.
#![cfg(target_os = "macos")]
use axdb_driver_protocol::{read_frame, write_frame, ExecuteChunk, Message, Reply, Request};
use dbcore::*;
use std::{
    collections::BTreeMap,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
#[link(name = "proc")]
unsafe extern "C" {
    fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut std::ffi::c_void) -> i32;
}
fn memory(pid: u32) -> Option<(u64, u64)> {
    // macOS SDK sys/resource.h rusage_info_v2: UUID[16] then 18 u64 fields, resident/footprint at slots 8/9.
    let mut info = [0u64; 20];
    let status = unsafe { proc_pid_rusage(pid as i32, 2, info.as_mut_ptr().cast()) };
    (status == 0).then_some((info[8], info[9]))
}
async fn measure(
    binary: &std::path::Path,
    incremental: bool,
    sets: usize,
    slow: bool,
) -> serde_json::Value {
    let mut child = tokio::process::Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let pid = child.id().unwrap();
    let mut input = child.stdin.take().unwrap();
    let mut output = child.stdout.take().unwrap();
    write_frame(
        &mut input,
        &Message::Request {
            id: 1,
            request: Request::Handshake { protocol: 1 },
        },
    )
    .await
    .unwrap();
    assert!(matches!(
        read_frame(&mut output).await.unwrap(),
        Some(Message::Result {
            result: Reply::Handshake { .. },
            ..
        })
    ));
    let config = ConnectConfig {
        fields: BTreeMap::from([
            ("host".into(), "127.0.0.1".into()),
            ("port".into(), "55432".into()),
            ("database".into(), "axdb_test".into()),
            ("user".into(), "postgres".into()),
            ("sslmode".into(), "disable".into()),
        ]),
        secret: Some("testpw".into()),
        connect_timeout_ms: 3000,
    };
    write_frame(
        &mut input,
        &Message::Request {
            id: 2,
            request: Request::Connect(config),
        },
    )
    .await
    .unwrap();
    let Some(Message::Result {
        result: Reply::Connected { session },
        ..
    }) = read_frame(&mut output).await.unwrap()
    else {
        panic!("connect");
    };
    let start_memory = memory(pid).unwrap();
    let finished = Arc::new(AtomicBool::new(false));
    let flag = finished.clone();
    let sampler = std::thread::spawn(move || {
        let mut rss = 0;
        let mut footprint = 0;
        let mut samples = 0;
        while !flag.load(Ordering::Acquire) {
            if let Some((r, f)) = memory(pid) {
                rss = rss.max(r);
                footprint = footprint.max(f);
                samples += 1;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        (rss, footprint, samples)
    });
    let query="SELECT i::int8, repeat('x',32) AS a, repeat('y',32) AS b, repeat('z',32) AS c, i::numeric, i::text, true AS v, i+1, i+2, i+3 FROM generate_series(1,100000) i";
    let req:ExecRequest=serde_json::from_value(serde_json::json!({"text":vec![query;sets].join(";"),"row_limit":100000,"timeout_ms":120000,"read_only":true})).unwrap();
    let started = Instant::now();
    let mut first = None;
    let mut rows = 0;
    write_frame(
        &mut input,
        &Message::Request {
            id: 3,
            request: if incremental {
                Request::ExecuteStream { session, req }
            } else {
                Request::Execute { session, req }
            },
        },
    )
    .await
    .unwrap();
    loop {
        match read_frame(&mut output).await.unwrap().unwrap() {
            Message::Chunk {
                chunk: ExecuteChunk::Rows { rows: batch, .. },
                ..
            } => {
                first.get_or_insert_with(|| started.elapsed());
                rows += batch.len();
                if slow {
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            }
            Message::Chunk { .. } => {}
            Message::Result {
                result: Reply::Unit,
                ..
            } => break,
            Message::Error { error, .. } => panic!("benchmark error kind {:?}", error.kind),
            _ => panic!("unexpected reply"),
        }
    }
    let elapsed = started.elapsed();
    finished.store(true, Ordering::Release);
    let (rss, footprint, samples) = sampler.join().unwrap();
    assert_eq!(rows, sets * 100000);
    child.kill().await.unwrap();
    child.wait().await.unwrap();
    serde_json::json!({"mode":if incremental{"incremental"}else{"legacy"},"rows":rows,"result_sets":sets,"slow_consumer":slow,"start_rss_bytes":start_memory.0,"peak_rss_bytes":rss,"start_footprint_bytes":start_memory.1,"peak_footprint_bytes":footprint,"memory_samples":samples,"first_chunk_ms":first.unwrap().as_secs_f64()*1000.,"total_ms":elapsed.as_secs_f64()*1000.})
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "manual real PostgreSQL performance run"]
async fn driver_streaming_memory_benchmark() {
    let binary =
        std::path::PathBuf::from(std::env::var_os("AXDB_DRIVER_DIR").expect("built plugins"))
            .join("axdb-driver-postgres");
    let mut samples = vec![];
    for pair in 0..3 {
        for sets in [1, 10] {
            for incremental in if pair % 2 == 0 {
                [false, true]
            } else {
                [true, false]
            } {
                samples.push(measure(&binary, incremental, sets, false).await);
            }
        }
    }
    samples.push(measure(&binary, true, 10, true).await);
    let report = serde_json::json!({"boundary":"driver stdio to a decoding consumer; excludes host/UI","profile":"debug","memory":"proc_pid_rusage RUSAGE_INFO_V2 sampled every 2ms; sampled maxima, not exact OS high-water marks","limit":"100000 per result unchanged; 1000000 total uses ten result sets","samples":samples});
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("bench/results-l2-streaming.json");
    std::fs::write(path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
}
