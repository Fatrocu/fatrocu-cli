use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use chrono::Local;
use std::collections::HashMap;
use std::env;

use std::io::BufWriter;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Parser)]
#[command(name = "fatrocu")]
#[command(author = "Nec0ti")]
#[command(version = "3.1.0")]
#[command(about = "ImajeV‑2B‑Q8_0 ile fatura işleme")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Process {
        #[arg(short, long)]
        model: Option<String>,
        // model_path has been removed – the binary now always uses the built‑in '--model' flag with the file in the models directory.
        #[arg(short, long)]
        image: String,
        #[arg(short, long, default_value = "sonuc.json")]
        output: String,
        #[arg(short, long, default_value = "0.2")]
        temp: f64,
        #[arg(short, long, default_value = "4")]
        threads: usize,
        #[arg(short, long, default_value = "0")]
        gpu_layers: usize,
        #[arg(short, long, default_value = "256")]
        ctx_size: usize,
        #[arg(short, long, default_value = "8")]
        n_predict: usize,
    },
    Models {
        #[command(subcommand)]
        sub: ModelSubcommand,
    },
    #[command(name = "list")]
    List,
    #[command(name = "status")]
    Status,
    Export {
        #[arg(short, long, value_enum, default_value = "xlsx")]
        format: ExportFormat,
        #[arg(short, long, default_value = "Fatrocu_Raporu.xlsx")]
        output: String,
        #[arg(long, default_value = "reviewed")]
        status: String,
        #[arg(long, default_value = "Fatura")]
        header: String,
    },
    Report {
        #[arg(short, long, default_value = "rapor_2026-10-06.json")]
        output: String,
    },
    #[command(name = "version")]
    Version,
    #[command(name = "help")]
    Help,
}

#[derive(Subcommand)]
enum ModelSubcommand {
    Download {
        model: String,
        #[arg(short, long, default_value = "models")]
        output: String,
    },
    Remove {
        model: String,
    },
}

#[derive(ValueEnum, Clone, Debug, PartialEq)]
enum ExportFormat {
    Xlsx,
    Csv,
    Json,
}

fn get_model_path(model: &str) -> PathBuf {
    let path = std::env::var("FATROCU_MODEL_PATH").unwrap_or_else(|_| "models".to_string());
    PathBuf::from(path).join(model)
}

fn get_cli_path() -> PathBuf {
    let exe = std::env::current_exe()
        .expect("Cannot get current executable path")
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "llama-cli.exe".to_string());
    let exe = exe.trim_end_matches(".exe");
    let current_exe = std::env::current_dir().unwrap_or_default();
    let current_path = current_exe.join(exe);
    if current_path.exists() { return current_path; }
    let default_path = PathBuf::from("C:/Users/PC/Desktop/fatrocu-cli/llama-cli.exe");
    if default_path.exists() { return default_path; }
    let appdata_path = std::env::var("APPDATA").unwrap_or_else(|_| "C:/Users/PC/Desktop/fatrocu-cli".to_string());
    let appdata_exe = PathBuf::from(appdata_path).join(exe);
    if appdata_exe.exists() { return appdata_exe; }
            println!("\n⚠️  llama-cli.exe not found. Downloading...\n");
            // Download llama-cli.exe from HuggingFace
            let url = "https://huggingface.co/cjhb/llama-cli-windows/releases/download/v2.5.0/llama-cli.exe";
            let status = Command::new("curl").args(&["-L", "-o", "C:/Users/PC/Desktop/fatrocu-cli/llama-cli.exe", url]).status()?;
            if !status.success() { return Err(anyhow::anyhow!("Failed to download llama-cli.exe")); }
            println!("✅ llama-cli.exe downloaded.");
            let downloaded = PathBuf::from("C:/Users/PC/Desktop/fatrocu-cli/llama-cli.exe");
            return Ok(downloaded);

}

fn check_cli() -> Result<PathBuf> {
    let cli_path = get_cli_path();
    if !cli_path.exists() {
        return Err(anyhow::anyhow!("llama-cli.exe bulunamadı: {:?}", cli_path));
    }
    Ok(cli_path)
}

