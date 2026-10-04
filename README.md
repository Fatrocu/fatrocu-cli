# Fatrocu CLI v3.1.0

> **İmajeV-2B-Q8_0** ile güçlendirilmiş, tamamen yerel çalışan fatura işleme aracı. OCR + alan çıkarma tek bir model çağrısında.

<div align="center">

```
            ('-.     .-') _   _  .-')                                      
           ( OO ).-.(  OO) ) ( \( -O )                                     
   ,------./ . --. //     .-'.'------.  .-'),-----.    .-----. ,--. ,--.   
('-| _.---'| \-.  \ |'--...__)|   /`. '( OO'  .-.  '  '  .--./ |  | |  |   
(OO|(_\  .-'-'  |  |'--.  .--'|  /  | |/   |  | |  |  |  |('-. |  | | .-') 
/  |  '--.\| |_.'  |   |  |   |  |\  \    `'  '-'  '(_'  '--'\('  '-'(_.-' 
\_)|  .--' |  .-.  |   |  |   |  |.\  \    `'  '-'  '(_'  '--'\('  '-'(_.-' 
  \|  |_)  |  | |  |   |  |   |  | |\  \    `'  '-'  '(_'  '--'\('  '-'(_.-' 
   `--'    `--' `--'   `--'   `--' '--'     `-----'    `-----'  `-----'    

     ★  AKILLI FATURA İŞLEME SİSTEMİ ★
```

[![Release](https://img.shields.io/badge/v3.1.0-blue?style=for-the-badge&logo=github)](https://github.com/Nec0ti/Fatrocu/releases)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-green?style=for-the-badge&logo=windows)](https://github.com/Nec0ti/Fatrocu/releases)
[![License](https://img.shields.io/badge/License-MIT-blue?style=for-the-badge)](../LICENSE)
[![Python](https://img.shields.io/badge/Python-3.10%2B-blue?style=for-the-badge&logo=python)](https://www.python.org/)
[![Rust](https://img.shields.io/badge/Rust-1.75%2B-orange?style=for-the-badge&logo=rust)](https://rustup.rs/)

**%100 Yerel · Bulut yok · API maliyeti yok · Veri gizliliği tam garanti**

[📦 İndir](https://github.com/Nec0ti/Fatrocu/releases) · [📚 Dokümantasyon](../docs/index.html) · [🐛 Hata Bildir](https://github.com/Nec0ti/Fatrocu/issues) · [📖 API Referansı](../docs/api-reference.md)

</div>

---

## 🚀 Hızlı Başlangıç

```bash
# 1. Repoyu klonla
git clone https://github.com/Nec0ti/Fatrocu.git
cd Fatrocu/fatrocu-cli

# 2. Bağımlılıkları kur
pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu124
pip install excelize python-dateutil python-dotenv

# 3. Model indir
cargo run --release -- models --download "ImajeV-2B-Q8_0"

# 4. Fatura işle
cargo run --release -- process --model "ImajeV-2B-Q8_0" --image "fatura.pdf" --output "sonuc.json"

# 5. Listele
cargo run --release -- list --format json
```

### Çıktı örneği

```json
{
  "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
  "file_name": "fatura.pdf",
  "file_type": "pdf",
  "status": "success",
  "fields": {
    "cari_unvan": "ABC İŞLETMELERİ A.Ş.",
    "cari_vergi_no": "1234567890",
    "fatura_no": "DF02026000018498",
    "fatura_tarihi": "15.01.2024",
    "ettn_uuid": "550e8400-e29b-41d4-a716-446655440000",
    "mal_hizmet_toplam_matrah": { "value": 15208.33, "detected": true },
    "kdv_orani": { "value": 20, "detected": true },
    "kdv_tutari": { "value": 3041.67, "detected": true },
    "genel_toplam": { "value": 18250.00, "detected": true }
  },
  "line_items": [
    {
      "row": 1,
      "cells": {
        "urun_adi": "Hizmet A",
        "miktar": 1,
        "birim_fiyat": 15208.33,
        "toplam": 15208.33
      }
    }
  ],
  "model_used": "ImajeV-2B-Q8_0",
  "processing_time_ms": 1247,
  "tokens_generated": 2048
}
```

---

## ✨ Özellikler

- **⚡ Hızlı İşleme**: ~55 token/s (CPU) / ~280 token/s (GPU RTX 4090)
- **🔒 %100 Yerel**: Veri hiç internete gitmez. Tüm işlemler bilgisayarınızda.
- **🤖 İmajeV-2B-Q8_0**: 2 milyar parametreli, hafif ve güçlü görsel-dil model.
- **📄 PDF & Görsel**: PDF, PNG, JPG, TIFF, BMP formatlarını destekler.
- **📊 Excel/CSV Export**: İşlenen faturaları Excel veya CSV olarak dışa aktarın.
- **🎯 Tam Otomasyon**: Modeli indirin, uygulamayı çalıştırın — geri kalanını biz hallederiz.

---

## 📦 Kurulum

### Gereksinimler

| Araç | Sürüm |
|------|-------|
| Rust | 1.75+ |
| Python | 3.10+ |
| Git | herhangi bir sürüm |
| Disk | 7-8 GB boş alan (model için) |

### Hızlı Kurulum

```bash
# 1. Rust kur (eğer yüklü değilse)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Python bağımlılıkları
pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu124
pip install excelize python-dateutil python-dotenv

# 3. Fatrocu CLI'yi derle
cargo build --release

# 4. Model indir
cargo run --release -- models --download "ImajeV-2B-Q8_0"

# 5. Hazır!
cargo run --release -- status
```

---

## 📋 Komutlar

```bash
# Fatura işleme
fatrocu process --model ImajeV-2B-Q8_0 --image "fatura.pdf" --output "sonuc.json"

# Model yönetimi
fatrocu models --download "ImajeV-2B-Q8_0"
fatrocu models --list
fatrocu models --remove "Gemma-4-E4B"

# Dışa aktarma
fatrocu export --format xlsx --output "rapor.xlsx"
fatrocu export --format csv --output "rapor.csv"

# Durum kontrolü
fatrocu status

# Listele
fatrocu list --format table
```

Detaylı API referansı için: [api-reference.md](../docs/api-reference.md)

---

## 🧩 Mimari

```
┌──────────────────────────────────────────────────┐
│              Fatrocu CLI v3.1.0                   │
├──────────────────────────────────────────────────┤
│                                                   │
│  ┌─────────────────────┐                         │
│  │  ImajeV-2B-Q8_0    │ ← Tek model, tek çağrı  │
│  │  (2B parametre)     │ ← OCR + çıkarma         │
│  └──────────┬──────────┘                         │
│             │                                     │
│   ┌─────────┼─────────┐                          │
│   │ PDF → PNG│ Görsel │                          │
│   └─────────┼─────────┘                          │
│             │                                     │
│   ┌─────────┼─────────┐                          │
│   │ İmajeV-2B│ Unified │ ← Tek JSON çıktısı     │
│   │ engine   │ prompt  │                         │
│   └─────────┼─────────┘                          │
│             │                                     │
│   ┌─────────┼─────────┐                          │
│   │ JSON    │ Fields  │ ← Çıkan veri            │
│   │ result  │ + satırlar│                        │
│   └─────────┼─────────┘                          │
│             │                                     │
│   ┌─────────┼─────────┐                          │
│   │ Excel   │ CSV     │ ← Dışa aktarma          │
│   │ / CSV   │         │                          │
│   └─────────┴─────────┘                          │
│                                                   │
└──────────────────────────────────────────────────┘
```

---

## 📊 Performans

| Sistem | Model | Sürat (token/s) |
|--------|-------|-----------------|
| Intel i7-13700K (CPU) | ImajeV-2B-Q8_0 | ~55 |
| NVIDIA RTX 4090 (24GB VRAM) | ImajeV-2B-Q8_0 | ~280 |
| NVIDIA RTX 4060 (8GB VRAM) | ImajeV-2B-Q8_0 | ~160 |
| Intel Arc A770 | ImajeV-2B-Q8_0 | ~140 |

> **Not**: Performans GPU modeline ve VRAM'a göre değişir. Modeli `--gpu-layers N` ile optimize edebilirsiniz.

---

## 📚 Dokümantasyon

- [📖 Hızlı Başlangıç](../docs/index.html) — Adım adım rehber
- [📋 API Referansı](../docs/api-reference.md) — Detaylı API dokümantasyonu
- [❓ Sık Sorulan Sorular](../docs/faq.html) — Sorun giderme

---

## 🔧 Geliştirme

```bash
git clone https://github.com/Nec0ti/Fatrocu.git
cd Fatrocu/fatrocu-cli
cargo build --release

# Debug modu
cargo run -- process --model "ImajeV-2B-Q8_0" --image test.pdf
```

Detaylı geliştirme rehberi için [CONTRIBUTING.md](CONTRIBUTING.md) dosyasına bakın.

---

## 🏆 Lisans

MIT Lisansı — [LICENSE](../LICENSE) dosyasına bakın.

---

<div align="center">

**Fatrocu CLI v3.1.0** · Rust + ImajeV-2B-Q8_0 · Tamamen Yerel

Made with ♥ by [Nec0ti](https://github.com/Nec0ti)

[GitHub](https://github.com/Nec0ti/Fatrocu) · [PyPI](https://pypi.org/project/fatrocu-cli/)

</div>
