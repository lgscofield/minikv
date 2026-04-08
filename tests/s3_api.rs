//! Basic test for the S3-compatible API (PUT then GET)
use reqwest::Client;
use std::env;
use std::fs;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

fn get_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn resolve_bin(var_names: &[&str]) -> Option<String> {
    for name in var_names {
        if let Ok(value) = env::var(name) {
            return Some(value);
        }
    }
    None
}

fn start_coord(http_port: u16, grpc_port: u16, test_id: &str) -> (Child, String, String) {
    let coord_bin = resolve_bin(&["CARGO_BIN_EXE_minikv-coord", "CARGO_BIN_EXE_minikv_coord"])
        .expect("Coordinator binary env var not set by cargo test");
    let coord_data = format!("coord-s3-data-{}", test_id);
    let _ = fs::remove_dir_all(&coord_data);
    let _ = fs::create_dir_all(&coord_data);
    let config_path = format!("/tmp/minikv-config-s3-{}.toml", test_id);
    fs::write(
        &config_path,
        format!(
            "node_id = 'coord-s3-{}'\nrole = 'coordinator'\nreplicas = 1\n",
            test_id
        ),
    )
    .expect("Failed to write config.toml");

    let mut cmd = Command::new(coord_bin);
    cmd.args([
        "serve",
        "--id",
        &format!("coord-s3-{}", test_id),
        "--bind",
        &format!("127.0.0.1:{}", http_port),
        "--grpc",
        &format!("127.0.0.1:{}", grpc_port),
        "--db",
        &coord_data,
    ]);
    cmd.env_clear();
    for (key, value) in env::vars() {
        if key != "MINIKV_CONFIG" && key != "RUST_LOG" && key != "RUST_BACKTRACE" {
            cmd.env(&key, &value);
        }
    }
    cmd.env("MINIKV_CONFIG", &config_path);
    cmd.env("RUST_LOG", "debug");
    cmd.env("RUST_BACKTRACE", "1");

    let log =
        fs::File::create(format!("coord-s3-{}.log", test_id)).expect("Failed to create log file");
    let log_err = log.try_clone().expect("Failed to clone log file");
    cmd.stdout(Stdio::from(log));
    cmd.stderr(Stdio::from(log_err));

    (
        cmd.spawn().expect("Failed to launch minikv-coord server"),
        coord_data,
        config_path,
    )
}

fn start_volume(
    http_port: u16,
    grpc_port: u16,
    coord_http_port: u16,
    test_id: &str,
) -> (Child, String, String) {
    let volume_bin = resolve_bin(&["CARGO_BIN_EXE_minikv-volume", "CARGO_BIN_EXE_minikv_volume"])
        .expect("Volume binary env var not set by cargo test");
    let vol_data = format!("vol-s3-data-{}", test_id);
    let vol_wal = format!("vol-s3-wal-{}", test_id);
    let _ = fs::remove_dir_all(&vol_data);
    let _ = fs::remove_dir_all(&vol_wal);
    let _ = fs::create_dir_all(&vol_data);
    let _ = fs::create_dir_all(&vol_wal);

    let mut cmd = Command::new(volume_bin);
    cmd.args([
        "serve",
        "--id",
        &format!("vol-s3-{}", test_id),
        "--bind",
        &format!("127.0.0.1:{}", http_port),
        "--grpc",
        &format!("127.0.0.1:{}", grpc_port),
        "--data",
        &vol_data,
        "--wal",
        &vol_wal,
        "--coordinators",
        &format!("http://127.0.0.1:{}", coord_http_port),
    ]);
    cmd.env_clear();
    for (key, value) in env::vars() {
        if key != "MINIKV_CONFIG" && key != "RUST_LOG" && key != "RUST_BACKTRACE" {
            cmd.env(&key, &value);
        }
    }
    cmd.env("RUST_LOG", "debug");
    cmd.env("RUST_BACKTRACE", "1");

    let log =
        fs::File::create(format!("vol-s3-{}.log", test_id)).expect("Failed to create log file");
    let log_err = log.try_clone().expect("Failed to clone log file");
    cmd.stdout(Stdio::from(log));
    cmd.stderr(Stdio::from(log_err));

    (
        cmd.spawn().expect("Failed to launch minikv-volume server"),
        vol_data,
        vol_wal,
    )
}

async fn wait_for_endpoint(childs: &mut [&mut Child], url: &str) {
    let client = Client::new();
    let start = Instant::now();
    loop {
        for child in childs.iter_mut() {
            if let Some(status) = child.try_wait().expect("Error waiting for server") {
                panic!("A server exited prematurely (exit code {status})");
            }
        }
        if start.elapsed() > Duration::from_secs(20) {
            panic!("Timeout: endpoint not ready at {url}");
        }
        if let Ok(resp) = client.get(url).send().await {
            if resp.status().is_success() || resp.status().as_u16() == 404 {
                break;
            }
        }
        sleep(Duration::from_millis(100));
    }
}

#[tokio::test]
async fn test_s3_put_get() {
    if resolve_bin(&["CARGO_BIN_EXE_minikv-coord", "CARGO_BIN_EXE_minikv_coord"]).is_none()
        || resolve_bin(&["CARGO_BIN_EXE_minikv-volume", "CARGO_BIN_EXE_minikv_volume"]).is_none()
    {
        eprintln!("Skipping test_s3_put_get: required binary env vars are not set");
        return;
    }

    let test_id = format!(
        "{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );

    let coord_http = get_free_port();
    let coord_grpc = get_free_port();
    let vol_http = get_free_port();
    let vol_grpc = get_free_port();

    let (mut coord, coord_data, config_path) = start_coord(coord_http, coord_grpc, &test_id);
    let (mut volume, vol_data, vol_wal) = start_volume(vol_http, vol_grpc, coord_http, &test_id);

    let s3_url = format!("http://127.0.0.1:{}/s3/testbucket/hello.txt", coord_http);
    wait_for_endpoint(&mut [&mut coord, &mut volume], &s3_url).await;

    let client = Client::new();
    let data = b"Hello, S3!";
    let put_resp = client
        .put(&s3_url)
        .body(data.as_ref())
        .send()
        .await
        .unwrap();
    assert!(put_resp.status().is_success(), "PUT failed: {:?}", put_resp);
    let get_resp = client.get(&s3_url).send().await.unwrap();
    assert!(get_resp.status().is_success(), "GET failed: {:?}", get_resp);
    let body = get_resp.bytes().await.unwrap();
    assert_eq!(body.as_ref(), data, "GET body mismatch");

    let _ = coord.kill();
    let _ = coord.wait();
    let _ = volume.kill();
    let _ = volume.wait();

    let _ = fs::remove_file(config_path);
    let _ = fs::remove_file(format!("coord-s3-{}.log", test_id));
    let _ = fs::remove_file(format!("vol-s3-{}.log", test_id));
    let _ = fs::remove_dir_all(coord_data);
    let _ = fs::remove_dir_all(vol_data);
    let _ = fs::remove_dir_all(vol_wal);
}
