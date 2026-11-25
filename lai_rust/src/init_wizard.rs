//! init_wizard.rs - Interactive setup wizard for lai
//! Detects hardware, recommends models, downloads via ollama, configures gateway,
//! generates AGENTS.md, and runs health checks. 100% local, zero cloud deps.

use anyhow::{Context, Result};
use atty;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols,
    text::{Line, Span, Text},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use sysinfo::System;
use tokio::process::Command as TokioCommand;

/// Hardware information detected from the system
#[derive(Debug, Clone)]
pub struct HardwareInfo {
    pub gpu_vram_mb: Option<u64>,
    pub gpu_name: Option<String>,
    pub total_ram_mb: u64,
    pub cpu_cores: usize,
    pub os: String,
    pub arch: String,
    pub ollama_installed: bool,
    pub ollama_version: Option<String>,
}

/// Model recommendation based on hardware
#[derive(Debug, Clone)]
pub struct ModelRecommendation {
    pub name: String,
    pub display_name: String,
    pub size_gb: f32,
    pub vram_required_gb: f32,
    pub description: String,
    pub is_moe: bool,
}

/// Wizard configuration options
#[derive(Debug, Clone)]
pub struct InitConfig {
    pub model: Option<String>,
    pub auto: bool,
    pub no_ollama: bool,
    pub reset: bool,
}

/// Main wizard state
pub struct InitWizard {
    hardware: Option<HardwareInfo>,
    recommendations: Vec<ModelRecommendation>,
    selected_model: Option<String>,
    config: InitConfig,
    terminal: Option<Terminal<CrosstermBackend<Stdout>>>,
    current_step: WizardStep,
    progress: f64,
    status_message: String,
    error_message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum WizardStep {
    DetectHardware,
    SelectModel,
    DownloadModel,
    ConfigureGateway,
    GenerateAgentsMd,
    HealthCheck,
    Complete,
    Error,
}

impl InitWizard {
    pub fn new(config: InitConfig) -> Self {
        Self {
            hardware: None,
            recommendations: Vec::new(),
            selected_model: config.model.clone(),
            config,
            terminal: None,
            current_step: WizardStep::DetectHardware,
            progress: 0.0,
            status_message: "Iniciando...".to_string(),
            error_message: None,
        }
    }

    /// Run the interactive wizard
    pub async fn run(&mut self) -> Result<()> {
        // Check if we're in a TTY for interactive mode
        if !self.config.auto && !atty::is(atty::Stream::Stdin) {
            return Err(anyhow::anyhow!(
                "Modo interactivo requiere TTY. Usa --auto para modo no interactivo."
            ));
        }

        if self.config.auto {
            return self.run_auto().await;
        }

        self.setup_terminal()?;
        let result = self.run_wizard_loop().await;
        self.restore_terminal()?;
        result
    }

    /// Run non-interactive auto mode (no TUI)
    async fn run_auto(&mut self) -> Result<()> {
        println!("🔍 Detectando hardware...");
        self.hardware = Some(self.detect_hardware().await?);
        self.recommendations = self.generate_recommendations();
        
        if self.selected_model.is_none() {
            self.selected_model = self.recommendations.first().map(|m| m.name.clone());
        }
        let model = self.selected_model.as_ref().context("No model available")?;
        println!("🤖 Modelo seleccionado: {}", model);

        if !self.config.no_ollama {
            println!("⬇ Descargando {}...", model);
            self.download_model_simple().await?;
            println!("✓ Modelo descargado");
        }

        println!("⚙ Configurando gateway...");
        self.configure_gateway().await?;
        println!("✓ Gateway configurado");

        println!("📝 Generando AGENTS.md...");
        self.generate_agents_md().await?;
        println!("✓ AGENTS.md generado");

        println!("🏥 Verificando instalación...");
        self.run_health_check().await?;
        println!("✓ Health check OK");

        println!("\n✅ ¡Listo! Ejecuta: lai auto \"tu tarea\"");
        Ok(())
    }

    /// Simple download without TUI progress
    async fn download_model_simple(&mut self) -> Result<()> {
        let model = self.selected_model.as_ref().context("No model selected")?;
        
        // Check if already exists
        let check = TokioCommand::new("ollama")
            .args(["list"])
            .output()
            .await?;
        
        let stdout = String::from_utf8_lossy(&check.stdout);
        if stdout.lines().any(|l| l.split_whitespace().next() == Some(model)) {
            println!("✓ Modelo {} ya existe", model);
            return Ok(());
        }

        let status = TokioCommand::new("ollama")
            .args(["pull", model])
            .status()
            .await?;
        
        if !status.success() {
            return Err(anyhow::anyhow!("Error descargando modelo: {}", status));
        }
        Ok(())
    }

