//! services.rs - Service management for llama.cpp, gateway, ollama

use reqwest::Client;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub port: u16,
    pub health_path: String,
    pub running: bool,
}

impl ServiceInfo {
    pub fn is_running(&self) -> bool {
        self.running
    }
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
                port: 8080,
                health_path: "/health".to_string(),
                running: false,
            },
            gateway: ServiceInfo {
                port: 8081,
                health_path: "/health".to_string(),
                running: false,
            },
            ollama: ServiceInfo {
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
        let client = Client::new();
        match client
            .get(format!(
                "http://127.0.0.1:{}{}",
                service.port, service.health_path
            ))
            .timeout(Duration::from_secs(2))
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    pub fn list(&self) -> Vec<(&str, &ServiceInfo)> {
        vec![
            ("llama.cpp", &self.llama_cpp),
            ("gateway", &self.gateway),
            ("ollama", &self.ollama),
        ]
    }

    #[allow(dead_code)]
    pub fn print_status(&self) {
        println!("Services status:");
        println!(
            "  llama.cpp: {} (Port {})",
            if self.llama_cpp.running {
                "Running"
            } else {
                "Stopped"
            },
            self.llama_cpp.port
        );
        println!(
            "  gateway: {} (Port {})",
            if self.gateway.running {
                "Running"
            } else {
                "Stopped"
            },
            self.gateway.port
        );
        println!(
            "  ollama: {} (Port {})",
            if self.ollama.running {
                "Running"
            } else {
                "Stopped"
            },
            self.ollama.port
        );
    }
}

impl Default for Services {
    fn default() -> Self {
        Self::new()
    }
}
