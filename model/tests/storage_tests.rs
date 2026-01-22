use model::sharding::compute_file_hash;
use model::{HttpStorage, LocalStorage, ModelShard, StorageBackend, StorageError};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, SystemTime};
use tokio::fs::{self, File};
use tokio::io::AsyncWriteExt;

fn temp_dir(prefix: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    dir.push(format!("aigen_{prefix}_{nanos}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[tokio::test]
async fn test_http_storage_mirror_fallback() {
    let root = temp_dir("http_mirror");
    let shard_path = root.join("shard.bin");
    let data = b"mirror-data".to_vec();
    write_bytes(&shard_path, &data).await;
    let hash = compute_file_hash(&shard_path).await.expect("hash");
    let shard = sample_shard("mirror-model", hash, data.len() as u64);

    let failure_server = TestServer::start(Arc::new(|_, _| HttpResponse {
        status: 500,
        body: Vec::new(),
    }));

    let success_data = Arc::new(data);
    let success_path = format!("/models/{}/shards/shard_0.bin", shard.model_id);
    let success_server = TestServer::start(Arc::new(move |method, path| {
        if path == success_path && (method == "GET" || method == "HEAD") {
            let body = if method == "GET" {
                success_data.as_ref().clone()
            } else {
                Vec::new()
            };
            HttpResponse { status: 200, body }
        } else {
            HttpResponse {
                status: 404,
                body: Vec::new(),
            }
        }
    }));

    let storage = HttpStorage::new(vec![failure_server.url.clone(), success_server.url.clone()])
        .expect("http storage");
    let output_path = root.join("downloaded.bin");
    storage
        .download_shard(&shard, &output_path)
        .await
        .expect("download");

    let output_hash = compute_file_hash(&output_path).await.expect("hash");
    assert_eq!(output_hash, hash);
}

async fn write_test_file(path: &Path, size: usize) {
    let mut file = File::create(path).await.expect("create file");
    let buffer = vec![5u8; size];
    file.write_all(&buffer).await.expect("write");
    file.flush().await.expect("flush");
}

async fn write_bytes(path: &Path, data: &[u8]) {
    let mut file = File::create(path).await.expect("create file");
    file.write_all(data).await.expect("write");
    file.flush().await.expect("flush");
}

struct HttpResponse {
    status: u16,
    body: Vec<u8>,
}

struct TestServer {
    url: String,
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl TestServer {
    fn start(responder: Arc<dyn Fn(&str, &str) -> HttpResponse + Send + Sync>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        let handle = thread::spawn(move || {
            listener.set_nonblocking(true).expect("nonblocking");
            while running_clone.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let responder = responder.clone();
                        thread::spawn(move || {
                            handle_connection(stream, responder);
                        });
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            url: format!("http://{}", addr),
            running,
            handle: Some(handle),
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn handle_connection(
    mut stream: TcpStream,
    responder: Arc<dyn Fn(&str, &str) -> HttpResponse + Send + Sync>,
) {
    let mut buffer = [0u8; 4096];
    let mut request = Vec::new();
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|chunk| chunk == b"\r\n\r\n") {
                    break;
                }
            }
            Err(_) => return,
        }
    }

    let request_text = String::from_utf8_lossy(&request);
    let request_line = request_text.lines().next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    let response = responder(method, path);
    let status_text = match response.status {
        200 => "OK",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Error",
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        status_text,
        response.body.len()
    );
    if stream.write_all(header.as_bytes()).is_ok() && method != "HEAD" {
        let _ = stream.write_all(&response.body);
    }
}

fn sample_shard(model_id: &str, hash: [u8; 32], size: u64) -> ModelShard {
    ModelShard {
        model_id: model_id.to_string(),
        shard_index: 0,
        total_shards: 1,
        hash,
        size,
        ipfs_cid: None,
        http_urls: Vec::new(),
        locations: Vec::new(),
    }
}

#[tokio::test]
async fn test_local_storage_upload_download() {
    let root = temp_dir("local_storage");
    let data_path = root.join("input.bin");
    write_test_file(&data_path, 1024 * 1024).await;
    let hash = compute_file_hash(&data_path).await.expect("hash");
    let shard = sample_shard("test-model", hash, 1024 * 1024);

    let storage = LocalStorage::new(root.clone());
    let location = storage
        .upload_shard(&shard, &data_path)
        .await
        .expect("upload");

    let primary = PathBuf::from(location);
    assert!(fs::metadata(&primary).await.is_ok());

    for replica_id in 1..=2 {
        let replica_path = root
            .join("models")
            .join("test-model")
            .join("replicas")
            .join(format!("replica_{}", replica_id))
            .join("shard_0.bin");
        assert!(fs::metadata(&replica_path).await.is_ok());
    }

    let output_path = root.join("downloaded.bin");
    storage
        .download_shard(&shard, &output_path)
        .await
        .expect("download");

    let output_hash = compute_file_hash(&output_path).await.expect("hash");
    assert_eq!(output_hash, hash);
}

#[tokio::test]
async fn test_local_storage_redundancy() {
    let root = temp_dir("local_storage_redundancy");
    let data_path = root.join("input.bin");
    write_test_file(&data_path, 512 * 1024).await;
    let hash = compute_file_hash(&data_path).await.expect("hash");
    let shard = sample_shard("redundant-model", hash, 512 * 1024);

    let storage = LocalStorage::new(root.clone());
    storage
        .upload_shard(&shard, &data_path)
        .await
        .expect("upload");

    for replica_id in 1..=2 {
        let replica_path = root
            .join("models")
            .join("redundant-model")
            .join("replicas")
            .join(format!("replica_{}", replica_id))
            .join("shard_0.bin");
        assert!(fs::metadata(&replica_path).await.is_ok());
    }

    let missing_replica = root
        .join("models")
        .join("redundant-model")
        .join("replicas")
        .join("replica_2")
        .join("shard_0.bin");
    let _ = fs::remove_file(&missing_replica).await;

    let output_path = root.join("downloaded.bin");
    storage
        .download_shard(&shard, &output_path)
        .await
        .expect("download");

    let output_hash = compute_file_hash(&output_path).await.expect("hash");
    assert_eq!(output_hash, hash);
}

#[tokio::test]
async fn test_storage_backend_error_handling() {
    let root = temp_dir("local_storage_error");
    let storage = LocalStorage::new(root.clone());
    let shard = sample_shard("missing", [1u8; 32], 1024);

    let result = storage.verify_shard(&shard).await.expect("verify");
    assert!(!result);

    let result = storage
        .download_shard(&shard, &root.join("downloaded.bin"))
        .await;
    assert!(matches!(result, Err(StorageError::ShardNotFound)));

    let bad_path = root.join("bad.bin");
    write_test_file(&bad_path, 1024).await;
    let bad_shard = sample_shard("bad", [9u8; 32], 1024);
    let result = storage.upload_shard(&bad_shard, &bad_path).await;
    assert!(matches!(result, Err(StorageError::IntegrityFailed)));

    let http_storage =
        HttpStorage::new(vec!["http://127.0.0.1:1".to_string()]).expect("http storage");
    let result = http_storage
        .download_shard(&bad_shard, &root.join("downloaded_http.bin"))
        .await;
    assert!(matches!(result, Err(StorageError::AllMirrorsFailed)));
}
