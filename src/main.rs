use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use chrono::{DateTime, Local};
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Instant;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(name = "fatrocu")]
#[command(author = "Nec0ti")]
#[command(version = "3.1.0")]
#[command(about = "İmajeV-2B-Q8_0 ile fatura işleme — yerel OCR + alan çıkarma")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Fatura işleme
    Process {
        /// Model adı
        #[arg(short, long)]
        model: Option<String>,
        /// Model dosyası yolu
        #[arg(short, long, default_value = "models/imajeV.gguf")]
        model_path: String,
        /// Giriş dosyası (PDF veya görsel)
        #[arg(short, long)]
        image: String,
        /// Çıktı JSON dosyası
        #[arg(short, long, default_value = "sonuc.json")]
        output: String,
        /// Temperature parametresi
        #[arg(short, long, default_value = "0.2")]
        temp: f64,
        /// Thread sayısı
        #[arg(short, long, default_value = "4")]
        threads: usize,
        /// GPU katman sayısı
        #[arg(short, long, default_value = "0")]
        gpu_layers: usize,
        /// Prompt uzunluğu
        #[arg(short, long, default_value = "2048")]
        n_predict: usize,
    },
    /// Model yönetimi
    Models {
        #[command(subcommand)]
        sub: ModelSubcommand,
    },
    /// Model listesi
    #[command(name = "list")]
    List,
    /// Motor durumu
    #[command(name = "status")]
    Status,
    /// Model export
    Export {
        /// Çıktı formatı
        #[arg(short, long, value_enum, default_value = "xlsx")]
        format: ExportFormat,
        /// Çıktı dosyası
        #[arg(short, long, default_value = "Fatrocu_Raporu.xlsx")]
        output: String,
        /// Durum filtresi
        #[arg(long, default_value = "reviewed")]
        status: String,
        /// Sayfa başlığı
        #[arg(long, default_value = "Fatura")]
        header: String,
    },
    /// Hata raporlama
    Report {
        /// Rapor dosyası
        #[arg(short, long, default_value = "rapor_2026-10-06.json")]
        output: String,
    },
    /// Sürüm bilgisi
    #[command(name = "version")]
    Version,
    /// Yardımcı
    #[command(name = "help")]
    Help,
}

#[derive(Subcommand)]
enum ModelSubcommand {
    /// Model indirme
    Download {
        /// Model adı
        model: String,
        /// Hedef dizin
        #[arg(short, long, default_value = "models")]
        output: String,
    },
    /// Model silme
    Remove {
        /// Model adı
        model: String,
    },
}

#[derive(ValueEnum, Clone, Debug)]
enum ExportFormat {
    Xlsx,
    Csv,
    Json,
}

fn get_model_path(model: &str) -> PathBuf {
    let path = std::env::var("FATROCU_MODEL_PATH")
        .unwrap_or_else(|_| "models".to_string());
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

    if current_path.exists() {
        return current_path;
    }

    let appdata_path = {
        let mut path = std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("C:/Users/PC/Desktop/fatrocu-cli"));
        while !path.is_absolute() {
            if let Ok(parent) = path.parent() {
                path = parent.clone();
            } else {
                break;
            }
        }
        if path == current_exe {
            break;
        }
        path
    };

    let appdata_exe = appdata_path.join(exe);
    if appdata_exe.exists() {
        return appdata_exe;
    }

    let default_path = PathBuf::from("C:/Users/PC/Desktop/fatrocu-cli/llama-cli.exe");
    if default_path.exists() {
        return default_path;
    }

    let appdata_path = std::env::var("APPDATA")
        .unwrap_or_else(|_| "C:/Users/PC/Desktop/fatrocu-cli".to_string());
    let appdata_exe = PathBuf::from(appdata_path).join(exe);

    if appdata_exe.exists() {
        return appdata_exe;
    }

    println!(
        "\n⚠️  llama-cli.exe bulunamadı! Lütfen manuel olarak indirin:\n\
         https://huggingface.co/cjhb/llama-cli-windows/releases/download/v2.5.0/llama-cli.exe\n\
         Ve C:/Users/PC/Desktop/fatrocu-cli/llama-cli.exe'e kopyalayın.\n"
    );

    PathBuf::from("C:/Users/PC/Desktop/fatrocu-cli/llama-cli.exe")
}

