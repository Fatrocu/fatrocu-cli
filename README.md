# Fatrocu CLI

> İmajeV-2B-Q8_0 ile güçlendirilmiş, tamamen yerel çalışan fatura işleme aracı. OCR + alan çıkarma tek model çağrısında.

<div align="center">

**[v3.1.0](https://github.com/Nec0ti/Fatrocu/releases)** — *25 Eylül 2026*

[![Version](https://img.shields.io/badge/version-3.1.0-blue)](https://github.com/Nec0ti/Fatrocu/releases)
[![License](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Python](https://img.shields.io/badge/python-3.10+-blue)](https://www.python.org/)
[![Rust](https://img.shields.io/badge/rust-1.81+-orange)](https://www.rust-lang.org/)

[![GitHub Pages](https://img.shields.io/badge/docs-https%3A%2F%2Fnec0ti.github.io%2FFatrocu-green)](https://nec0ti.github.io/Fatrocu/docs)
[![GitHub](https://img.shields.io/badge/GitHub-Nec0ti%2FFatrocu-red)](https://github.com/Nec0ti/Fatrocu)

</div>

---

## 🎯 Özellikler

- **⚡ Hızlı** — ~55 token/s (CPU) / ~280 token/s (GPU)
- **🔒 Yerel** — Veri hiç internete gitmez
- **🤖 İmajeV-2B-Q8_0** — 2B parametreli, hafif ve güçlü model
- **📄 PDF & Görsel** — Otomatik sayfa ayrıştırma
- **📊 Excel Export** — İşlenen faturaları Excel/CSV olarak dışa aktarın
- **💻 Tamamen Yerel** — Hiçbir API'ye ihtiyaç yok

## 📸 Ekran Görüntüleri

<div align="center" style="display:grid;grid-template-columns:repeat(auto-fit,minmax(250px,1fr));gap:12px;">
    <div style="padding:12px;background:var(--bg-card);border:2px dashed var(--border-default);border-radius:12px;">
        <div style="font-size:0.8rem;color:var(--text-muted);margin-bottom:8px;">Terminal</div>
        <div style="font-family:monospace;font-size:0.85rem;white-space:pre-wrap;">fatrocu status<br>✅ Online: ✓<br>✅ Model: İmajeV-2B-Q8_0<br>✅ Device: CPU</div>
    </div>
    <div style="padding:12px;background:var(--bg-card);border:2px dashed var(--border-default);border-radius:12px;">
        <div style="font-size:0.8rem;color:var(--text-muted);margin-bottom:8px;">Excel Çıktı</div>
        <div style="font-size:0.85rem;">Fatura No | Tarih | Cari | Tutar<br>12345 | 2026-01-15 | ABC Ltd. | 12,500.00 TL</div>
    </div>
</div>

## 🚀 Hızlı Başlangıç

### 1. Kurulum

```bash
git clone https://github.com/Nec0ti/Fatrocu.git
cd Fatrocu/fatrocu-cli
cargo build --release
```

### 2. Model İndirme

```bash
cargo run --release -- models --download "ImajeV-2B-Q8_0"
```

### 3. İlk İşlem

```bash
cargo run --release -- process --model "ImajeV-2B-Q8_0" --image "invoice.pdf" --output "result.json"
```

## 📖 Dokümantasyon

- [Ana Sayfa](https://nec0ti.github.io/Fatrocu/docs) — Genel özellikler
- [Hızlı Başlangıç](https://nec0ti.github.io/Fatrocu/docs/quickstart.html) — Adım adım rehber
- [API Referansı](https://nec0ti.github.io/Fatrocu/docs/api-reference.md) — Komut tablosu
- [Modeller](https://nec0ti.github.io/Fatrocu/docs/models.html) — Model karşılaştırması
- [Sık Sorulan Sorular](https://nec0ti.github.io/Fatrocu/docs/faq.html) — Sorun giderme

## 🛠️ Komutlar

| Komut | Açıklama |
|--------|-----------|
| `fatrocu process` | Fatura işleme |
| `fatrocu models --download` | Model indirme |
| `fatrocu models --list` | Model listesi |
| `fatrocu export --format xlsx` | Excel export |
| `fatrocu status` | Motor durumu |

## 📋 Gereksinimler

- **İşletim Sistemi:** Windows 10/11 x64, Linux, macOS
- **RAM:** 8 GB (minimum), 16 GB+ önerilir
- **GPU (opsiyonel):** 6 GB+ VRAM, NVIDIA CUDA 12+ veya AMD ROCm
- **Disk:** ~7 GB boş alan

## 🧪 Sistem Gereksinimleri

| Model | Parametre | RAM | GPU VRAM | CPU (token/s) | GPU (token/s) |
|-------|-----------|-----|----------|----------------|---------------|
| İmajeV-2B-Q8_0 | 2B | ~8 GB | ~8 GB | ~55 | ~280 |
| Gemma-4-E2B-It | 2.8B | ~9 GB | ~10 GB | ~48 | ~220 |
| Gemma-4-E4B-It | 4.8B | ~16 GB | ~20 GB | ~35 | ~180 |
| Mistral-Nemo-1.5B | 1.5B | ~4.5 GB | ~5 GB | ~120 | ~450 |

## 📄 Lisans

MIT License — [Lisans](LICENSE)

---

<div align="center">
  <a href="https://github.com/Nec0ti/Fatrocu">github.com/Nec0ti/Fatrocu</a>
</div>
