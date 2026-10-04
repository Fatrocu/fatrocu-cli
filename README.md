# Fatrocu CLI

Fatrocu CLI is a command‑line tool for processing invoices locally using the Moondream 3.1‑9B‑A2B model. It runs entirely on the host machine and does not send any data to external services.

## Features

- Process a single image or PDF file and output JSON results.
- Download and manage the required GGUF model files.
- Adjustable parameters: temperature, number of predictions, and context size.
- Fully offline operation; no internet connection required after the model is downloaded.

## Installation

```sh
# Clone the repository
git clone https://github.com/Fatrocu/fatrocu-cli.git
cd fatrocu-cli

# Build the binary (requires Rust 1.75+)
cargo build --release
```

The executable will be created at `target/release/fatrocu-cli.exe`.

## Usage

```sh
# Show help
./target/release/fatrocu-cli.exe --help

# Download the model (once)
./target/release/fatrocu-cli.exe models --download "Moondream 3.1-9B-A2B"

# Process an invoice image or PDF
./target/release/fatrocu-cli.exe process \
  --model "Moondream 3.1-9B-A2B" \
  --image "invoice.pdf" \
  --temp 0.2 \
  --n-predict 8 \
  --ctx-size 256
```

## Available Commands

- `process` – Run the model on a supplied image or PDF.
- `models --list` – List available models.
- `models --download <model>` – Download a model from Hugging Face.
- `status` – Show the current configuration and model status.
- `export` – Export the JSON result to CSV or Excel.

## Model Management

Model files are stored in the `models/` directory:

```
models/
├─ Moondream-3.1-9B-A2B.gguf
├─ Moondream-3.1-9B-A2B-mmproj-f16.gguf
```

The `models --download` command retrieves both files automatically.

## License

MIT License – see the `LICENSE` file.
