//! Fatrocu CLI v3.1 — Moondream 3.1-9B-A2B Unified Model
//!
//! Usage:
//!   fatrocu process --model "Moondream 3.1-9B-A2B" --model-path "model.gguf" --image "invoice.pdf"
//!   fatrocu list
//!   fatrocu models --list
//!   fatrocu models --download "Moondream 3.1-9B-A2B"
//!   fatrocu models --remove "Moondream 3.1-9B-A2B"

use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::process::{Command, Stdio};

// ─── CLI Parser ───────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "fatrocu")]
#[command(author = "Nec0ti")]
#[command(version = "3.1.0")]
#[command(about = "Fatrocu CLI — Moondream 3.1 powered invoice processing", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Process an invoice image/PDF using Moondream 3.1-9B-A2B
    Process {
        /// Image or PDF path
        #[arg(short = 'i', long = "image")]
        image: Option<String>,

        /// Model name (Moondream 3.1-9B-A2B, or custom)
        #[arg(short = 'm', long = "model", default_value = "Moondream 3.1-9B-A2B")]
        model: String,

        /// Model file path
        #[arg(short, long = "model-path")]
        model_path: Option<String>,

        /// Invoice configuration
        #[arg(short, long = "config", default_value = "predefined")]
        config: String,

        /// Output JSON result path
        #[arg(short, long = "output", default_value = "result.json")]
        output: String,

        /// Temperature
        #[arg(long = "temp", default_value = "0.2")]
        temperature: f32,

        /// Prediction tokens
        #[arg(long = "n-predict", default_value = "2048")]
        n_predict: u32,

        /// Number of threads
        #[arg(short, long = "threads", default_value = "4")]
        threads: u32,

        /// GPU layers (0 = CPU only)
        #[arg(long = "gpu-layers", default_value = "0")]
        gpu_layers: u32,

        /// Custom prompt
        #[arg(long = "prompt")]
        prompt: Option<String>,
    },
    /// List processed invoices
    List {
        #[arg(short, long = "format", default_value = "table")]
        format: ValueEnum,
    },
    /// Export invoices to Excel/CSV
    Export {
        #[arg(short, long = "format", default_value = "xlsx")]
        format: ValueEnum,
        #[arg(short, long = "output", default_value = "Fatrocu_Raporu.xlsx")]
        output: String,
    },
    /// Manage models
    Models {
        #[command(subcommand)]
        subcommand: ModelCommands,
    },
    /// Show engine status
    Status,
    /// List available models
    ListModels,
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
    },
    /// Remove a model
    Remove {
        name: String,
    },
    /// List models
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
    pub created_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LineItem {
    pub row: u32,
    pub cells: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelsList {
    models: Vec<ModelInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModelInfo {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub downloaded_at: String,
}

// ─── Moondream Engine ─────────────────────────────────────────────────────────

pub struct MoondreamEngine {
    model_name: String,
    model_path: PathBuf,
    threads: u32,
    gpu_layers: u32,
    temp: f32,
    n_predict: u32,
}

impl MoondreamEngine {
    pub fn new(
        model_name: String,
        model_path: PathBuf,
        threads: u32,
        gpu_layers: u32,
        temp: f32,
        n_predict: u32,
    ) -> Self {
        Self {
            model_name,
            model_path,
            threads,
            gpu_layers,
            temp,
            n_predict,
        }
    }

    fn find_llama_cli() -> Option<PathBuf> {
        let candidates = if cfg!(windows) {
            vec!["llama-cli.exe", "llama.exe", "main.exe"]
        } else {
            vec!["llama-cli", "llama", "main"]
        };

        let path_var = std::env::var("PATH").unwrap_or_default();
        let separator = if cfg!(windows) { ';' } else { ':' };
        for dir in path_var.split(separator) {
            for name in &candidates {
                let p = PathBuf::from(dir).join(name);
                if p.exists() {
                    return Some(p);
                }
            }
        }

        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                for name in &candidates {
                    let p = exe_dir.join(name);
                    if p.exists() { return Some(p); }
                    let bin_p = exe_dir.join("bin").join(name);
                    if bin_p.exists() { return Some(bin_p); }
                }
            }
        }

        if let Some(data_dir) = dirs::data_dir() {
            let app_bin = data_dir.join("Fatrocu").join("bin");
            if app_bin.exists() {
                for name in &candidates {
                    let p = app_bin.join(name);
                    if p.exists() { return Some(p); }
                }
            }
        }

        None
    }

    fn find_model_file(&self) -> Option<PathBuf> {
        let mut search_dirs = Vec::new();
        search_dirs.push(PathBuf::from("gguf"));
        search_dirs.push(PathBuf::from("models"));

        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                search_dirs.push(exe_dir.join("gguf"));
                search_dirs.push(exe_dir.join("models"));
            }
        }

        if let Some(data_dir) = dirs::data_dir() {
            search_dirs.push(data_dir.join("Fatrocu").join("models"));
            search_dirs.push(data_dir.join("Fatrocu").join("gguf"));
        }

        let lower_name = self.model_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_lowercase();

        for dir in &search_dirs {
            if dir.exists() {
                if let Ok(entries) = std::fs::read_dir(dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().map(|e| e == "gguf").unwrap_or(false) {
                            let filename = path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("")
                                .to_lowercase();
                            if filename.contains(&lower_name) {
                                return Some(path);
                            }
                        }
                    }
                }
            }
        }

        self.model_path.clone()
    }

    pub fn run(&self, image_path: &PathBuf) -> Result<String, String> {
        let cli = Self::find_llama_cli()
            .ok_or_else(|| "llama-cli not found. Run: fatrocu models --download Moondream 3.1-9B-A2B")?;

        let resolved_path = self.find_model_file();
        let model_path = resolved_path.as_ref().map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| self.model_path.to_string_lossy().to_string());

        if !std::path::Path::new(&model_path).exists() {
            return Err(format!("Model file not found: {}", model_path));
        }

        let image_path_str = image_path.to_string_lossy().to_string();

        let prompt = format!(
            r#"<image>
You are a precise invoice data extraction assistant. Extract ALL fields from this invoice document.

Return a single valid JSON object (no markdown code blocks, no explanations) with this EXACT structure:

{{
  "fields": {{
    "fatura_no": "extracted invoice number or empty string",
    "fatura_tarihi": "extracted date in DD.MM.YYYY format, or empty string",
    "vkn": "extracted VAT number or empty string",
    "maliyet_toplam": "total cost amount, or empty string",
    "kdv_toplam": "total VAT amount, or empty string",
    "toplam_tutar": "total amount, or empty string"
  }},
  "lineItems": [
    {{
      "row": 1,
      "tanim": "description or item name, or empty string",
      "birim": "unit or empty string",
      "adet": "quantity, or empty string",
      "birim_fiyat": "unit price, or empty string",
      "toplam": "line total, or empty string"
    }},
    ...
  ],
  "raw_markdown": "full extracted text from the document"
}}

Rules:
- Return ONLY the JSON object. No markdown fences (no ```json ... ```).
- If a field is not found, use an empty string "".
- Do not add any extra fields or explanations.
- Preserve original number formats (e.g., "1.250,00").
- lineItems: one object per table row. Empty array [] if no rows found.
- raw_markdown: include the full raw OCR text for reference.

Think through the document structure carefully. Look for tables, headers, and structured sections.
"#,
        );

        let gpu_arg = self.gpu_layers.to_string();
        let threads_arg = self.threads.to_string();

        println!("=== Moondream 3.1-9B-A2B ===");
        println!("  Model: {}", model_path);
        println!("  Image: {}", image_path_str);
        println!("  GPU layers: {}", gpu_arg);
        println!("  Threads: {}", threads_arg);
        println!("  Temp: {}", self.temp);
        println!("  N-predict: {}", self.n_predict);
        println!("==============================");

        let output = Command::new(&cli)
            .args([
                "--model", model_path.as_str(),
                "--image", image_path_str.as_str(),
                "--prompt", &prompt,
                "--n-predict", &self.n_predict.to_string(),
                "--temp", &self.temp.to_string(),
                "--n-gpu-layers", &gpu_arg,
                "--threads", &threads_arg,
                "--log-disable",
                "--no-display-prompt",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| format!("llama-cli failed: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(format!(
                "Moondream execution failed (exit={:?}). stderr: {}\nstdout: {}",
                output.status.code(),
                stderr,
                stdout
            ));
        }

        let result = String::from_utf8_lossy(&output.stdout).to_string();
        println!("\n✓ Moondream completed successfully");
        println!("  Output length: {} bytes", result.len());
        Ok(result)
    }

    pub fn parse_response(&self, raw: &str) -> Result<(InvoiceResult, String), String> {
        let json_start = raw.find('{').ok_or("No JSON start found")?;
        let json_end = raw.rfind('}').map(|i| i + 1).ok_or("No JSON end found")?;
        let json_str = &raw[json_start..json_end];

        let parsed: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| format!("JSON parse error: {}", e))?;

        let mut fields: HashMap<String, String> = HashMap::new();
        let mut line_items: Vec<LineItem> = Vec::new();

        if let Some(obj) = parsed.as_object() {
            let field_keys = ["fatura_no", "fatura_tarihi", "vkn", "maliyet_toplam", "kdv_toplam", "toplam_tutar"];
            for key in &field_keys {
                if let Some(val) = obj.get("fields").and_then(|v| v.as_object())
                    .and_then(|fields| fields.get(key))
                    .and_then(|v| v.as_str())
                {
                    fields.insert(key.to_string(), val.to_string());
                }
            }

            if let Some(items_arr) = obj.get("lineItems").and_then(|v| v.as_array()) {
                for item in items_arr {
                    if let Some(item_obj) = item.as_object() {
                        let mut row = LineItem {
                            row: 0,
                            cells: HashMap::new(),
                        };
                        for cell_key in ["tanim", "birim", "adet", "birim_fiyat", "toplam"] {
                            if let Some(val) = item_obj.get(cell_key).and_then(|v| v.as_str()) {
                                row.cells.insert(cell_key.to_string(), val.to_string());
                            }
                        }
                        if let Some(row_num) = item_obj.get("row").and_then(|v| v.as_f64()) {
                            row.row = row_num as u32;
                        }
                        if !row.cells.is_empty() || item_obj.get("row").is_some() {
                            line_items.push(row);
                        }
                    }
                }
            }

            if let Some(md) = obj.get("raw_markdown").and_then(|v| v.as_str()) {
                if !md.is_empty() {
                    fields.insert("raw_markdown".to_string(), md.to_string());
                }
            }
        }

        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%S.%Z").to_string();

        let result = InvoiceResult {
            id,
            file_name: String::new(),
            file_type: String::new(),
            status: "success".to_string(),
            fields,
            line_items,
            raw_markdown: fields.get("raw_markdown").cloned(),
            model_used: "Moondream 3.1-9B-A2B".to_string(),
            created_at: now,
        };

        Ok((result, raw.to_string()))
    }
}

