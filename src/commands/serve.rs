use crate::{error::CliError, ui, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

pub struct ServeOptions {
    pub host: String,
    pub port: u16,
    pub rust_socket: Option<PathBuf>,
    pub bun_socket: Option<PathBuf>,
    pub no_nginx: bool,
    pub no_bun: bool,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 3000,
            rust_socket: None,
            bun_socket: None,
            no_nginx: false,
            no_bun: false,
        }
    }
}

pub fn run(options: ServeOptions) -> Result<()> {
    ui::header("🚀", "Starting ForgeDB server");

    // Create runtime directory for sockets
    let runtime_dir = PathBuf::from(".forgedb/runtime");
    fs::create_dir_all(&runtime_dir)?;

    // Determine socket paths
    let rust_socket = options.rust_socket.clone()
        .unwrap_or_else(|| runtime_dir.join("rust.sock"));
    let bun_socket = options.bun_socket.clone()
        .unwrap_or_else(|| runtime_dir.join("bun.sock"));

    // Clean up old sockets
    let _ = fs::remove_file(&rust_socket);
    let _ = fs::remove_file(&bun_socket);

    // Set up signal handler for graceful shutdown
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        ui::info("Shutting down...");
        r.store(false, Ordering::SeqCst);
    }).map_err(|e| CliError::Other(format!("Failed to set signal handler: {}", e)))?;

    // Start services
    let mut processes: Vec<Process> = Vec::new();

    // 1. Start Rust API server
    ui::info("Starting Rust API server...");
    let rust_child = start_rust_server(&rust_socket)?;
    processes.push(Process {
        name: "Rust API".to_string(),
        child: rust_child,
    });
    ui::success(&format!("Rust API listening on {}", rust_socket.display()));

    // Wait for Rust server to be ready
    thread::sleep(Duration::from_millis(500));

    // 2. Start Bun runtime server (unless disabled)
    if !options.no_bun {
        ui::info("Starting Bun runtime server...");
        let bun_child = start_bun_server(&bun_socket)?;
        processes.push(Process {
            name: "Bun Runtime".to_string(),
            child: bun_child,
        });
        ui::success(&format!("Bun runtime listening on {}", bun_socket.display()));

        // Wait for Bun server to be ready
        thread::sleep(Duration::from_millis(500));
    }

    // 3. Start nginx reverse proxy (unless disabled)
    if !options.no_nginx {
        ui::info("Starting nginx reverse proxy...");

        // Generate nginx config
        let nginx_config_path = runtime_dir.join("nginx.conf");
        generate_nginx_config(&nginx_config_path, &options, &rust_socket, &bun_socket)?;

        // Start nginx
        let nginx_child = start_nginx(&nginx_config_path, &runtime_dir)?;
        processes.push(Process {
            name: "nginx".to_string(),
            child: nginx_child,
        });
        ui::success(&format!("nginx listening on http://{}:{}", options.host, options.port));
    }

    println!();
    ui::success("ForgeDB server is running!");
    println!();
    println!("  Server:     http://{}:{}", options.host, options.port);
    println!("  Rust API:   {}", rust_socket.display());
    if !options.no_bun {
        println!("  Bun Runtime: {}", bun_socket.display());
    }
    println!();
    println!("Press Ctrl+C to stop");
    println!();

    // Monitor processes and handle shutdown
    while running.load(Ordering::SeqCst) {
        thread::sleep(Duration::from_millis(100));

        // Check if any process died
        for process in &mut processes {
            if let Ok(Some(status)) = process.child.try_wait() {
                ui::error(&format!("{} process exited with status: {}", process.name, status));
                running.store(false, Ordering::SeqCst);
                break;
            }
        }
    }

    // Graceful shutdown
    ui::info("Stopping services...");
    for mut process in processes {
        ui::info(&format!("Stopping {}...", process.name));
        let _ = process.child.kill();
        let _ = process.child.wait();
    }

    // Clean up sockets
    let _ = fs::remove_file(&rust_socket);
    let _ = fs::remove_file(&bun_socket);

    ui::success("Server stopped");
    Ok(())
}

struct Process {
    name: String,
    child: Child,
}

fn start_rust_server(socket_path: &Path) -> Result<Child> {
    // Build Rust server binary if needed
    let binary_path = find_rust_binary()?;

    // Start server on Unix socket
    let child = Command::new(binary_path)
        .arg("--socket")
        .arg(socket_path)
        .env("RUST_LOG", "info")
        .spawn()
        .map_err(|e| CliError::Other(format!("Failed to start Rust server: {}", e)))?;

    Ok(child)
}

fn start_bun_server(socket_path: &Path) -> Result<Child> {
    // Find Bun binary (check bundled first, then system PATH)
    let bun_path = find_bundled_binary("bun")
        .or_else(|| which::which("bun").ok())
        .ok_or_else(|| CliError::Other("Bun not found. Install from https://bun.sh".to_string()))?;

    // Find server.ts
    let server_ts = find_bun_server_file()?;

    // Start Bun server on Unix socket
    let child = Command::new(bun_path)
        .arg(&server_ts)
        .env("SOCKET_PATH", socket_path)
        .env("DB_MODE", "ffi")
        .env("FORGEDB_DATA", "./data")
        .spawn()
        .map_err(|e| CliError::Other(format!("Failed to start Bun server: {}", e)))?;

    Ok(child)
}