fn print_header() {
    println!(
        "\n╔══════════════════════════════════════════════════════════════╗\n\
         ║   Fatrocu CLI v3.1.0 — ImajeV‑2B‑Q8_0                      ║\n\
         ║   © 2026 Nec0ti — All rights reserved.                     ║\n\
         ╠══════════════════════════════════════════════════════════════╣\n");
    );
}

fn print_footer() {
    println!("╚══════════════════════════════════════════════════════════════╝");
}

fn run_process(
    model: &str, _model_path: &str, image_path: &str, output: &str,
    temp: f64, threads: usize, gpu_layers: usize, ctx_size: usize, n_predict: usize,
) -> Result<()> {
    println!("\n📄 Invoice processing started...");
    println!("   Model:   {}", model);
    println!("   File:   {}", image_path);
    println!("   Output: {}", output);
    println!("   Temp:     {}, Threads: {}, GPU Layers: {}, N-Predict: {}",
             temp, threads, gpu_layers, n_predict);

    let cli_path = check_cli()?;
    // Use absolute forward‑slash path for the model file – llama‑cli on Windows expects '/' separators
    let model_file = PathBuf::from("C:/Users/PC/Desktop/fatrocu-cli/models").join(format!("{}.gguf", model));
    let output_path = PathBuf::from(output);
    let mut args = vec![
        "--model".to_string(), model_file.to_str().unwrap().to_string(),
        "--image".to_string(), image_path.to_string(),
        "--temp".to_string(), temp.to_string(),
        "--threads".to_string(), threads.to_string(),
        "--gpu-layers".to_string(), gpu_layers.to_string(),
        "--n-predict".to_string(), n_predict.to_string(),
    ];
    // If a fatrocu-server URL is provided, POST the request there instead of invoking llama-cli locally
    if let Ok(server_url) = env::var("FATROCU_SERVER_URL") {
        // Build multipart POST using curl (available via MSYS on Windows)
        let mut curl_cmd = Command::new("curl");
        curl_cmd.args(&[
            "-X", "POST",
            "-F", &format!("image=@{}", image_path),
            "-F", &format!("model={}", model),
            "-F", &format!("temp={}", temp),
            "-F", &format!("n_predict={}", n_predict),
            "-F", &format!("ctx_size={}", ctx_size),
            &format!("{}/process", server_url),
        ]);
        let output = curl_cmd.output().context("failed to call fatrocu-server")?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("Server call failed: {}", err));
        }
        // Write server JSON response directly to the output file
        std::fs::write(&output_path, &output.stdout)?;
        println!("✅ Server response saved to {}", output_path.display());
        return Ok(());
    }
    // Add optional context size flag (once)
    if ctx_size > 0 {
        args.push("--ctx-size".to_string());
        args.push(ctx_size.to_string());
    }


    // Enable low‑vram to reduce memory on CPU‑only systems
    args.push("--low-vram".to_string());
    let mmproj_path = PathBuf::from("C:/Users/PC/Desktop/fatrocu-cli/models").join(format!("{}-mmproj-f16.gguf", model));
    if mmproj_path.exists() {
        args.push("--mmproj".to_string());
        args.push(mmproj_path.to_str().unwrap().to_string());
    }
    let start = Instant::now();

    let output_file = File::create(&output_path).context("Çıktı dosyası oluşturulamadı")?;
    let mut writer = BufWriter::new(output_file);

    let output = Command::new(&cli_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("llama-cli çalıştırılamadı")?;

    let elapsed = start.elapsed();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("\n   ❌ İşlem başarısız: {}\n   Çıktı: {}", stderr, output.status);
        return Err(anyhow::anyhow!("İşlem başarısız: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("   ✅ İşlem tamamlandı! Süre: {:?}\n", elapsed);
    println!("   ▶️  JSON parse ediliyor...\n");

    let result: Vec<ProcessedDocument> = match serde_json::from_str(&stdout) {
        Ok(docs) => docs,
        Err(e) => {
            println!("   ⚠️  JSON parse hatası: {}", e);
            let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
            let mut result = Vec::new();
            for line in lines {
                if let Ok(doc) = serde_json::from_str::<ProcessedDocument>(line) {
                    result.push(doc);
                }
            }
            result
        }
    };

    if result.is_empty() {
        println!("   ⚠️  Hiçbir fatura bulunamadı");
        std::fs::write(&output_path, stdout.to_string())?;
        return Ok(());
    }

    for doc in &result {
        println!(
            "   [+] Sayfa {}: {} — {} TL",
            doc.page.unwrap_or(0) + 1,
            doc.fatura_no.as_deref().unwrap_or("N/A"),
            doc.genel_toplam.as_deref().unwrap_or("0.00")
        );
    }

    println!("   ✅ {} fatura işlendi", result.len());
    std::fs::write(&output_path, stdout.to_string())?;
    Ok(())
}

fn run_models_subcommand(sub: ModelSubcommand) -> Result<()> {
    println!("\n📦 Model Yönetimi");
    let cli_path = check_cli()?;
    let args = match sub {
        ModelSubcommand::Download { model, output } => {
            // Ensure the output directory exists (output defaults to "models")
            let out_dir = std::path::Path::new(&output);
            std::fs::create_dir_all(out_dir).ok();
            let out_file = out_dir.join(format!("{}.gguf", model));
            // Construct Hugging Face download URL (model file assumed to be <model>.gguf)
            let url = format!("https://huggingface.co/nec0ti/{}/resolve/main/{}.gguf", model, model);
            println!("Downloading model from {}...", url);
            let status = std::process::Command::new("curl")
                .args(&["-L", "-H", &format!("Authorization: Bearer {}", std::env::var("HF_TOKEN").unwrap_or_default()), "-o", out_file.to_str().unwrap(), &url])
                .status()?;
            if !status.success() {
                return Err(anyhow::anyhow!("Failed to download model {}", model));
            }
            println!("✅ Model downloaded to {}", out_file.display());
            return Ok(());
        },
        ModelSubcommand::Remove { model } => vec![
            "remove".to_string(),
            model.clone(),
        ],
    };
    // For non-download subcommands, forward to llama-cli
    let output = std::process::Command::new(&cli_path).args(&args).output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("   ❌ Hata: {}", stderr);
        return Err(anyhow::anyhow!("{}", stderr));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{}", stdout);
    Ok(())
}



fn run_export(format: ExportFormat, output: &str, status: &str, header: &str) -> Result<()> {
    println!("\n📊 Export Başlatılıyor...");
    println!("   Format:  {:?}", format);
    println!("   File:   {}", output);
    println!("   Durum:   {}", status);
    println!("   Başlık:  {}", header);

    let cli_path = check_cli()?;
    let output_path = PathBuf::from(output);

    if output_path.exists() {
        let content = std::fs::read_to_string(&output_path)?;
        let data: Vec<ProcessedDocument> = serde_json::from_str(&content).unwrap_or_else(|_| Vec::new());
        let mut rows = Vec::new();
        for doc in data {
            rows.push(vec![
                doc.fatura_no.unwrap_or_else(|| "N/A".to_string()),
                doc.fatura_tarihi.unwrap_or_else(|| "N/A".to_string()),
                doc.cari_unvan.unwrap_or_else(|| "N/A".to_string()),
                doc.cari_vergi_no.unwrap_or_else(|| "N/A".to_string()),
                doc.ettn_uuid.unwrap_or_else(|| "N/A".to_string()),
                doc.mal_hizmet_toplam_matrah.unwrap_or_else(|| "0.00".to_string()),
                doc.kdv_orani.unwrap_or_else(|| "0".to_string()),
                doc.kdv_tutari.unwrap_or_else(|| "0.00".to_string()),
                doc.genel_toplam.unwrap_or_else(|| "0.00".to_string()),
                doc.kalemler.iter().map(|k| {
                    let ad = k.ad.as_ref().map(|s| s.as_str()).unwrap_or("N/A");
                    let miktar = k.miktar.as_ref().map(|s| s.as_str()).unwrap_or("0.00");
                    let birim_fiyat = k.birim_fiyat.as_ref().map(|s| s.as_str()).unwrap_or("0.00");
                    let toplam = k.toplam.as_ref().map(|s| s.as_str()).unwrap_or("0.00");
                    format!("{}|{}|{}|{}|{} TL", ad, miktar, birim_fiyat, toplam, k.ad.as_ref().map(|s| s.as_str()).unwrap_or("N/A"))
                }).collect::<Vec<_>>().join(";"),
            ]);
        }

        if format == ExportFormat::Xlsx {
            let filename = output_path.file_name().unwrap_or_default().to_str().unwrap_or("Fatrocu_Raporu.xlsx");
            println!("   ✅ Excel export (skribo kullanılamıyordu, CSV kullanılıyor)");
            if let Ok(f) = File::create(&output_path) {
                let mut writer = csv::Writer::from_writer(f);
                writer.write_record(&["Fatura No", "Fatura Tarihi", "Kari Ünvan", "Kari Vergi No",
                    "ETTN UUID", "Mal/Hizmet Toplam Matrah", "KDV Oranı", "KDV Tutarı",
                    "Genel Toplam", "Kalemler"])?;
                for row in rows { writer.write_record(row)?; }
                writer.flush()?;
            }
            println!("   ✅ CSV dosyası oluşturuldu: {}", filename);
        } else if format == ExportFormat::Csv {
            let f = File::create(&output_path).context("Dosya oluşturulamadı")?;
            let mut writer = csv::Writer::from_writer(f);
            for row in rows { writer.write_record(row)?; }
            writer.flush()?;
            println!("   ✅ CSV dosyası oluşturuldu: {}", output);
        } else {
            println!("   ✅ JSON çıktı: {}", output);
        }
    } else {
        println!("   ⚠️  Çıktı dosyası bulunamadı: {}", output);
    }
    Ok(())
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ProcessedDocument {
    page: Option<usize>,
    fatura_no: Option<String>,
    fatura_tarihi: Option<String>,
    cari_unvan: Option<String>,
    cari_vergi_no: Option<String>,
    ettn_uuid: Option<String>,
    mal_hizmet_toplam_matrah: Option<String>,
    kdv_orani: Option<String>,
    kdv_tutari: Option<String>,
    genel_toplam: Option<String>,
    kalemler: Vec<LineItem>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct LineItem {
    ad: Option<String>,
    miktar: Option<String>,
    birim_fiyat: Option<String>,
    toplam: Option<String>,
}



fn main() -> Result<()> {
    let cli = Cli::parse();
    print_header();
    let cli_path = check_cli()?;
    match cli.command {
        Commands::Process { model, image, output, temp, threads, gpu_layers, ctx_size, n_predict } => {
            let model = model.unwrap_or_else(|| "ImajeV-2B-Q8_0".to_string());
            // model_path is no longer needed – ignore it completely.
            run_process(&model, "", &image, &output, temp, threads, gpu_layers, ctx_size, n_predict)?;
        }
        Commands::Models { sub } => run_models_subcommand(sub)?,
        Commands::List => {
            let output = Command::new(&cli_path).arg("models --list").output()?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            println!("{}", stdout);
        }
        Commands::Status => {
            let output = Command::new(&cli_path).arg("status").output()?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            println!("{}", stdout);
        }
        Commands::Export { format, output, status, header } => run_export(format, &output, &status, &header)?,
        Commands::Report { output } => {
            let mut report = HashMap::new();
            report.insert("tarih", Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
            report.insert("sistem", "Fatrocu CLI v3.1.0".to_string());
            report.insert("model", "ImajeV‑2B‑Q8_0".to_string());
            let args: Vec<String> = std::env::args().collect();
            report.insert("komut", args.join(" ").to_string());
            let json = serde_json::to_string_pretty(&report)?;
            std::fs::write(&output, &json)?;
            println!("✅ Rapor kaydedildi: {}", output);
        }
        Commands::Version => println!("fatrocu-cli v3.1.0 (c) 2026 Nec0ti"),
        Commands::Help => {
            println!(
                "
KULLANIM: fatrocu <komut> [seçenekler]

KOMUTLAR:
  process    Fatura işleme
  models     Model yönetimi (download, remove)
  list       Model listesi
  status     Motor durumu
  export     Excel/CSV export
  report     Hata raporu
  version    Sürüm bilgisi
  help       Bu mesaj

ÖRNEKLER:
  fatrocu process --model \"ImajeV-2B-Q8_0\" --image invoice.pdf --output result.json
  fatrocu models --download \"ImajeV-2B-Q8_0\"
  fatrocu models --list
  fatrocu status
  fatrocu export --format csv --output rapor.csv
"
            );
        }
    }
    print_footer();
    Ok(())
}