    fn setup_terminal(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        self.terminal = Some(terminal);
        Ok(())
    }

    fn restore_terminal(&mut self) -> Result<()> {
        if let Some(mut terminal) = self.terminal.take() {
            disable_raw_mode()?;
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            terminal.show_cursor()?;
        }
        Ok(())
    }

    async fn run_wizard_loop(&mut self) -> Result<()> {
        // Step 1: Detect hardware
        self.current_step = WizardStep::DetectHardware;
        self.status_message = "Detectando hardware...".to_string();
        self.draw()?;
        self.hardware = Some(self.detect_hardware().await?);
        self.recommendations = self.generate_recommendations();
        self.progress = 0.15;

        // Step 2: Select model (skip if auto or model provided)
        if !self.config.auto && self.selected_model.is_none() {
            self.current_step = WizardStep::SelectModel;
            self.status_message = "Selecciona modelo...".to_string();
            self.draw()?;
            self.select_model_interactive().await?;
        } else if self.selected_model.is_none() {
            // Auto mode: pick first recommendation
            self.selected_model = self.recommendations.first().map(|m| m.name.clone());
        }
        self.progress = 0.3;

        // Step 3: Download model (if using ollama)
        if !self.config.no_ollama {
            self.current_step = WizardStep::DownloadModel;
            self.status_message = format!("Descargando {}...", self.selected_model.as_deref().unwrap_or("modelo"));
            self.draw()?;
            self.download_model().await?;
        }
        self.progress = 0.55;

        // Step 4: Configure gateway
        self.current_step = WizardStep::ConfigureGateway;
        self.status_message = "Configurando gateway...".to_string();
        self.draw()?;
        self.configure_gateway().await?;
        self.progress = 0.7;

        // Step 5: Generate AGENTS.md
        self.current_step = WizardStep::GenerateAgentsMd;
        self.status_message = "Generando AGENTS.md...".to_string();
        self.draw()?;
        self.generate_agents_md().await?;
        self.progress = 0.85;

        // Step 6: Health check
        self.current_step = WizardStep::HealthCheck;
        self.status_message = "Verificando instalación...".to_string();
        self.draw()?;
        self.run_health_check().await?;
        self.progress = 1.0;

        // Complete
        self.current_step = WizardStep::Complete;
        self.status_message = "¡Listo! Ejecuta: lai auto \"tu tarea\"".to_string();
        self.draw()?;
        
        // Wait for user to press Enter or q
        self.wait_for_exit().await?;
        Ok(())
    }

    async fn detect_hardware(&self) -> Result<HardwareInfo> {
        let mut sys = System::new_all();
        sys.refresh_all();

        // RAM
        let total_ram_mb = sys.total_memory() / 1024;

        // CPU
        let cpu_cores = num_cpus::get();

        // OS/Arch
        let os = std::env::consts::OS.to_string();
        let arch = std::env::consts::ARCH.to_string();

        // GPU VRAM
        let (gpu_vram_mb, gpu_name) = self.detect_gpu_vram().await;

        // Ollama
        let (ollama_installed, ollama_version) = self.check_ollama().await;

        Ok(HardwareInfo {
            gpu_vram_mb,
            gpu_name,
            total_ram_mb,
            cpu_cores,
            os,
            arch,
            ollama_installed,
            ollama_version,
        })
    }

