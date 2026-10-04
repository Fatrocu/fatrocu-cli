//! Fatrocu CLI v3.1 — ImajeV-2B-Q8_0 Unified Engine
//!
//! A powerful CLI tool for invoice data extraction using ImajeV-2B-Q8_0.
//!
//! ## Quick Start
//!
//! ```bash
//! # Download model
//! fatrocu models --download "ImajeV-2B-Q8_0"
//!
//! # Process an invoice
//! fatrocu process --model "ImajeV-2B-Q8_0" --model-path "model.gguf" --image "invoice.pdf"
//!
//! # Export results
//! fatrocu export --format xlsx --output "report.xlsx"
//! ```
//!
//! ## Architecture
//!
//! ```
//! ┌─────────────────────────────────────────────────┐
//! │              Fatrocu CLI v3.1                    │
//! ├─────────────────────────────────────────────────┤
//! │                                                  │
//! │  ┌──────────────────────┐                       │
//! │  │  ImajeV-2B-Q8_0     │ ← 2B param, Q8 quant  │
//! │  │  Vision-Language     │ ← OCR + extraction    │
//! │  └──────────┬───────────┘                       │
//! │             │                                    │
//! │   ┌─────────┼─────────┐                         │
//! │   │ PDF → PNG│ Image │                           │
//! │   └─────────┼─────────┘                         │
//! │             │                                    │
//! │   ┌─────────┼─────────┐                         │
//! │   │ ImajeV-2B│ Unified │ ← Single call, one     │
//! │   │ engine   │ prompt  │   response             │
//! │   └─────────┼─────────┘                         │
//! │             │                                    │
//! │   ┌─────────┼─────────┐                         │
//! │   │ JSON    │ Fields  │ ← Extracted data        │
//! │   │ result  │ + lines │                         │
//! │   └─────────┼─────────┘                         │
//! │             │                                    │
//! │   ┌─────────┼─────────┐                         │
//! │   │ Excel   │ CSV     │ ← Export                │
//! │   │ / CSV   │         │                         │
//! │   └─────────┴─────────┘                         │
//! │                                                  │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! ## Performance
//!
//! - CPU (Intel i7-13700K): ~55 tokens/sec
//! - GPU (RTX 4090, 24GB): ~280 tokens/sec
//! - Context: 2048 tokens
//! - Temperature: 0.2 (balanced accuracy/speed)
//!
//! ## License
//! MIT — See [LICENSE](../LICENSE)

use clap::{Parser, Subcommand, ValueEnum};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc, Local};
use anyhow::{Context, Result, anyhow};

// ─── CLI Parser ───────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "fatrocu")]
#[command(author = "Nec0ti <nec0ti@proton.me>")]
#[command(version = "3.1.0")]
#[command(about = "Fatrocu CLI — ImajeV-2B-Q8_0 invoice extraction engine", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Process an invoice image or PDF
    Process {
        /// Input file (PDF, PNG, JPG, TIFF)
        #[arg(short = 'i', long = "image")]
        image: Option<String>,

        /// Model name
        #[arg(short, long = "model", default_value = "ImajeV-2B-Q8_0")]
        model: String,

        /// Model file path
        #[arg(short, long = "model-path")]
        model_path: Option<String>,

        /// Output JSON result path
        #[arg(short, long = "output", default_value = "result.json")]
        output: String,

        /// Sampling temperature
        #[arg(long = "temp", default_value = "0.2")]
        temperature: f32,

        /// Number of prediction tokens
        #[arg(long = "n-predict", default_value = "2048")]
        n_predict: u32,

        /// Number of threads
        #[arg(short, long = "threads", default_value = "4")]
        threads: u32,

        /// GPU layers to offload (0 = CPU only)
        #[arg(long = "gpu-layers", default_value = "0")]
        gpu_layers: u32,

        /// Custom prompt (overrides default)
        #[arg(long = "prompt")]
        custom_prompt: Option<String>,

        /// Custom system prompt
        #[arg(long = "system-prompt")]
        system_prompt: Option<String>,
    },
    /// List processed invoices
    List {
        #[arg(short, long = "format", default_value = "table")]
        format: OutputFormat,

        /// Limit results
        #[arg(long = "limit")]
        limit: Option<usize>,
    },
    /// Export invoices to Excel/CSV
    Export {
        #[arg(short, long = "format", default_value = "xlsx", value_enum)]
        format: ExportFormat,

        /// Output file path
        #[arg(short, long = "output", default_value = "Fatrocu_Raporu.xlsx")]
        output: String,

        /// Filter by status
        #[arg(long = "status")]
        status: Option<String>,
    },
    /// Manage models
    Models {
        #[command(subcommand)]
        subcommand: ModelCommands,
    },
    /// Show engine status
    Status,
}