fn start_nginx(config_path: &Path, runtime_dir: &Path) -> Result<Child> {
    // Find nginx binary (check bundled first, then system PATH)
    let nginx_path = find_bundled_binary("nginx")
        .or_else(|| which::which("nginx").ok())
        .ok_or_else(|| CliError::Other(
            "nginx not found. Install with: brew install nginx (macOS) or apt install nginx (Linux)".to_string()
        ))?;

    // Create nginx directories
    let logs_dir = runtime_dir.join("logs");
    let temp_dir = runtime_dir.join("temp");
    fs::create_dir_all(&logs_dir)?;
    fs::create_dir_all(&temp_dir)?;

    // Start nginx
    let child = Command::new(nginx_path)
        .arg("-c")
        .arg(config_path.canonicalize()?)
        .arg("-g")
        .arg("daemon off;") // Run in foreground
        .spawn()
        .map_err(|e| CliError::Other(format!("Failed to start nginx: {}", e)))?;

    Ok(child)
}

fn generate_nginx_config(
    config_path: &Path,
    options: &ServeOptions,
    rust_socket: &Path,
    bun_socket: &Path,
) -> Result<()> {
    let runtime_dir = config_path.parent().unwrap();
    let logs_dir = runtime_dir.join("logs");
    let temp_dir = runtime_dir.join("temp");

    let config = format!(
r#"# ForgeDB nginx configuration
# Auto-generated - do not edit manually

# Process settings
worker_processes auto;
pid {runtime_dir}/nginx.pid;

# Error log
error_log {logs_dir}/error.log warn;

events {{
    worker_connections 1024;
}}

http {{
    # Logging
    access_log {logs_dir}/access.log;

    # Basic settings
    sendfile on;
    tcp_nopush on;
    tcp_nodelay on;
    keepalive_timeout 65;
    types_hash_max_size 2048;

    # Temp directories
    client_body_temp_path {temp_dir}/client_body;
    proxy_temp_path {temp_dir}/proxy;
    fastcgi_temp_path {temp_dir}/fastcgi;
    uwsgi_temp_path {temp_dir}/uwsgi;
    scgi_temp_path {temp_dir}/scgi;

    # MIME types
    include /usr/local/etc/nginx/mime.types;
    default_type application/octet-stream;

    # Upstream servers
    upstream rust_api {{
        server unix:{rust_socket};
    }}

    upstream bun_runtime {{
        server unix:{bun_socket};
    }}

    # Main server
    server {{
        listen {host}:{port};
        server_name _;

        # Health check
        location /health {{
            access_log off;
            return 200 "OK\n";
            add_header Content-Type text/plain;
        }}

        # UI Components & TypeScript API -> Bun
        location ~ ^/(pages|routes|components) {{
            proxy_pass http://bun_runtime;
            proxy_http_version 1.1;
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection 'upgrade';
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
            proxy_cache_bypass $http_upgrade;
        }}

        # REST API & Database -> Rust
        # Forwards `Upgrade`/`Connection` so the generated change-feed WebSocket
        # endpoints (`/subscribe/<model>`, #62 Direction A) can upgrade; plain
        # HTTP requests are unaffected by these headers.
        location / {{
            proxy_pass http://rust_api;
            proxy_http_version 1.1;
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection 'upgrade';
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
            proxy_cache_bypass $http_upgrade;
        }}
    }}
}}
"#,
        runtime_dir = runtime_dir.display(),
        logs_dir = logs_dir.display(),
        temp_dir = temp_dir.display(),
        rust_socket = rust_socket.display(),
        bun_socket = bun_socket.display(),
        host = options.host,
        port = options.port,
    );

    fs::write(config_path, config)
        .map_err(|e| CliError::Other(format!("Failed to write nginx config: {}", e)))?;

    Ok(())
}

fn find_rust_binary() -> Result<PathBuf> {
    // Guess the binary name from the CWD directory name; fall back to "app"
    // if the CWD has no final component (e.g. filesystem root, which is
    // valid but highly unusual — .unwrap() would panic there).
    let cwd = std::env::current_dir()?;
    let bin_name = cwd
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".to_string());

    // Check target/release first
    let release_path = PathBuf::from("target/release").join(&bin_name);
    if release_path.exists() {
        return Ok(release_path);
    }

    // Check target/debug
    let debug_path = PathBuf::from("target/debug").join(&bin_name);
    if debug_path.exists() {
        return Ok(debug_path);
    }

    Err(CliError::Other(
        "Rust binary not found. Run 'cargo build' first.".to_string(),
    ))
}

fn find_bun_server_file() -> Result<PathBuf> {
    // Check common locations
    let candidates = [
        "runtime/bun/src/server.ts",
        "generated/server.ts",
        "src/server.ts",
    ];

    for candidate in &candidates {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Ok(path);
        }
    }

    Err(CliError::Other(
        "Bun server file not found. Run 'forgedb generate' first.".to_string()
    ))
}

/// Find a bundled binary in common locations
fn find_bundled_binary(name: &str) -> Option<PathBuf> {
    // Get the path of the current binary
    let current_exe = std::env::current_exe().ok()?;
    let exe_dir = current_exe.parent()?;

    // Check locations relative to the binary
    let candidates = [
        // For npm package: binaries/forgedb-cli and binaries/runtime/nginx are siblings
        exe_dir.join(format!("runtime/{}", name)),
        // For development: binary in target/release, runtime in assets/bin or npm-package/binaries/runtime
        exe_dir.join(format!("../../npm-package/binaries/runtime/{}", name)),
        exe_dir.join(format!("../../assets/bin/{}", name)),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return Some(candidate.clone());
        }
    }

    None
}