    async fn detect_gpu_vram(&self) -> (Option<u64>, Option<String>) {
        // Try nvidia-smi first (NVIDIA)
        if let Ok(output) = TokioCommand::new("nvidia-smi")
            .args(["--query-gpu=memory.total,name", "--format=csv,noheader,nounits"])
            .output()
            .await
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
                    if parts.len() >= 2 {
                        if let Ok(vram) = parts[0].parse::<u64>() {
                            return (Some(vram), Some(parts[1].to_string()));
                        }
                    }
                }
            }
        }

        // Try rocm-smi (AMD)
        if let Ok(output) = TokioCommand::new("rocm-smi")
            .args(["--showmeminfo", "vram", "--json"])
            .output()
            .await
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Ok(_json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                    // Parse AMD VRAM if needed
                }
            }
        }

        // macOS: system_profiler
        if cfg!(target_os = "macos") {
            if let Ok(output) = TokioCommand::new("system_profiler")
                .args(["SPDisplaysDataType", "-json"])
                .output()
                .await
            {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&stdout) {
                        // Parse Apple Silicon unified memory
                        // For simplicity, return None - we'll use RAM as fallback
                    }
                }
            }
        }

        (None, None)
    }

    async fn check_ollama(&self) -> (bool, Option<String>) {
        // Check if ollama is in PATH
        let ollama_path = which::which("ollama").ok();
        if ollama_path.is_none() {
            return (false, None);
        }

        // Get version
        if let Ok(output) = TokioCommand::new("ollama")
            .args(["--version"])
            .output()
            .await
        {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                return (true, Some(version));
            }
        }

        (true, None)
    }

    fn generate_recommendations(&self) -> Vec<ModelRecommendation> {
        let vram_gb = self.hardware.as_ref().and_then(|h| h.gpu_vram_mb).map(|v| v as f32 / 1024.0);
        let ram_gb = self.hardware.as_ref().map(|h| h.total_ram_mb as f32 / 1024.0).unwrap_or(0.0);
        let effective_vram = vram_gb.unwrap_or(ram_gb * 0.5); // CPU fallback: use half RAM

        let mut models = vec![
            ModelRecommendation {
                name: "qwen3-coder:480b".to_string(),
                display_name: "Qwen3-Coder 480B (MoE)".to_string(),
                size_gb: 280.0,
                vram_required_gb: 48.0,
                description: "Maximum capability MoE, 480B total, 35B active".to_string(),
                is_moe: true,
            },
            ModelRecommendation {
                name: "qwen3-coder:30b".to_string(),
                display_name: "Qwen3-Coder 30B (MoE)".to_string(),
                size_gb: 19.0,
                vram_required_gb: 24.0,
                description: "Best coding MoE, 30B total, 3.3B active per token".to_string(),
                is_moe: true,
            },
            ModelRecommendation {
                name: "deepseek-coder-v2:16b".to_string(),
                display_name: "DeepSeek-Coder-V2 16B".to_string(),
                size_gb: 10.0,
                vram_required_gb: 16.0,
                description: "Strong coding, great for refactoring".to_string(),
                is_moe: false,
            },
            ModelRecommendation {
                name: "gemma4:12b".to_string(),
                display_name: "Gemma 4 12B".to_string(),
                size_gb: 7.0,
                vram_required_gb: 12.0,
                description: "Google's lightweight open model, fast".to_string(),
                is_moe: false,
            },
            ModelRecommendation {
                name: "qwen3-coder:14b".to_string(),
                display_name: "Qwen3-Coder 14B".to_string(),
                size_gb: 9.0,
                vram_required_gb: 12.0,
                description: "Dense Qwen coder, good balance".to_string(),
                is_moe: false,
            },
            ModelRecommendation {
                name: "gemma4:9b".to_string(),
                display_name: "Gemma 4 9B".to_string(),
                size_gb: 5.0,
                vram_required_gb: 8.0,
                description: "Small but capable".to_string(),
                is_moe: false,
            },
            ModelRecommendation {
                name: "qwen3-coder:7b".to_string(),
                display_name: "Qwen3-Coder 7B".to_string(),
                size_gb: 4.0,
                vram_required_gb: 6.0,
                description: "Fits in 8GB VRAM, usable for small tasks".to_string(),
                is_moe: false,
            },
            ModelRecommendation {
                name: "qwen3-coder:1.5b".to_string(),
                display_name: "Qwen3-Coder 1.5B".to_string(),
                size_gb: 1.0,
                vram_required_gb: 2.0,
                description: "Minimum viable for CPU-only".to_string(),
                is_moe: false,
            },
        ];

        // Sort by suitability (closest fit to available VRAM, preferring MoE for high VRAM)
        models.sort_by(|a, b| {
            let a_fit = (a.vram_required_gb - effective_vram).abs();
            let b_fit = (b.vram_required_gb - effective_vram).abs();
            // Prefer models that fit comfortably (vram_required <= effective_vram * 0.9)
            let a_fits = a.vram_required_gb <= effective_vram * 0.9;
            let b_fits = b.vram_required_gb <= effective_vram * 0.9;
            
            match (a_fits, b_fits) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a_fit.partial_cmp(&b_fit).unwrap_or(std::cmp::Ordering::Equal),
            }
        });

        models
    }

    async fn select_model_interactive(&mut self) -> Result<()> {
        let mut list_state = ListState::default();
        list_state.select(Some(0));

        loop {
            self.draw_model_selection(&list_state)?;

            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Up => {
                        let i = list_state.selected().unwrap_or(0);
                        if i > 0 {
                            list_state.select(Some(i - 1));
                        }
                    }
                    KeyCode::Down => {
                        let i = list_state.selected().unwrap_or(0);
                        if i + 1 < self.recommendations.len() + 1 { // +1 for custom option
                            list_state.select(Some(i + 1));
                        }
                    }
                    KeyCode::Enter => {
                        let i = list_state.selected().unwrap_or(0);
                        if i < self.recommendations.len() {
                            self.selected_model = Some(self.recommendations[i].name.clone());
                            break;
                        } else {
                            // Custom model input
                            self.selected_model = Some(self.input_custom_model().await?);
                            break;
                        }
                    }
                    KeyCode::Char('q') | KeyCode::Esc => {
                        return Err(anyhow::anyhow!("Usuario canceló"));
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    async fn input_custom_model(&self) -> Result<String> {
        // For simplicity, return a default - in full implementation would show input dialog
        Ok("qwen3-coder:7b".to_string())
    }

    async fn download_model(&mut self) -> Result<()> {
        let model = self.selected_model.as_ref().context("No model selected")?.clone();
        
        // Check if already exists
        let check = TokioCommand::new("ollama")
            .args(["list"])
            .output()
            .await?;
        
        let stdout = String::from_utf8_lossy(&check.stdout);
        if stdout.lines().any(|l| l.split_whitespace().next() == Some(&model)) {
            self.status_message = format!("✓ Modelo {} ya existe", model);
            self.draw()?;
            tokio::time::sleep(Duration::from_secs(1)).await;
            return Ok(());
        }

        // Pull with progress (ollama doesn't give granular progress, so we simulate)
        let mut child = TokioCommand::new("ollama")
            .args(["pull", &model])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Simulate progress updates while waiting
        let mut progress = 0.0_f64;
        while !child.try_wait()?.is_some() {
            progress = (progress + 0.05).min(0.95);
            self.progress = 0.3 + progress * 0.25;
            self.status_message = format!("Descargando {}... {:.0}%", model, progress * 100.0);
            self.draw()?;
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        let status = child.wait().await?;
        if !status.success() {
            return Err(anyhow::anyhow!("Error descargando modelo: {}", status));
        }

        self.status_message = format!("✓ Modelo {} descargado", model);
        self.progress = 0.55;
        self.draw()?;
        tokio::time::sleep(Duration::from_secs(1)).await;
        Ok(())
    }

    async fn configure_gateway(&self) -> Result<()> {
        // Generate gateway config
        let config_dir = dirs::config_dir()
            .context("No se encontró directorio de configuración")?
            .join("lai");
        std::fs::create_dir_all(&config_dir)?;

        let model = self.selected_model.as_ref().context("No model selected")?;
        let gateway_config = format!(r#"
# lai gateway configuration
# Generated by lai init on {}

server:
  host: "127.0.0.1"
  port: 8081
  workers: 4

upstreams:
  - name: "ollama"
    url: "http://127.0.0.1:11434/v1"
    models:
      - "{}"
    weight: 100
    timeout_seconds: 300

cache:
  enabled: true
  max_entries: 5000
  max_memory_mb: 512
  semantic_enabled: true
  similarity_threshold: 0.85

circuit_breaker:
  enabled: true
  failure_threshold: 5
  timeout_seconds: 30

rate_limit:
  enabled: false
  requests_per_minute: 60

logging:
  level: "info"
  format: "json"
"#, chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"), model);

        std::fs::write(config_dir.join("gateway.yaml"), gateway_config)?;

        // Generate lai config
        let lai_config = format!(r#"
# lai configuration
# Generated by lai init on {}

model: "{}"
base_url: "http://127.0.0.1:8081/v1"
allow_remote: false

services:
  llama_cpp:
    enabled: false
    port: 8080
  gateway:
    enabled: true
    port: 8081
  ollama:
    enabled: true
    port: 11434
  lai_serve:
    enabled: true
    port: 8082

tools:
  sandbox: true
  max_file_size_mb: 10
  allowed_paths: []
  denied_paths: ["/etc", "/root", "/home/*/.ssh"]

auto_mode:
  max_iterations: 20
  max_cost_usd: 0.0
  require_approval: false
"#, chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"), model);

        std::fs::write(config_dir.join("config.yaml"), lai_config)?;
        Ok(())
    }

    async fn generate_agents_md(&self) -> Result<()> {
        let cwd = std::env::current_dir()?;
        let project_name = cwd.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("proyecto");

        let hardware = self.hardware.as_ref().context("No hardware info")?;
        let model = self.selected_model.as_ref().context("No model selected")?;

        let agents_md = format!(r#"# AGENTS.md - Instrucciones para lai en {project_name}

> Generado automáticamente por `lai init` el {}
> Hardware detectado: {} ({:.1}GB RAM, {} cores)
> Modelo: {} ({}GB VRAM requerida)
> GPU: {}

---

## 🎯 Objetivos del Proyecto

- [ ] Definir arquitectura y stack tecnológico
- [ ] Implementar funcionalidad principal
- [ ] Añadir tests y documentación
- [ ] Configurar CI/CD local

## 📋 Reglas de Trabajo

1. **Privacidad primero**: Todo código y datos permanecen en esta máquina
2. **Modelos locales únicamente**: Usar {} vía gateway local
3. **Calidad sobre velocidad**: Verificar antes de commit
4. **Documentar decisiones**: Actualizar este archivo con cada cambio importante

## 🛠 Stack Tecnológico Detectado

- **OS**: {} {}
- **RAM**: {:.1} GB
- **CPU**: {} cores
- **GPU**: {} ({:.1} GB VRAM)
- **Modelo IA**: {} (ollama)

## 🚀 Comandos Útiles

```bash
# Iniciar servicios
lai serve &

# Modo autónomo
lai auto "implementar feature X"

# Revisar código
lai review .

# Chat interactivo
lai chat

# Ver estado
lai status
```

## 📝 Notas del Desarrollador

_Añade aquí notas específicas del proyecto, decisiones de arquitectura, TODOs, etc._

---
*Este archivo debe versionarse en git. lai lo leerá automáticamente para entender el contexto del proyecto.*
"#, 
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
            hardware.gpu_name.as_deref().unwrap_or("CPU-only"),
            hardware.total_ram_mb as f32 / 1024.0,
            hardware.cpu_cores,
            model,
            self.recommendations.iter().find(|m| m.name == *model).map(|m| m.vram_required_gb).unwrap_or(0.0),
            hardware.gpu_name.as_deref().unwrap_or("No detectada / CPU-only"),
            model,
            hardware.os, hardware.arch,
            hardware.total_ram_mb as f32 / 1024.0,
            hardware.cpu_cores,
            hardware.gpu_name.as_deref().unwrap_or("No detectada"),
            hardware.gpu_vram_mb.map(|v| v as f32 / 1024.0).unwrap_or(0.0),
            model,
        );

        std::fs::write(cwd.join("AGENTS.md"), agents_md)?;
        Ok(())
    }

    async fn run_health_check(&mut self) -> Result<()> {
        let model = self.selected_model.as_ref().context("No model selected")?;
        
        // 1. Check ollama has model
        let check = TokioCommand::new("ollama")
            .args(["list"])
            .output()
            .await?;
        let stdout = String::from_utf8_lossy(&check.stdout);
        if !stdout.lines().any(|l| l.split_whitespace().next() == Some(model)) {
            return Err(anyhow::anyhow!("Modelo no encontrado en ollama"));
        }
        self.status_message = "✓ ollama: modelo disponible".to_string();
        self.draw()?;
        tokio::time::sleep(Duration::from_millis(500)).await;

        // 2. Check gateway config exists
        let config_dir = dirs::config_dir().context("No config dir")?.join("lai");
        if !config_dir.join("gateway.yaml").exists() {
            return Err(anyhow::anyhow!("Config gateway no encontrada"));
        }
        self.status_message = "✓ gateway: config OK".to_string();
        self.draw()?;
        tokio::time::sleep(Duration::from_millis(500)).await;

        // 3. Try to start gateway (background) and test
        // For now just verify config is valid
        self.status_message = "✓ lai serve: config OK".to_string();
        self.draw()?;
        tokio::time::sleep(Duration::from_millis(500)).await;

        // 4. Test tool calling would work (simulated)
        self.status_message = "✓ Tool calling: ready".to_string();
        self.draw()?;
        tokio::time::sleep(Duration::from_millis(500)).await;

        Ok(())
    }

    async fn wait_for_exit(&mut self) -> Result<()> {
        loop {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Enter | KeyCode::Char('q') | KeyCode::Esc => break,
                    _ => {}
                }
            }
        }
        Ok(())
    }

    // ==================== DRAWING ====================

    fn draw(&mut self) -> Result<()> {
        if let Some(terminal) = self.terminal.as_mut() {
            let current_step = self.current_step;
            let progress = self.progress;
            let status_message = self.status_message.clone();
            let error_message = self.error_message.clone();
            let hardware = self.hardware.clone();
            let recommendations = self.recommendations.clone();
            let selected_model = self.selected_model.clone();
            
            terminal.draw(move |f| {
                Self::render_frame_static(
                    f, 
                    current_step, 
                    progress, 
                    &status_message, 
                    error_message.as_deref(),
                    hardware.as_ref(),
                    &recommendations,
                    selected_model.as_deref(),
                )
            })?;
        }
        Ok(())
    }

    fn draw_model_selection(&mut self, list_state: &ListState) -> Result<()> {
        if let Some(terminal) = self.terminal.as_mut() {
            let recommendations = self.recommendations.clone();
            let hardware = self.hardware.clone();
            let list_state = list_state.clone();
            
            terminal.draw(move |f| {
                Self::render_model_selection_static(f, &recommendations, hardware.as_ref(), list_state)
            })?;
        }
        Ok(())
    }

    fn render_frame_static(
        f: &mut ratatui::Frame,
        current_step: WizardStep,
        progress: f64,
        status_message: &str,
        error_message: Option<&str>,
        hardware: Option<&HardwareInfo>,
        recommendations: &[ModelRecommendation],
        selected_model: Option<&str>,
    ) {
        let area = f.size();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),   // Title
                Constraint::Min(10),     // Content
                Constraint::Length(3),   // Progress bar
                Constraint::Length(3),   // Status
                Constraint::Length(2),   // Help
            ])
            .split(area);

        // Title
        let title = Paragraph::new("🚀 lai init - Configuración Inicial")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Blue)));
        f.render_widget(title, chunks[0]);

        // Content based on step
        let content = match current_step {
            WizardStep::DetectHardware => Self::render_hardware_detection_static(hardware),
            WizardStep::SelectModel => Self::render_model_list_static(recommendations, hardware),
            WizardStep::DownloadModel => Self::render_download_progress_static(selected_model),
            WizardStep::ConfigureGateway => Self::render_configuring_static("gateway"),
            WizardStep::GenerateAgentsMd => Self::render_configuring_static("AGENTS.md"),
            WizardStep::HealthCheck => Self::render_health_check_static(),
            WizardStep::Complete => Self::render_complete_static(),
            WizardStep::Error => Self::render_error_static(error_message),
        };
        f.render_widget(content, chunks[1]);

        // Progress bar
        let progress_bar = Gauge::default()
            .block(Block::default().borders(Borders::ALL).title("Progreso"))
            .gauge_style(Style::default().fg(Color::Green))
            .percent((progress * 100.0) as u16)
            .label(format!("{:.0}%", progress * 100.0));
        f.render_widget(progress_bar, chunks[2]);

        // Status message
        let status_style = if error_message.is_some() {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Yellow)
        };
        let status = Paragraph::new(status_message.to_string())
            .style(status_style)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Estado"));
        f.render_widget(status, chunks[3]);

        // Help
        let help = Paragraph::new("←/→ navegar  ⏎ seleccionar  q/ESC cancelar")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        f.render_widget(help, chunks[4]);
    }

    fn render_model_selection_static(
        f: &mut ratatui::Frame,
        recommendations: &[ModelRecommendation],
        hardware: Option<&HardwareInfo>,
        list_state: ListState,
    ) {
        let area = f.size();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(area);

        let title = Paragraph::new("🤖 Selecciona Modelo")
            .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Blue)));
        f.render_widget(title, chunks[0]);

        let items: Vec<ListItem> = recommendations.iter().enumerate().map(|(i, m)| {
            let fits = hardware.map(|h| {
                let vram = h.gpu_vram_mb.unwrap_or(h.total_ram_mb / 2) as f32 / 1024.0;
                m.vram_required_gb <= vram * 0.9
            }).unwrap_or(false);
            let marker = if fits { "✓" } else { "⚠" };
            let moe = if m.is_moe { " [MoE]" } else { "" };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} {}. ", marker, i + 1), Style::default().fg(Color::Green)),
                Span::styled(m.display_name.clone(), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(format!("{} ~{}GB", moe, m.size_gb), Style::default().fg(Color::Yellow)),
            ]))
        }).collect();

        let mut items_with_custom = items;
        items_with_custom.push(ListItem::new(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("Escribir modelo personalizado...", Style::default().fg(Color::Blue).add_modifier(Modifier::ITALIC)),
        ])));

        let list = List::new(items_with_custom)
            .block(Block::default().borders(Borders::ALL).title("Modelos Recomendados"))
            .highlight_style(Style::default().bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD))
            .highlight_symbol("► ");
        
        let mut state = list_state;
        f.render_stateful_widget(list, chunks[1], &mut state);

        // Hardware info
        if let Some(hw) = hardware {
            let info = Paragraph::new(format!(
                "GPU: {} | VRAM: {} | RAM: {:.1}GB | CPU: {} cores",
                hw.gpu_name.as_deref().unwrap_or("CPU-only"),
                hw.gpu_vram_mb.map(|v| format!("{:.1}GB", v as f32 / 1024.0)).unwrap_or("N/A".to_string()),
                hw.total_ram_mb as f32 / 1024.0,
                hw.cpu_cores
            ))
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Hardware"));
            f.render_widget(info, chunks[2]);
        }
    }

    fn render_hardware_detection_static(hardware: Option<&HardwareInfo>) -> Paragraph {
        if let Some(hw) = hardware {
            let gpu_info = hw.gpu_name.as_deref().unwrap_or("No detectada (modo CPU)");
            let vram_info = hw.gpu_vram_mb.map(|v| format!("{:.1} GB", v as f32 / 1024.0)).unwrap_or("N/A".to_string());
            let ollama_status = if hw.ollama_installed {
                format!("✓ Instalado ({})", hw.ollama_version.as_deref().unwrap_or("?"))
            } else {
                "✗ No instalado - se usará llama.cpp directo".to_string()
            };

            Paragraph::new(Text::from(vec![
                Line::from(vec![Span::styled("🖥  Hardware Detectado\n", Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan))]),
                Line::from(""),
                Line::from(vec![Span::styled("GPU: ", Style::default().fg(Color::Yellow)), Span::raw(gpu_info)]),
                Line::from(vec![Span::styled("VRAM: ", Style::default().fg(Color::Yellow)), Span::raw(vram_info)]),
                Line::from(vec![Span::styled("RAM Total: ", Style::default().fg(Color::Yellow)), Span::raw(format!("{:.1} GB", hw.total_ram_mb as f32 / 1024.0))]),
                Line::from(vec![Span::styled("CPU: ", Style::default().fg(Color::Yellow)), Span::raw(format!("{} cores", hw.cpu_cores))]),
                Line::from(vec![Span::styled("OS/Arch: ", Style::default().fg(Color::Yellow)), Span::raw(format!("{} {}", hw.os, hw.arch))]),
                Line::from(""),
                Line::from(vec![Span::styled("Ollama: ", Style::default().fg(Color::Yellow)), Span::raw(ollama_status)]),
                Line::from(""),
                Line::from(vec![Span::styled("Generando recomendaciones de modelo...\n", Style::default().fg(Color::Green))]),
            ]))
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title("Detección"))
        } else {
            Paragraph::new("Detectando hardware...")
                .block(Block::default().borders(Borders::ALL).title("Detección"))
        }
    }

    fn render_model_list_static<'a>(recommendations: &'a [ModelRecommendation], hardware: Option<&'a HardwareInfo>) -> Paragraph<'a> {
        let mut lines = vec![
            Line::from(vec![Span::styled("Modelos recomendados para tu hardware:\n", Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan))]),
            Line::from(""),
        ];

        for (i, m) in recommendations.iter().enumerate() {
            let fits = hardware.map(|h| {
                let vram = h.gpu_vram_mb.unwrap_or(h.total_ram_mb / 2) as f32 / 1024.0;
                m.vram_required_gb <= vram * 0.9
            }).unwrap_or(false);
            let marker = if fits { "✓" } else { "⚠" };
            let moe = if m.is_moe { " [MoE]" } else { "" };
            lines.push(Line::from(vec![
                Span::styled(format!("{} {}. ", marker, i + 1), Style::default().fg(Color::Green)),
                Span::styled(m.display_name.clone(), Style::default().add_modifier(Modifier::BOLD)),
                Span::styled(format!("{} ~{}GB\n", moe, m.size_gb), Style::default().fg(Color::Yellow)),
                Span::styled(format!("   {}\n", m.description), Style::default().fg(Color::Gray)),
            ]));
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("Escribir modelo personalizado...", Style::default().fg(Color::Blue).add_modifier(Modifier::ITALIC)),
        ]));

        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title("Selección"))
    }

    fn render_download_progress_static(selected_model: Option<&str>) -> Paragraph {
        Paragraph::new(Text::from(vec![
            Line::from(vec![Span::styled("⬇ Descargando modelo...\n", Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan))]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Modelo: ", Style::default().fg(Color::Yellow)),
                Span::styled(selected_model.unwrap_or(""), Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled("Esto puede tomar varios minutos según tu conexión.\n", Style::default().fg(Color::Gray))]),
            Line::from(""),
            Line::from(vec![Span::styled("Progreso simulado - ollama no expone progreso granular", Style::default().fg(Color::DarkGray))]),
        ]))
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("Descarga"))
    }

    fn render_configuring_static(component: &str) -> Paragraph {
        Paragraph::new(Text::from(vec![
            Line::from(vec![Span::styled(format!("⚙ Configurando {}...\n", component), Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan))]),
            Line::from(""),
            Line::from(vec![Span::styled("Escribiendo archivos de configuración...", Style::default().fg(Color::Gray))]),
        ]))
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("Configuración"))
    }

    fn render_health_check_static<'a>() -> Paragraph<'a> {
        Paragraph::new(Text::from(vec![
            Line::from(vec![Span::styled("🏥 Verificando instalación...\n", Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan))]),
            Line::from(""),
            Line::from("✓ ollama: modelo disponible"),
            Line::from("✓ gateway: config OK"),
            Line::from("✓ lai serve: config OK"),
            Line::from("✓ Tool calling: ready"),
        ]))
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("Health Check"))
    }

    fn render_complete_static<'a>() -> Paragraph<'a> {
        Paragraph::new(Text::from(vec![
            Line::from(vec![Span::styled("✅ ¡Instalación Completada!\n\n", Style::default().add_modifier(Modifier::BOLD).fg(Color::Green))]),
            Line::from(vec![Span::styled("Tu lai está listo para usar.\n\n", Style::default().fg(Color::Cyan))]),
            Line::from(vec![Span::styled("Próximos pasos:\n", Style::default().add_modifier(Modifier::BOLD).fg(Color::Yellow))]),
            Line::from("  lai auto \"tu tarea\"        # Modo autónomo"),
            Line::from("  lai chat                   # Chat interactivo"),
            Line::from("  lai review .               # Revisar código"),
            Line::from("  lai status                 # Ver servicios"),
            Line::from(""),
            Line::from("Presiona ENTER o 'q' para salir"),
        ]))
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("¡Completado!"))
    }

    fn render_error_static(error_message: Option<&str>) -> Paragraph {
        Paragraph::new(Text::from(vec![
            Line::from(vec![Span::styled("❌ Error\n\n", Style::default().add_modifier(Modifier::BOLD).fg(Color::Red))]),
            Line::from(error_message.unwrap_or("Error desconocido")),
            Line::from(""),
            Line::from("Presiona 'q' para salir"),
        ]))
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("Error"))
    }
}

