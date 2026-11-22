//! Service management: check and start local services (llama.cpp, gateway, ollama).

use serde::Serialize;
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct ServiceInfo {
    pub port: u16,
    pub health_path: String,
    pub script: Option<String>,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub running: bool,
}

#[derive(Debug, Clone)]
pub struct Services {
    pub llama: ServiceInfo,
    pub gateway: ServiceInfo,
    pub ollama: ServiceInfo,
}

impl Services {
    pub fn new() -> Self {
        Self {
            llama: ServiceInfo {
                port: 8080,
                health_path: "/health".into(),
                script: Some("/home/kubuntu/llama.cpp-bin/llama-b12567/llama-server".into()),
                args: vec![],
                cwd: None,
                running: false,
            },
            gateway: ServiceInfo {
                port: 8081,
                health_path: "/health".into(),
                script: Some(
                    "/home/kubuntu/curso-ia/modelo-local/gateway-go/gateway".into(),
                ),
                args: vec![],
                cwd: Some("/home/kubuntu/curso-ia/modelo-local/gateway-go".into()),
                running: false,
            },
            ollama: ServiceInfo {
                port: 11434,
                health_path: "/api/tags".into(),
                script: Some("/usr/bin/ollama".into()),
                args: vec!["serve".into()],
                cwd: None,
                running: false,
            },
        }
    }

    pub fn list(&self) -> Vec<(&str, &ServiceInfo)> {
        vec![
            ("llama.cpp", &self.llama),
            ("gateway", &self.gateway),
            ("ollama", &self.ollama),
        ]
    }

    pub fn check_all(&mut self) {
        self.llama.running = check_service(self.llama.port, &self.llama.health_path);
        self.gateway.running = check_service(self.gateway.port, &self.gateway.health_path);
        self.ollama.running = check_service(self.ollama.port, &self.ollama.health_path);
    }

    pub fn get_stopped(&self) -> Vec<&ServiceInfo> {
        self.list()
            .into_iter()
            .filter(|(_, s)| !s.running)
            .map(|(_, s)| s)
            .collect()
    }

    pub fn start(&mut self, name: &str) -> bool {
        match name {
            "llama.cpp" => start_service(&mut self.llama),
            "gateway" => start_service(&mut self.gateway),
            "ollama" => start_service(&mut self.ollama),
            _ => false,
        }
    }
}

impl Default for Services {
    fn default() -> Self {
        Self::new()
    }
}

fn check_service(port: u16, health_path: &str) -> bool {
    // Try HTTP health endpoint
    if let Ok(resp) = reqwest::blocking::get(format!("http://127.0.0.1:{}{}", port, health_path)) {
        if resp.status().is_success() {
            return true;
        }
    }
    // TCP fallback
    if let Ok(_) = std::net::TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", port).parse().unwrap(),
        Duration::from_millis(500),
    ) {
        return true;
    }
    false
}

fn start_service(svc: &mut ServiceInfo) -> bool {
    if svc.running {
        return true;
    }
    let script = match &svc.script {
        Some(s) => s,
        None => return false,
    };

    if !std::path::Path::new(script).exists() {
        eprintln!("No executable found for service: {}", script);
        return false;
    }

    println!("Starting service on port {}...", svc.port);
    let mut cmd = Command::new(script);
    cmd.args(&svc.args);
    if let Some(cwd) = &svc.cwd {
        cmd.current_dir(cwd);
    }
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    if cmd.spawn().is_err() {
        eprintln!("Failed to spawn service on port {}", svc.port);
        return false;
    }

    // Wait up to 10 seconds for it to become healthy
    for _ in 0..20 {
        std::thread::sleep(Duration::from_millis(500));
        if check_service(svc.port, &svc.health_path) {
            svc.running = true;
            println!("Service started successfully on port {}", svc.port);
            return true;
        }
    }

    eprintln!("Failed to start service on port {} (timeout)", svc.port);
    false
}
