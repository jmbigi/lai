//! services.rs - Service management for llama.cpp, gateway, ollama

use anyhow::Result;

#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub name: String,
    pub port: u16,
    pub health_path: String,
    pub running: bool,
}

pub struct Services {
    pub llama_cpp: ServiceInfo,
    pub gateway: ServiceInfo,
    pub ollama: ServiceInfo,
}

impl Services {
    pub fn new() -> Self {
        Self {
            llama_cpp: ServiceInfo {
                name: "llama.cpp".to_string(),
                port: 8080,
                health_path: "/health".to_string(),
                running: false,
            },
            gateway: ServiceInfo {
                name: "gateway".to_string(),
                port: 8081,
                health_path: "/health".to_string(),
                running: false,
            },
            ollama: ServiceInfo {
                name: "ollama".to_string(),
                port: 11434,
                health_path: "/api/tags".to_string(),
                running: false,
            },
        }
    }

    pub async fn check_all(&mut self) {
        self.llama_cpp.running = self.check_service(&self.llama_cpp).await;
        self.gateway.running = self.check_service(&self.gateway).await;
        self.ollama.running = self.check_service(&self.ollama).await;
    }

    async fn check_service(&self, service: &ServiceInfo) -> bool {
        let client = reqwest::Client::new();
        match client
            .get(format!("http://127.0.0.1:{}{}", service.port, service.health_path))
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    pub fn print_status(&self) {
        println!("Services status:");
        println!(
            "  llama.cpp: {} (Port {})",
            if self.llama_cpp.running { "Running" } else { "Stopped" },
            self.llama_cpp.port
        );
        println!(
            "  gateway: {} (Port {})",
            if self.gateway.running { "Running" } else { "Stopped" },
            self.gateway.port
        );
        println!(
            "  ollama: {} (Port {})",
            if self.ollama.running { "Running" } else { "Stopped" },
            self.ollama.port
        );
    }
}

impl Default for Services {
    fn default() -> Self {
        Self::new()
    }
}