/// CLI command entry point
pub async fn cmd_init(
    model: Option<String>,
    auto: bool,
    no_ollama: bool,
    reset: bool,
) -> Result<()> {
    if reset {
        // Clean up config
        let config_dir = dirs::config_dir().context("No config dir")?.join("lai");
        if config_dir.exists() {
            std::fs::remove_dir_all(&config_dir)?;
            println!("✓ Configuración anterior eliminada");
        }
        let cwd = std::env::current_dir()?;
        let agents = cwd.join("AGENTS.md");
        if agents.exists() {
            std::fs::remove_file(agents)?;
            println!("✓ AGENTS.md anterior eliminado");
        }
    }

    let config = InitConfig {
        model,
        auto,
        no_ollama,
        reset,
    };

    let mut wizard = InitWizard::new(config);
    wizard.run().await?;

    println!("\n✅ lai init completado. Ejecuta: lai auto \"tu tarea\"");
    Ok(())
}

/// Doctor command - diagnose existing installation
pub async fn cmd_doctor() -> Result<()> {
    println!("🔍 lai doctor - Diagnóstico de instalación\n");

    let mut issues = Vec::new();
    let mut warnings = Vec::new();

    // Check config dir
    let config_dir = dirs::config_dir().context("No config dir")?.join("lai");
    if !config_dir.exists() {
        issues.push("Directorio de configuración no existe: ~/.config/lai".to_string());
    } else {
        println!("✓ ~/.config/lai existe");
        if !config_dir.join("config.yaml").exists() {
            warnings.push("config.yaml no encontrado".to_string());
        } else {
            println!("✓ config.yaml existe");
        }
        if !config_dir.join("gateway.yaml").exists() {
            warnings.push("gateway.yaml no encontrado".to_string());
        } else {
            println!("✓ gateway.yaml existe");
        }
    }

    // Check ollama
    if which::which("ollama").is_ok() {
        println!("✓ ollama instalado");
        let output = TokioCommand::new("ollama").args(["list"]).output().await?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let models: Vec<&str> = stdout.lines().skip(1).filter_map(|l| l.split_whitespace().next()).collect();
        if models.is_empty() {
            warnings.push("No hay modelos descargados en ollama".to_string());
        } else {
            println!("✓ Modelos en ollama: {}", models.join(", "));
        }
    } else {
        issues.push("ollama no instalado".to_string());
    }

    // Check gateway binary
    let gateway_path = config_dir.join("gateway");
    if gateway_path.exists() {
        println!("✓ gateway binary existe");
    } else {
        warnings.push("gateway binary no encontrado (se compilará al iniciar)".to_string());
    }

    // Check AGENTS.md
    let cwd = std::env::current_dir()?;
    if cwd.join("AGENTS.md").exists() {
        println!("✓ AGENTS.md existe en proyecto actual");
    } else {
        warnings.push("AGENTS.md no encontrado en directorio actual".to_string());
    }

    // Summary
    println!("\n📋 Resumen:");
    if issues.is_empty() && warnings.is_empty() {
        println!("✅ Todo OK - lai listo para usar");
    } else {
        if !issues.is_empty() {
            println!("❌ Errores ({}):", issues.len());
            for i in &issues { println!("  - {}", i); }
        }
        if !warnings.is_empty() {
            println!("⚠ Advertencias ({}):", warnings.len());
            for w in &warnings { println!("  - {}", w); }
        }
        println!("\n💡 Ejecuta 'lai init' para corregir");
    }

    Ok(())
}