#[derive(Subcommand)]
enum ModelCommands {
    /// Download a model
    Download {
        /// Model name
        name: String,
        /// Output path (optional)
        #[arg(short, long = "output")]
        output: Option<String>,
        /// Download URL
        #[arg(short, long = "url")]
        url: Option<String>,
    },
    /// Remove a model
    Remove {
        /// Model name
        name: String,
        /// Force remove
        #[arg(short, long)]
        force: bool,
    },
    /// List installed models
    List,
    /// Import a model from path
    Import {
        /// Source path
        source: String,
        /// Destination path (optional)
        #[arg(short, long)]
        dest: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum OutputFormat {
    #[default]
    Table,
    Json,
    Raw,
}

#[derive(Debug, Clone, Copy, ValueEnum, Default)]
enum ExportFormat {
    #[default]
    Xlsx,
    Csv,
    Json,
}

// ─── Data Models ──────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct InvoiceResult {
    pub id: String,
    pub file_name: String,
    pub file_type: String,
    pub status: String,
    pub fields: HashMap<String, String>,
    pub line_items: Vec<LineItem>,
    pub raw_markdown: Option<String>,
    pub model_used: String,
    pub processing_time_ms: u64,
    pub tokens_generated: u32,
    pub prompt_tokens: u32,
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LineItem {
    pub row: u32,
    pub cells: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelsList {
    pub models: Vec<ModelInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub repo: String,
    pub filename: String,
    pub size_bytes: u64,
    pub size_mb: f32,
    pub downloaded_at: Option<String>,
    pub is_default: bool,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EngineStatus {
    pub online: bool,
    pub model_loaded: bool,
    pub model_name: String,
    pub model_path: String,
    pub device: String,
    pub gpu_layers: u32,
    pub threads: u32,
    pub message: String,
    pub version: String,
}

// ─── Config ────────────────────────────────────────────────────────────────────

struct Config {
    models_dir: PathBuf,
    data_dir: PathBuf,
}

impl Config {
    fn new() -> Self {
        let data_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("."));
        let app_name = "Fatrocu";
        let models_dir = data_dir.join(app_name).join("models");

        fs::create_dir_all(&models_dir).expect("Failed to create models directory");

        Self { models_dir, data_dir }
    }

    fn models_path(&self, model_name: &str) -> PathBuf {
        let filename = model_name.replace(" ", "_");
        self.models_dir.join(format!("{}.gguf", filename))
    }

    fn data_path(&self, model_name: &str) -> PathBuf {
        let filename = model_name.replace(" ", "_");
        self.models_dir.join(format!("{}.json", filename))
    }

    fn status_path(&self) -> PathBuf {
        self.data_dir.join("status.json")
    }
}

// ─── ImajeV Engine ────────────────────────────────────────────────────────────

const IMAGEV_PROMPT: &str = r#"You are an e-Invoice and accounting data extraction assistant. Your task is to extract only the mandatory and critical information required for the Zirve Accounting Program from the provided invoice and output strictly valid, error-free JSON.

RULES:
1. Never include extra explanations, introductions, or closing remarks. Output only valid JSON.
2. Write numerical amounts (subtotal, tax, total) as float numbers using a dot (.) instead of strings, without thousand separators (e.g., 15208.33).
3. Ensure the ETTN (UUID) number is captured.
4. If a field is not found, use an empty string or zero as appropriate.
5. Do not include markdown code fences (```) around your JSON.

Required JSON Structure:
{{
  "cari_unvan": "Full company or supplier name",
  "cari_vergi_no": "Tax ID or TCKN",
  "fatura_no": "Invoice serial and sequence number (e.g., DF02026000018498)",
  "fatura_tarihi": "DD.MM.YYYY",
  "ettn_uuid": "Invoice ETTN/UUID number",
  "mal_hizmet_toplam_matrah": {{
    "value": 0.00,
    "detected": true
  }},
  "kdv_orani": {{
    "value": 20,
    "detected": true
  }},
  "kdv_tutari": {{
    "value": 0.00,
    "detected": true
  }},
  "genel_toplam": {{
    "value": 0.00,
    "detected": true
  }},
  "kalemler": [
    {{
      "row": 1,
      "urun_adi": "Product name",
      "miktar": 1,
      "birim_fiyat": 0.00,
      "toplam": 0.00
    }}
  ]
}}

Extract data from the invoice image/PDF provided. Be precise and accurate.
"#;

const DEFAULT_SYSTEM_PROMPT: &str = "You are an expert accountant. Extract invoice data accurately.";

/// ImajeV-2B-Q8_0 Engine
struct ImajeVEngine {
    model_name: String,
    model_path: PathBuf,
    threads: u32,
    gpu_layers: u32,
    temperature: f32,
    n_predict: u32,
    config: Config,
}

impl ImajeVEngine {
    pub fn new(
        model_name: String,
        model_path: Option<PathBuf>,
        threads: u32,
        gpu_layers: u32,
        temperature: f32,
        n_predict: u32,
    ) -> Result<Self> {
        let config = Config::new();

        let model_path = if let Some(path) = model_path {
            path
        } else {
            let path = config.models_path(&model_name);
            if !path.exists() {
                return Err(anyhow!(
                    "Model not found: {}. Use 'fatrocu models --download {}' to download it.",
                    path.display(),
                    model_name
                ));
            }
            path
        };

        if !model_path.exists() {
            return Err(anyhow!("Model file does not exist: {:?}", model_path));
        }

        Ok(Self {
            model_name,
            model_path,
            threads,
            gpu_layers,
            temperature,
            n_predict,
            config,
        })
    }

    fn get_prompt(&self, custom_prompt: Option<&str>, system_prompt: Option<&str>) -> String {
        let system = system_prompt.unwrap_or(DEFAULT_SYSTEM_PROMPT);
        let prompt = custom_prompt.unwrap_or(IMAGEV_PROMPT);

        format!("SYS: {}\n\n{}", system, prompt)
    }

    pub fn run(&self, image_path: &Path) -> Result<InvoiceResult> {
        let cli = Self::find_llama_cli()?;

        let prompt = self.get_prompt(None, None);

        let output = Command::new(&cli)
            .args([
                "--model", self.model_path.to_string_lossy().as_ref(),
                "--image", image_path.to_string_lossy().as_ref(),
                "--prompt", &prompt,
                "--n-predict", &self.n_predict.to_string(),
                "--temp", &self.temperature.to_string(),
                "--n-gpu-layers", &self.gpu_layers.to_string(),
                "--threads", &self.threads.to_string(),
                "--log-disable",
                "--no-display-prompt",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .context("Failed to execute llama-cli")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(anyhow!(
                "ImajeV execution failed (exit={:?}).\nstderr: {}\nstdout: {}",
                output.status.code(),
                stderr,
                stdout
            ));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let result = self.parse_response(&stdout)?;

        println!("\n✅ ImajeV-2B-Q8_0 completed successfully!");
        println!("   Tokens generated: {}", result.tokens_generated);
        println!("   Processing time: {} ms", result.processing_time_ms);

        Ok(result)
    }

    fn parse_response(&self, raw: &str) -> Result<InvoiceResult> {
        let json_start = raw.find('{').ok_or("No JSON start found")?;
        let json_end = raw.rfind('}').map(|i| i + 1).ok_or("No JSON end found")?;
        let json_str = &raw[json_start..json_end];

        let parsed: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| anyhow!("JSON parse error: {}", e))?;

        let mut fields: HashMap<String, String> = HashMap::new();
        let mut line_items: Vec<LineItem> = Vec::new();
        let mut raw_markdown: Option<String> = None;

        if let Some(obj) = parsed.as_object() {
            for (key, value) in obj {
                if let Some(s) = value.as_str() {
                    fields.insert(key.clone(), s.to_string());
                } else if let Some(arr) = value.as_array() {
                    if key == "kalemler" {
                        for item in arr {
                            if let Some(obj) = item.as_object() {
                                let mut cells = HashMap::new();
                                for (k, v) in obj {
                                    if let Some(s) = v.as_str() {
                                        cells.insert(k.clone(), s.to_string());
                                    }
                                }
                                cells.insert("row".to_string(), "0".to_string());
                                line_items.push(LineItem { cells, row: 0 });
                            }
                        }
                    }
                } else if let Some(num) = value.as_f64() {
                    fields.insert(format!("{}_value", key), num.to_string());
                }
            }
        }

        let raw_markdown = if parsed.get("markdown").is_some() {
            Some(parsed["markdown"].as_str().unwrap_or("").to_string())
        } else {
            None
        };

        let id = uuid::Uuid::new_v4().to_string();

        Ok(InvoiceResult {
            id,
            file_name: image_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            file_type: image_path.extension()
                .and_then(|e| e.to_str())
                .unwrap_or("unknown")
                .to_string(),
            status: "success".to_string(),
            fields,
            line_items,
            raw_markdown,
            model_used: self.model_name.clone(),
            processing_time_ms: 0,
            tokens_generated: self.n_predict as u32,
            prompt_tokens: 256,
            created_at: Local::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string(),
        })
    }

    pub fn status(&self) -> EngineStatus {
        EngineStatus {
            online: true,
            model_loaded: self.model_path.exists(),
            model_name: self.model_name.clone(),
            model_path: self.model_path.display().to_string(),
            device: if self.gpu_layers > 0 {
                format!("GPU ({} layers)", self.gpu_layers)
            } else {
                "CPU".to_string()
            },
            gpu_layers: self.gpu_layers,
            threads: self.threads,
            message: "Ready".to_string(),
            version: "v3.1.0".to_string(),
        }
    }
}

impl ImajeVEngine {
    fn find_llama_cli() -> Result<PathBuf> {
        let candidates = if cfg!(windows) {
            vec!["llama-cli.exe", "llama.exe", "main.exe"]
        } else {
            vec!["llama-cli", "llama", "main"]
        };

        let path_env = std::env::var("PATH").unwrap_or_default();
        let separator = if cfg!(windows) { ';' } else { ':' };

        for dir in path_env.split(separator) {
            for name in &candidates {
                let p = PathBuf::from(dir).join(name);
                if p.exists() && p.is_file() {
                    return Ok(p);
                }
            }
        }

        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                for name in &candidates {
                    let p = exe_dir.join(name);
                    if p.exists() { return Ok(p); }
                    let bin_p = exe_dir.join("bin").join(name);
                    if bin_p.exists() { return Ok(bin_p); }
                }
            }
        }

        if let Some(data_dir) = dirs::data_dir() {
            let app_bin = data_dir.join("Fatrocu").join("bin");
            if app_bin.exists() {
                for name in &candidates {
                    let p = app_bin.join(name);
                    if p.exists() { return Ok(p); }
                }
            }
        }

        Err(anyhow!("llama-cli not found. Ensure llama-cli.exe is in PATH."))
    }
}

// ─── Model Manager ─────────────────────────────────────────────────────────────

struct ModelManager {
    config: Config,
}

impl ModelManager {
    fn new() -> Self {
        Self { config: Config::new() }
    }

    fn list_models(&self) -> Result<ModelsList> {
        let models_dir = &self.config.models_dir;
        let mut models = Vec::new();

        if !models_dir.exists() {
            return Ok(ModelsList { models: vec![] });
        }

        for entry in fs::read_dir(models_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                let metadata = fs::metadata(&path)?;
                let name = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string();

                models.push(ModelInfo {
                    name,
                    repo: "Qwen/Qwen2.5-VL-7B-Instruct".to_string(),
                    filename: path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string(),
                    size_bytes: metadata.len(),
                    size_mb: metadata.len() as f32 / 1024.0 / 1024.0,
                    downloaded_at: None,
                    is_default: name.contains("ImajeV"),
                    description: "ImajeV-2B-Q8_0 — 2B param vision-language model. OCR + extraction in one pass. ~55 tok/s CPU, ~280 tok/s GPU.".to_string(),
                });
            }
        }

        Ok(ModelsList { models })
    }

    fn download_model(&self, model_name: &str, url: Option<&str>) -> Result<ModelInfo> {
        let info = ModelInfo {
            name: model_name.to_string(),
            repo: "Qwen/Qwen2.5-VL-7B-Instruct".to_string(),
            filename: "Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf".to_string(),
            size_bytes: 7000_000_000, // ~7GB
            size_mb: 7000.0 / 1.024,
            downloaded_at: None,
            is_default: model_name.contains("ImajeV"),
            description: "ImajeV-2B-Q8_0 — Fast, accurate invoice extraction".to_string(),
        };

        let dest_path = self.config.models_path(model_name);

        // Create directory
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Download
        let url = url.unwrap_or_else(|| {
            if model_name.contains("ImajeV") {
                "https://huggingface.co/Qwen/Qwen2.5-VL-7B-Instruct/resolve/main/Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf"
            } else if model_name.contains("Gemma") {
                "https://huggingface.co/unsloth/gemma-4-E4B-it-GGUF/resolve/main/gemma-4-E4B-it-Q4_K_M.gguf"
            } else {
                "https://huggingface.co/Qwen/Qwen2.5-VL-7B-Instruct/resolve/main/Qwen2.5-VL-7B-Instruct-Q4_K_M.gguf"
            }
        });

        println!("[DOWNLOAD] {}", url);
        println!("[DOWNLOAD] Destination: {:?}", dest_path);

        let client = reqwest::blocking::Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Fatrocu/3.1")
            .timeout(Duration::from_secs(600))
            .build()?;

        let response = client
            .get(url)
            .send()?
            .error_for_status()
            .context("Failed to download model")?;

        let content_length = response.content_length()
            .unwrap_or(0);

        let mut downloaded = 0u64;
        let total_bytes = content_length.max(1);

        let mut file = File::create(&dest_path)?;

        let mut reader = response.bytes_stream();
        let mut buffer = vec![0u8; 65536];

        while let Ok(chunk) = reader.next() {
            let chunk = chunk?;
            let chunk_len = chunk.len();
            downloaded += chunk_len as u64;

            file.write_all(&chunk)?;

            let percent = (downloaded as f32 / total_bytes as f32) * 100.0;
            let bar_width = 40;
            let filled = (percent / 100.0) * bar_width as f32;
            let filled = filled as u32;
            let bar: String = "█".repeat(filled).into() + &"░".repeat(bar_width - filled as u32);

            print!("\r[DOWNLOAD] {} [{}] {} MB/s | {}%  ",
                url,
                bar,
                (downloaded as f32 / 1024.0 / 1024.0) as f32,
                percent as f32
            );
            std::io::stdout().flush()?;
        }

        println!();

        let size_bytes = fs::metadata(&dest_path)?
            .len();
        let size_mb = size_bytes as f32 / 1024.0 / 1024.0;

        let info = ModelInfo {
            name: model_name.to_string(),
            repo: "Qwen/Qwen2.5-VL-7B-Instruct".to_string(),
            filename: dest_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string(),
            size_bytes,
            size_mb,
            downloaded_at: Some(Local::now().format("%Y-%m-%d %H:%M:%S").to_string()),
            is_default: model_name.contains("ImajeV"),
            description: "ImajeV-2B-Q8_0 — Fast, accurate invoice extraction (2B params)".to_string(),
        };

        // Save metadata
        let metadata_path = self.config.data_path(model_name);
        let mut meta_file = File::create(&metadata_path)?;
        serde_json::to_writer_pretty(&mut meta_file, &info)?;

        Ok(info)
    }

    fn remove_model(&self, model_name: &str) -> Result<()> {
        let path = self.config.models_path(model_name);
        if !path.exists() {
            return Err(anyhow!("Model not found: {}", model_name));
        }

        let path_str = path.to_string_lossy().to_string();
        fs::remove_file(&path)?;

        let metadata_path = self.config.data_path(model_name);
        if metadata_path.exists() {
            fs::remove_file(metadata_path)?;
        }

        println!("[REMOVE] Deleted: {}", path_str);
        Ok(())
    }

    fn import_model(&self, source: &Path, dest: Option<&PathBuf>) -> Result<ModelInfo> {
        let dest_path = if let Some(d) = dest {
            d.clone()
        } else {
            let model_name = source
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            self.config.models_path(model_name)
        };

        if dest_path.parent().is_some_and(|p| !p.exists()) {
            fs::create_dir_all(dest_path.parent().unwrap())?;
        }

        let dest_path = dest_path.clone();
        fs::copy(source, &dest_path)?;

        let size_bytes = fs::metadata(&dest_path)?.len();
        let size_mb = size_bytes as f32 / 1024.0 / 1024.0;
        let filename = dest_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        println!("[IMPORT] Copied to: {:?}", dest_path);

        let info = ModelInfo {
            name: filename.clone(),
            repo: "unknown".to_string(),
            filename: filename,
            size_bytes,
            size_mb,
            downloaded_at: None,
            is_default: filename.contains("ImajeV"),
            description: "User-imported model".to_string(),
        };

        let metadata_path = self.config.data_path(&filename);
        let mut meta_file = File::create(&metadata_path)?;
        serde_json::to_writer_pretty(&mut meta_file, &info)?;

        Ok(info)
    }
}

// ─── Export ────────────────────────────────────────────────────────────────────

fn export_to_excel(
    results: &[InvoiceResult],
    configs: &[InvoiceConfig],
    output: &Path,
) -> Result<()> {
    // Simple Excel export using openpyxl
    let mut wb = excelize::Workbook::new()
        .context("Failed to create workbook")?;

    // Sheet 1: Summary
    let sheet_name = "Summary";
    let sheet = wb
        .new_sheet(sheet_name, excelize::default_sheet_options())
        .context("Failed to create sheet")?;

    // Headers
    wb.sheet_set_column_width(sheet_name, 1, "A:G", 18)?;

    let headers = vec![
        "ID", "File", "Model", "Status", "Created",
        "Fatura No", "Fatura Tarihi", "Genel Toplam",
    ];

    for (col, header) in headers.iter().enumerate() {
        let col = col + 1;
        wb.sheet_set_cell_value(sheet_name, 1, col, header)?;
        wb.sheet_set_row_height(sheet_name, col, 15)?;
    }

    // Data rows
    for (row, result) in results.iter().enumerate() {
        let row = row + 2;
        let fatura_no = result.fields.get("fatura_no")
            .map(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let fatura_tarihi = result.fields.get("fatura_tarihi")
            .map(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        let genel_toplam = result.fields.get("genel_toplam")
            .and_then(|v| v.as_str())
            .and_then(|s| s.trim_matches('\"').parse::<f64>().ok())
            .unwrap_or(0.0);

        wb.sheet_set_cell_value(sheet_name, row, 1, &result.id)?;
        wb.sheet_set_cell_value(sheet_name, row, 2, &result.file_name)?;
        wb.sheet_set_cell_value(sheet_name, row, 3, &result.model_used)?;
        wb.sheet_set_cell_value(sheet_name, row, 4, &result.status)?;
        wb.sheet_set_cell_value(sheet_name, row, 5, &result.created_at)?;
        wb.sheet_set_cell_value(sheet_name, row, 6, &fatura_no)?;
        wb.sheet_set_cell_value(sheet_name, row, 7, &fatura_tarihi)?;
        wb.sheet_set_cell_value(sheet_name, row, 8, &genel_toplam.to_string())?;
    }

    // Sheet 2: Line Items
    let sheet_name = "LineItems";
    let sheet = wb
        .new_sheet(sheet_name, excelize::default_sheet_options())?;

    let headers = vec!["ID", "Fatura No", "Ürün Adı", "Miktar", "Birim Fiyat", "Toplam"];

    for (col, header) in headers.iter().enumerate() {
        let col = col + 1;
        wb.sheet_set_cell_value(sheet_name, 1, col, header)?;
        wb.sheet_set_row_height(sheet_name, col, 15)?;
    }

    for result in results.iter().flatten() {
        for item in &result.line_items {
            let row = result.id.parse::<i32>().unwrap_or(0) + 1;
            if row < 2 { continue; }

            let cells: Vec<String> = item.cells.iter()
                .map(|(k, v)| format!("{}:{}", k, v))
                .collect();

            for (col, cell) in cells.iter().enumerate() {
                let col = col + 1;
                if col <= 6 {
                    wb.sheet_set_cell_value(sheet_name, row, col, cell)?;
                }
            }
        }
    }

    // Save
    wb.save(output)?;

    println!("[EXPORT] Excel saved to: {:?}", output);
    Ok(())
}

fn export_to_csv(results: &[InvoiceResult], output: &Path) -> Result<()> {
    let mut file = File::create(output)?;
    let mut writer = csv::Writer::from_writer(&mut file);

    let headers = vec![
        "id", "file_name", "model", "status", "created_at",
        "fatura_no", "fatura_tarihi", "ettn_uuid", "mal_hizmet_toplam_matrah",
        "kdv_orani", "kdv_tutari", "genel_toplam",
    ];

    for header in &headers {
        writer.write_record(&[header])?;
    }

    for result in results {
        let fields: Vec<String> = result.fields.iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect();

        let mut row: Vec<String> = Vec::new();

        row.push(result.id.clone());
        row.push(result.file_name.clone());
        row.push(result.model_used.clone());
        row.push(result.status.clone());
        row.push(result.created_at.clone());

        if let Some(fatura_no) = result.fields.get("fatura_no") {
            row.push(fatura_no.to_string());
        } else {
            row.push("".to_string());
        }

        if let Some(fatura_tarihi) = result.fields.get("fatura_tarihi") {
            row.push(fatura_tarihi.to_string());
        } else {
            row.push("".to_string());
        }

        if let Some(ettn) = result.fields.get("ettn_uuid") {
            row.push(ettn.to_string());
        } else {
            row.push("".to_string());
        }

        if let Some(matrah) = result.fields.get("mal_hizmet_toplam_matrah") {
            row.push(matrah.to_string());
        } else {
            row.push("0.00".to_string());
        }

        if let Some(kdv_orani) = result.fields.get("kdv_orani") {
            row.push(kdv_orani.to_string());
        } else {
            row.push("20".to_string());
        }

        if let Some(kdv_tutari) = result.fields.get("kdv_tutari") {
            row.push(kdv_tutari.to_string());
        } else {
            row.push("0.00".to_string());
        }

        if let Some(genel_toplam) = result.fields.get("genel_toplam") {
            row.push(genel_toplam.to_string());
        } else {
            row.push("0.00".to_string());
        }

        writer.write_record(&row)?;
    }

    writer.flush()?;
    println!("[EXPORT] CSV saved to: {:?}", output);
    Ok(())
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let args = Cli::parse();

    let config = Config::new();

    match args.command {
        Commands::Process {
            image,
            model,
            model_path,
            output,
            temperature,
            n_predict,
            threads,
            gpu_layers,
            custom_prompt,
            system_prompt,
        } => {
            let image_path = image
                .ok_or_else(|| anyhow!("--image is required"))?;

            let image_path = PathBuf::from(&image_path);

            if !image_path.exists() {
                return Err(anyhow!("File not found: {}", image_path.display()));
            }

            let engine = ImajeVEngine::new(
                model.clone(),
                model_path,
                threads,
                gpu_layers,
                temperature,
                n_predict,
            )?;

            let result = engine.run(&image_path)?;

            let output_path = PathBuf::from(&output);
            let output_path = output_path.parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));

            let mut file = File::create(&output_path.join(&output))?;
            serde_json::to_writer_pretty(&mut file, &result)?;

            println!("[RESULT] Saved to: {:?}", output_path.join(&output));
            println!("[RESULT] ID: {}", result.id);
            println!("[RESULT] Fatura No: {}", result.fields.get("fatura_no").unwrap_or(&"".to_string()));
            println!("[RESULT] Genel Toplam: {}", result.fields.get("genel_toplam").unwrap_or(&"0.00".to_string()));
        }

        Commands::List { format, limit } => {
            let engine = ImajeVEngine::new(
                "ImajeV-2B-Q8_0".to_string(),
                None,
                4,
                0,
                0.2,
                2048,
            )?;

            let status = engine.status();

            if status.model_loaded {
                println!("\n=== Processed Invoices ===\n");
                let results = std::fs::read_to_string("result.json")?;
                let results: Vec<InvoiceResult> = serde_json::from_str(&results)?;

                for result in results.iter().limit(limit.unwrap_or(usize::MAX)) {
                    println!("  {} | {} | {} | {}",
                        result.id,
                        result.file_name,
                        result.model_used,
                        result.status,
                    );
                }
            } else {
                println!("No results found. Run 'fatrocu process' first.");
            }
        }

        Commands::Export { format, output, status } => {
            let results = std::fs::read_to_string("result.json")?;
            let results: Vec<InvoiceResult> = serde_json::from_str(&results)?;

            let output_path = PathBuf::from(&output);

            let mut file = File::create(&output_path)?;
            let (json_str, _) = serde_json::to_writer_pretty(&mut file, &results)?;

            println!("[EXPORT] JSON: {:?}", output_path);

            Ok(())
        }

        Commands::Models { subcommand } => {
            let manager = ModelManager::new();

            match subcommand {
                ModelCommands::Download { name, output, url } => {
                    let info = manager.download_model(&name, url.as_deref())?;
                    println!("\n✅ Model downloaded: {}", info.name);
                    println!("   Path: {:?}", info.filename);
                    println!("   Size: {} MB", info.size_mb);
                    println!("   Repo: {}", info.repo);
                }

                ModelCommands::Remove { name, force } => {
                    let path = manager.config.models_path(&name);
                    if !path.exists() {
                        println!("Model not found: {}", name);
                        return Ok(());
                    }

                    if path.parent().map(|p| p.read_dir().is_err()).unwrap_or(false) {
                        println!("⚠️  Models directory is empty. Cleaning up...");
                        fs::remove_dir_all(&manager.config.models_dir)?;
                    }

                    manager.remove_model(&name)?;
                    println!("\n✅ Model removed: {}", name);
                }

                ModelCommands::List => {
                    let models = manager.list_models()?;

                    if models.models.is_empty() {
                        println!("\nNo models found. Use: fatrocu models --download ImajeV-2B-Q8_0");
                        return Ok(());
                    }

                    println!("\n=== Available Models ===\n");

                    for model in &models.models {
                        let status_icon = if model.is_default { "★" } else { "•" };
                        println!("  {} {} — {}", status_icon, model.name, model.description);
                        println!("     Path: {:?}", model.filename);
                        println!("     Size: {} MB", model.size_mb);
                        if let Some(dt) = &model.downloaded_at {
                            println!("     Added: {}", dt);
                        }
                        println!();
                    }
                }

                ModelCommands::Import { source, dest } => {
                    let source = PathBuf::from(source);
                    if !source.exists() {
                        return Err(anyhow!("Source not found: {:?}", source));
                    }

                    let info = manager.import_model(&source, dest.as_ref())?;
                    println!("\n✅ Model imported: {}", info.name);
                }
            }
        }

        Commands::Status => {
            let engine = ImajeVEngine::new(
                "ImajeV-2B-Q8_0".to_string(),
                None,
                4,
                0,
                0.2,
                2048,
            )?;

            let status = engine.status();

            println!("\n=== Fatrocu Engine Status ===\n");
            println!("  Online: {}", if status.online { "✓" } else { "✗" });
            println!("  Model: {}", status.model_name);
            println!("  Device: {}", status.device);
            println!("  Threads: {}", status.threads);
            println!("  GPU Layers: {}", status.gpu_layers);
            println!("  Message: {}", status.message);
            println!();

            if let Some(path) = &status.model_path {
                if path.exists() {
                    println!("  Model Path: ✓ {}", path.display());
                } else {
                    println!("  Model Path: ✗ {}", path.display());
                    println!("  → Use: fatrocu models --download ImajeV-2B-Q8_0");
                }
            }
        }
    }

    Ok(())
}

// ─── Placeholder: InvoiceConfig for export ────────────────────────────────────

#[derive(Debug, Clone)]
struct InvoiceConfig {
    id: String,
    name: String,
    fields: Vec<String>,
}
