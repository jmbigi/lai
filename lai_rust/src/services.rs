//! Service management: check and start local services (llama.cpp, gateway, ollama).

use reqwest::Client;
use serde::Serialize;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::sleep;

#[derive(Debug, Serialize)]
pub struct ServiceInfo {
    pub port: u16,
    pub health_path: String,
    pub script: Option<String>,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    #[serde(skip)]
    pub running: AtomicBool,
}

impl ServiceInfo {
    pub fn new(port: u16, health_path: &str) -> Self {
        Self {
            port,
            health_path: health_path.to_string(),
            script: None,
            args: vec![],
            cwd: None,
            running: AtomicBool::new(false),
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn set_running(&self, v: bool) {
        self.running.store(v, Ordering::Relaxed);
    }
}

#[derive(Debug)]
pub struct Services {
    pub llama: ServiceInfo,
    pub gateway: ServiceInfo,
    pub ollama: ServiceInfo,
}

impl Services {
    pub fn new() -> Self {
        // Rutas configurables por entorno: no dependen de la máquina del autor.
        let llama_script =
            std::env::var("LAI_LLAMA_SERVER").unwrap_or_else(|_| "llama-server".to_string());
        let gateway_script =
            std::env::var("LAI_GATEWAY_BIN").unwrap_or_else(|_| "gateway".to_string());
        let gateway_cwd = std::env::var("LAI_GATEWAY_DIR").ok();
        let ollama_script =
            std::env::var("LAI_OLLAMA_BIN").unwrap_or_else(|_| "ollama".to_string());
        Self {
            llama: ServiceInfo {
                port: 8080,
                health_path: "/health".into(),
                script: Some(llama_script),
                args: vec![],
                cwd: None,
                running: AtomicBool::new(false),
            },
            gateway: ServiceInfo {
                port: 8081,
                health_path: "/health".into(),
                script: Some(gateway_script),
                args: vec![],
                cwd: gateway_cwd,
                running: AtomicBool::new(false),
            },
            ollama: ServiceInfo {
                port: 11434,
                health_path: "/api/tags".into(),
                script: Some(ollama_script),
                args: vec!["serve".into()],
                cwd: None,
                running: AtomicBool::new(false),
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

    pub fn check_all(&self) {
        self.llama
            .set_running(check_service(self.llama.port, &self.llama.health_path));
        self.gateway
            .set_running(check_service(self.gateway.port, &self.gateway.health_path));
        self.ollama
            .set_running(check_service(self.ollama.port, &self.ollama.health_path));
    }

    pub fn get_stopped(&self) -> Vec<&ServiceInfo> {
        self.list()
            .into_iter()
            .filter(|(_, s)| !s.is_running())
            .map(|(_, s)| s)
            .collect()
    }

    pub fn start(&self, name: &str) -> bool {
        match name {
            "llama.cpp" => start_service(&self.llama),
            "gateway" => start_service(&self.gateway),
            "ollama" => start_service(&self.ollama),
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
    // Prefer the HTTP health endpoint. A TCP-only fallback would report
    // unrelated processes (for example a web server on port 8080) as healthy.
    if !health_path.is_empty() {
        return match reqwest::blocking::get(format!("http://127.0.0.1:{}{}", port, health_path)) {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        };
    }
    std::net::TcpStream::connect_timeout(
        &format!("127.0.0.1:{}", port).parse().unwrap(),
        Duration::from_millis(500),
    )
    .is_ok()
}

fn start_service(svc: &ServiceInfo) -> bool {
    if svc.is_running() {
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
            svc.set_running(true);
            println!("Service started successfully on port {}", svc.port);
            return true;
        }
    }

    eprintln!("Failed to start service on port {} (timeout)", svc.port);
    false
}

pub async fn check_service_async(client: &Client, port: u16, health_path: &str) -> bool {
    // Prefer the HTTP health endpoint. Only fall back to a TCP probe when the
    // service has no health path, so unrelated listeners are not reported.
    if !health_path.is_empty() {
        return match client
            .get(format!("http://127.0.0.1:{}{}", port, health_path))
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        };
    }
    tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
        .await
        .is_ok()
}

pub async fn check_all_services_async(services: &Services, client: &Client) {
    services.llama.set_running(
        check_service_async(client, services.llama.port, &services.llama.health_path).await,
    );
    services.gateway.set_running(
        check_service_async(client, services.gateway.port, &services.gateway.health_path).await,
    );
    services.ollama.set_running(
        check_service_async(client, services.ollama.port, &services.ollama.health_path).await,
    );
}

/// Regla 9 - Supervivencia: verifica que al menos un modelo local esté disponible.
/// Versión síncrona para uso en comandos no-async (usa check_service síncrono).
pub fn ensure_local_model_available(services: &Services) -> Result<(), String> {
    // Solo usar check_service síncrono para evitar bloqueos en runtime async
    let llama_ok = check_service(services.llama.port, &services.llama.health_path);
    let gateway_ok = check_service(services.gateway.port, &services.gateway.health_path);
    let ollama_ok = check_service(services.ollama.port, &services.ollama.health_path);

    services.llama.set_running(llama_ok);
    services.gateway.set_running(gateway_ok);
    services.ollama.set_running(ollama_ok);

    let has_local = llama_ok || gateway_ok || ollama_ok;
    if !has_local {
        return Err(
            "Regla 9 (Supervivencia): No hay modelos locales disponibles (llama.cpp, gateway, Ollama).\n\
            Inicia al menos uno antes de operar:\n\
            - llama.cpp: llama-server (o define LAI_LLAMA_SERVER)\n\
            - gateway: gateway (o define LAI_GATEWAY_BIN / LAI_GATEWAY_DIR)\n\
            - ollama: ollama serve\n\
            O usa --allow-remote para endpoints no locales (requiere consentimiento explícito)."
                .to_string(),
        );
    }
    Ok(())
}

/// Versión async para uso en comandos async.
pub async fn ensure_local_model_available_async(
    services: &Services,
    client: &Client,
) -> Result<(), String> {
    check_all_services_async(services, client).await;
    let has_local = services.list().iter().any(|(_, s)| s.is_running());
    if !has_local {
        return Err(
            "Regla 9 (Supervivencia): No hay modelos locales disponibles (llama.cpp, gateway, Ollama).\n\
            Inicia al menos uno antes de operar:\n\
            - llama.cpp: llama-server (o define LAI_LLAMA_SERVER)\n\
            - gateway: gateway (o define LAI_GATEWAY_BIN / LAI_GATEWAY_DIR)\n\
            - ollama: ollama serve\n\
            O usa --allow-remote para endpoints no locales (requiere consentimiento explícito)."
                .to_string(),
        );
    }
    Ok(())
}

async fn _start_service_async(svc: &ServiceInfo) -> bool {
    if svc.is_running() {
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

    let client = Client::new();
    for _ in 0..20 {
        sleep(Duration::from_millis(500)).await;
        if check_service_async(&client, svc.port, &svc.health_path).await {
            svc.set_running(true);
            println!("Service started successfully on port {}", svc.port);
            return true;
        }
    }

    eprintln!("Failed to start service on port {} (timeout)", svc.port);
    false
}