// ─── Model Manager ────────────────────────────────────────────────────────────

pub struct ModelManager {
    models_dir: PathBuf,
}

impl ModelManager {
    pub fn new() -> Self {
        let models_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Fatrocu")
            .join("models");
        std::fs::create_dir_all(&models_dir).unwrap_or(());
        Self { models_dir }
    }

    pub fn models_dir(&self) -> &PathBuf {
        &self.models_dir
    }

    pub fn list_models(&self) -> Vec<ModelInfo> {
        let mut models = Vec::new();
        if self.models_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&self.models_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().map(|e| e == "gguf").unwrap_or(false) {
                        let filename = path.file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("")
                            .to_string();
                        let size_bytes = path.metadata().map(|m| m.len()).unwrap_or(0);
                        models.push(ModelInfo {
                            name: filename,
                            path: path.to_string_lossy().to_string(),
                            size_bytes,
                            downloaded_at: String::new(),
                        });
                    }
                }
            }
        }
        models.sort_by(|a, b| a.name.as_str().cmp(&b.name.as_str()));
        models
    }

    pub fn download_model(&self, model_name: &str, output: Option<&PathBuf>) -> Result<PathBuf, String> {
        use reqwest::Client;

        let model = Self::find_model_repo(model_name)
            .ok_or_else(|| format!("Model not found: {}. Try 'fatrocu list-models'.", model_name))?;

        let dest = output.clone().or_else(|| {
            let filename = model.filename();
            Some(self.models_dir.join(filename))
        });

        let dest = dest.ok_or_else(|| "No destination path specified")?;
        let dest_dir = dest.parent().ok_or("No parent directory")?;
        std::fs::create_dir_all(dest_dir).map_err(|e| e.to_string())?;

        println!("Downloading: {} ({} MB)", model.name, model.size_mb());

        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Fatrocu-CLI/3.1")
            .timeout(std::time::Duration::from_secs(300))
            .build()
            .map_err(|e| e.to_string())?;

        let resp = client
            .get(&model.url)
            .send()
            .map_err(|e| format!("Download failed: {}", e))?;

        if !resp.status().is_success() {
            return Err(format!("HTTP {} for {}", resp.status(), model.url));
        }

        let bytes = resp
            .bytes()
            .map_err(|e| format!("Read failed: {}", e))?;

        std::fs::write(&dest, &bytes)
            .map_err(|e| format!("Write failed: {}", e))?;

        println!("✓ Downloaded: {}", dest.display());
        Ok(dest)
    }

    pub fn remove_model(&self, model_name: &str) -> Result<(), String> {
        let filename = model_name.trim().to_lowercase();
        let model_path = self.models_dir.join(&filename);
        if model_path.exists() {
            std::fs::remove_file(&model_path).map_err(|e| e.to_string())?;
            println!("✓ Removed: {}", model_path.display());
        } else {
            return Err(format!("Model not found: {}", model_path.display()));
        }
        Ok(())
    }

    pub fn import_model(&self, source: &PathBuf, dest: Option<&PathBuf>) -> Result<PathBuf, String> {
        if !source.exists() {
            return Err(format!("Source not found: {}", source.display()));
        }

        let filename = source.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let destination = dest.map(PathBuf::from).or_else(|| {
            let base = source.file_stem().unwrap_or(&filename);
            Some(self.models_dir.join(base))
        });

        let destination = destination.ok_or("No destination specified")?;
        let destination_dir = destination.parent().ok_or("No parent directory")?;
        std::fs::create_dir_all(destination_dir).map_err(|e| e.to_string())?;

        let destination = destination.clone();

        if source == destination {
            return Ok(destination);
        }

        std::fs::copy(source, &destination).map_err(|e| format!("Copy failed: {}", e))?;
        println!("✓ Imported: {} → {}", source.display(), destination.display());
        Ok(destination)
    }

    fn find_model_repo(model_name: &str) -> Option<ModelInfo> {
        let base = "https://huggingface.co";

        let repos = match model_name.to_lowercase().as_str() {
            "moondream" | "moondream 3.1-9b-a2b" | "moondream3.1" => {
                Some(("Qwen/Qwen2.5-VL-7B-Instruct", "Qwen/Qwen2.5-VL-7B-Instruct", "Moondream 3.1-9B-A2B".to_string()).0)
            }
            "moondream" | "moondream 3.1-9b-a2b" | "moondream3.1" => {
                Some(("Qwen/Qwen2.5-VL-7B-Instruct", "Qwen/Qwen2.5-VL-7B-Instruct", "Moondream 3.1-9B-A2B".to_string()).0)
            }
            _ => None,
        };

        Some(ModelInfo {
            name: model_name.to_string(),
            path: String::new(),
            size_bytes: 0,
            downloaded_at: String::new(),
        })
    }

    fn filename(&self, model_name: &str) -> String {
        model_name.trim().to_lowercase()
    }
}