fn check_cli() -> Result<PathBuf> {
    let cli_path = get_cli_path();
    if !cli_path.exists() {
        return Err(anyhow::anyhow!(
            "llama-cli.exe bulunamadı: {:?}",
            cli_path
        ));
    }
    Ok(cli_path)
}

fn print_header() {
    println!(
        "\n╔══════════════════════════════════════════════════════════════╗\n\
         ║   Fatrocu CLI v3.1.0 — İmajeV-2B-Q8_0                      ║\n\
         ║   İmajeV-2B-Q8_0 ile fatura işleme, OCR + alan çıkarma     ║\n\
         ║   © 2026 Nec0ti — Tüm hakları saklıdır.                     ║\n\
         ╠══════════════════════════════════════════════════════════════╣\n"
    );
}

fn print_footer() {
    println!("╚══════════════════════════════════════════════════════════════╝");
}

fn run_process(
    model: &str,
    model_path: &str,
    image_path: &str,
    output: &str,
    temp: f64,
    threads: usize,
    gpu_layers: usize,
    n_predict: usize,
) -> Result<()> {
    println!("\n📄 Fatura İşleme Başlatılıyor...");
    println!("   Model:   {}", model);
    println!("   Dosya:   {}", image_path);
    println!("   Çıktı:   {}", output);
    println!("   Temp:     {}", temp);
    println!("   Threads:  {}", threads);
    println!("   GPU Layers: {}", gpu_layers);

    let cli_path = check_cli()?;
    let model_path = PathBuf::from(model_path);
    let output_path = PathBuf::from(output);

    let temp_str = temp.to_string();
    let threads_str = threads.to_string();
    let gpu_str = gpu_layers.to_string();
    let predict_str = n_predict.to_string();

    let args = vec![
        "--model".to_string(),
        model.to_string(),
        "--model-path".to_string(),
        model_path.to_str().unwrap().to_string(),
        "--image".to_string(),
        image_path.to_string(),
        "--temp".to_string(),
        temp_str,
        "--threads".to_string(),
        threads_str,
        "--gpu-layers".to_string(),
        gpu_str,
        "--n-predict".to_string(),
        predict_str,
    ];

    println!("\n   ▶️  İşlem başlatılıyor...\n");
    let start = Instant::now();

    let output_file = File::create(&output_path).context("Çıktı dosyası oluşturulamadı")?;
    let mut writer = std::io::BufWriter::new(output_file);

    let output = Command::new(&cli_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("llama-cli çalıştırılamadı")?;

    let elapsed = start.elapsed();

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!(
            "\n   ❌ İşlem başarısız: {}\n   Çıktı: {}",
            stderr,
            output.status
        );
        return Err(anyhow::anyhow!("İşlem başarısız: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("   ✅ İşlem tamamlandı! Süre: {:?}\n", elapsed);
    println!("   ▶️  JSON parse ediliyor...\n");

    let result: Vec<ProcessedDocument> = match serde_json::from_str(&stdout) {
        Ok(docs) => docs,
        Err(e) => {
            println!("   ⚠️  JSON parse hatası: {}", e);
            println!("   ▶️  Metin tabanlı ayrıştırma...\n");
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
        std::fs::write(&output_path, stdout)?;
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

    std::fs::write(&output_path, stdout)?;
    Ok(())
}

fn run_models_subcommand(sub: ModelSubcommand) -> Result<()> {
    println!("\n📦 Model Yönetimi");
    let cli_path = check_cli()?;

    let output = Command::new(&cli_path)
        .arg("models")
        .arg(&sub.to_string())
        .output()
        .context("llama-cli çalıştırılamadı")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("   ❌ Hata: {}", stderr);
        return Err(anyhow::anyhow!("{}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("{}", stdout);
    Ok(())
}

fn run_export(
    format: ExportFormat,
    output: &str,
    status: &str,
    header: &str,
) -> Result<()> {
    println!("\n📊 Export Başlatılıyor...");
    println!("   Format:  {:?}", format);
    println!("   Dosya:   {}", output);
    println!("   Durum:   {}", status);
    println!("   Başlık:  {}", header);

    let cli_path = check_cli()?;

    let output_path = PathBuf::from(output);

    if output_path.exists() {
        let content = std::fs::read_to_string(&output_path)?;
        let data: Vec<ProcessedDocument> =
            serde_json::from_str(&content).unwrap_or_else(|_| Vec::new());

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
                doc.kalemler
                    .iter()
                    .map(|k| {
                        format!(
                            "{} | {} | {} | {} | {} TL",
                            k.ad.unwrap_or("N/A"),
                            k.miktar.unwrap_or("0.00"),
                            k.birim_fiyat.unwrap_or("0.00"),
                            k.toplam.unwrap_or("0.00"),
                            k.ad.unwrap_or("N/A"),
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(";"),
            ]);
        }

        if format == ExportFormat::Xlsx {
            let mut wb = excelize::Workbook::new().unwrap();
            for (i, row) in rows.iter().enumerate() {
                for (col, value) in row.iter().enumerate() {
                    let cell = excelize::Cell::String(value.to_string());
                    let cell_addr = excelize::Address::new(i + 1, col + 1);
                    wb.set_cell(cell_addr, cell).unwrap();
                }
            }
            let filename = output_path
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or("Fatrocu_Raporu.xlsx");
            wb.save(filename)?;
            println!("   ✅ Excel dosyası oluşturuldu: {}", filename);
        } else if format == ExportFormat::Csv {
            let mut f = File::create(&output_path).context("Dosya oluşturulamadı")?;
            let mut writer = csv::Writer::from_writer(f);
            for row in rows {
                writer.write_record(row)?;
            }
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
    let args = cli.command.to_string();

    match cli.command {
        Commands::Process {
            model,
            model_path,
            image,
            output,
            temp,
            threads,
            gpu_layers,
            n_predict,
        } => {
            let model = model.unwrap_or_else(|| "ImajeV-2B-Q8_0".to_string());
            run_process(
                &model,
                &model_path,
                &image,
                &output,
                temp,
                threads,
                gpu_layers,
                n_predict,
            )?;
        }
        Commands::Models { sub } => {
            run_models_subcommand(sub)?;
        }
        Commands::List => {
            let output = Command::new(&cli_path)
                .arg("models --list")
                .output()
                .context("llama-cli çalıştırılamadı")?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            println!("{}", stdout);
        }
        Commands::Status => {
            let output = Command::new(&cli_path)
                .arg("status")
                .output()
                .context("llama-cli çalıştırılamadı")?;

            let stdout = String::from_utf8_lossy(&output.stdout);
            println!("{}", stdout);
        }
        Commands::Export {
            format,
            output,
            status,
            header,
        } => {
            run_export(format, &output, &status, &header)?;
        }
        Commands::Report { output } => {
            let mut report = HashMap::new();
            report.insert("tarih", Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
            report.insert("sistem", "Fatrocu CLI v3.1.0".to_string());
            report.insert("model", "İmajeV-2B-Q8_0".to_string());

            let args = std::env::args().collect::<Vec<_>>();
            report.insert("komut", args.join(" ").to_string());

            let json = serde_json::to_string_pretty(&report)?;
            std::fs::write(&output, json)?;
            println!("✅ Rapor kaydedildi: {}", output);
        }
        Commands::Version => {
            println!("fatrocu-cli v3.1.0 (c) 2026 Nec0ti");
        }
        Commands::Help => {
            print!(
                "
KULLANIM: fatrocu <komut> [seçenekler]

KOMUTLAR:
  process    Fatura işleme
  models     Model yönetimi
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
  fatrocu export --format xlsx --output rapor.xlsx
  fatrocu status
"
            );
        }
    }

    print_footer();
    Ok(())
}