// ─── PDF/Image Processor ──────────────────────────────────────────────────────

pub fn convert_pdf_to_png(pdf_path: &PathBuf, output_path: &PathBuf) -> Result<PathBuf, String> {
    use std::process::Command;

    if pdf_path.extension().map(|e| e == "pdf").unwrap_or(false) {
        println!("Converting PDF to PNG...");

        let output = Command::new("pdftoppm")
            .args([
                "-png",
                "-r", "150",
                "-pngalpha",
                pdf_path.to_string_lossy().as_ref(),
                output_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("output"),
            ])
            .output()
            .map_err(|e| format!("pdftoppm failed: {}", e))?;

        if !output.status.success() {
            return Err(format!("pdftoppm error: {}", String::from_utf8_lossy(&output.stderr)));
        }

        let png_path = output_path.join("output-1.png");
        if png_path.exists() {
            return Ok(png_path);
        }

        Err("pdftoppm did not produce expected output".to_string())
    } else {
        Ok(PathBuf::from(pdf_path.to_string_lossy().as_ref()))
    }
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let args = Cli::parse();
    let models_dir = ModelManager::new();

    match args.command {
        Commands::Process {
            image,
            model,
            model_path,
            config,
            output,
            temperature,
            n_predict,
            threads,
            gpu_layers,
            prompt,
        } => {
            let image_path = image
                .or_else(|| {
                    std::env::args()
                        .nth(2)
                        .map(PathBuf::from)
                        .or_else(|| {
                            std::env::current_dir()
                                .ok()
                                .map(|cwd| cwd.join("input.png"))
                        })
                })
                .ok_or("No image specified. Use --image <path>")?;

            if !image_path.exists() {
                eprintln!("Error: Image file not found: {}", image_path.display());
                std::process::exit(1);
            }

            let engine = MoondreamEngine::new(
                model.to_string(),
                model_path.unwrap_or(PathBuf::new()),
                threads,
                gpu_layers,
                temperature,
                n_predict,
            );

            match engine.run(&image_path) {
                Ok(raw_output) => {
                    match engine.parse_response(&raw_output) {
                        Ok((result, _)) => {
                            let json = serde_json::to_string_pretty(&result).unwrap();
                            std::fs::write(&output, json).unwrap();
                            println!("Result saved to: {}", output);
                            println!("{}", json);
                        }
                        Err(e) => {
                            eprintln!("Parse error: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Commands::List { format } => {
            // List processed invoices from storage
            println!("Processed invoices:");
            // TODO: load from JSON storage
        }
        Commands::Export { format, output } => {
            // Export to Excel/CSV
            println!("Export: {} → {}", format, output);
            // TODO: implement export
        }
        Commands::Models { subcommand } => {
            match subcommand {
                ModelCommands::Download { name, output } => {
                    match models_dir.download_model(&name, output.as_ref().map(PathBuf::from)) {
                        Ok(path) => println!("Downloaded: {}", path.display()),
                        Err(e) => {
                            eprintln!("Download error: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                ModelCommands::Remove { name } => {
                    match models_dir.remove_model(&name) {
                        Ok(()) => println!("Removed: {}", name),
                        Err(e) => {
                            eprintln!("Error: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                ModelCommands::List => {
                    let models = models_dir.list_models();
                    if models.is_empty() {
                        println!("No models found. Use: fatrocu models --download <model-name>");
                    } else {
                        for m in &models {
                            println!("  {} — {} MB", m.name, m.size_bytes / 1024 / 1024);
                        }
                    }
                }
                ModelCommands::Import { source, dest } => {
                    match models_dir.import_model(&source, dest.as_ref().map(PathBuf::from)) {
                        Ok(path) => println!("Imported: {}", path.display()),
                        Err(e) => {
                            eprintln!("Import error: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
            }
        }
        Commands::Status => {
            let cli = MoondreamEngine::find_llama_cli();
            let models_dir = models_dir.models_dir().to_string_lossy().to_string();
            let models = models_dir.list_models();

            println!("=== Fatrocu CLI v3.1 Status ===");
            println!("  Engine: {:?}", cli.is_some());
            println!("  Models dir: {}", models_dir);
            println!("  Models found: {}", models.len());
            if !models.is_empty() {
                for m in &models {
                    println!("    - {} ({} MB)", m.name, m.size_bytes / 1024 / 1024);
                }
            }
            println!("==============================");
        }
        Commands::ListModels => {
            println!("Available models:");
            println!("  Moondream 3.1-9B-A2B — Qwen/Qwen2.5-VL-7B-Instruct (7 GB)");
            println!("  Moondream — same as above");
            println!("  ...");
        }
    }
}
