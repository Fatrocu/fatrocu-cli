# Fatrocu CLI and shared core 3.1

Rust document extraction, human review and local export for Windows and Linux. This repository owns `fatrocu_core`; the native Windows desktop calls the library directly.

## Build and test

```text
cargo build --locked
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```

PDF text extraction is implemented in Rust. PDF page previews and scanned-PDF inference also require Poppler `pdftoppm` on PATH. Optionally, `pdftotext` adds exact word coordinates. Install Poppler through your operating system package manager or a trusted distribution. Images use PNG/JPEG directly.

## Local automation

Start the model with `fatrocu-server setup`, then `fatrocu-server start` on subsequent launches. The default endpoint is `http://127.0.0.1:1842/v1`; the single text/vision model is `unsloth/Qwen3.5-4B-GGUF:UD-Q4_K_XL`.

```text
fatrocu-cli profile add --name "Client legal name" --tax-id <VKN_OR_TCKN>
fatrocu-cli template list
fatrocu-cli process <file.xml> <scan.pdf> --profile <PROFILE_ID>
fatrocu-cli process <z-report.png> --profile <PROFILE_ID> --template okc --kind z-report
fatrocu-cli process <fuel-slip.jpg> --profile <PROFILE_ID> --template okc --kind utts
fatrocu-cli list
fatrocu-cli show <DOCUMENT_ID>
fatrocu-cli review <DOCUMENT_ID> --revision 1 --set seller_name="Correct legal name"
fatrocu-cli approve <DOCUMENT_ID> --revision 2
fatrocu-cli export --output <NEW_OUTPUT_PATH.xlsx> --format xlsx
fatrocu-cli reopen <DOCUMENT_ID> --revision 3
```

Use `--no-model` for deterministic/manual imports; unreadable fields remain null and require review. Nothing is auto-approved. Every edit/approval/reopen/delete requires the current revision, so an automation cannot overwrite a concurrent review. Approved records must be reopened before editing. `review --input` accepts the full Document returned by `show.document`; `--taxes` accepts a TaxRow array. `--set field=` clears a value.

Approval rejects missing required values until the reviewer corrects them or individually acknowledges the issue with `--acknowledge-missing <FIELD_PATH>`. Acknowledgement retains the null and needs-review flag; it does not invent a value. Invalid dates, tax IDs, decimals and coordinates cannot be acknowledged away.

For e-Archive, matching seller/buyer tax IDs determine sale/purchase. Neither or both matching leaves direction null. Z reports use Z number; UTTS additionally requires the plate. Original tax rows are retained. Merging groups only complete rows with the same tax type and exact rate.

## Settings, credentials and privacy

```text
fatrocu-cli settings show
fatrocu-cli settings set --default-export csv
fatrocu-cli settings set --endpoint <HTTPS_BASE_URL/v1> --model <MODEL_ID> --api-key-env MODEL_API_KEY
fatrocu-cli settings set --protocol anthropic --endpoint https://api.anthropic.com/v1 --model <MODEL_ID> --api-key-env ANTHROPIC_API_KEY
fatrocu-cli connection-check
fatrocu-cli backup <NEW_BACKUP_DIRECTORY>
```

Credentials are read from the named environment variable. They are never persisted in settings. OpenAI-compatible chat models must support text/image messages, JSON mode, temperature and max_tokens. Claude uses Messages protocol version 2023-06-01. Provider accounts, model availability and billing are managed by the user; live hosted-provider tests are not run without credentials and explicit data-transfer authorization.

For every non-loopback inference endpoint, the CLI prints the exact destination disclosure to stderr and refuses document transfer without `--allow-remote-document-transfer`. HTTP redirects and HTTP proxy use are disabled for model requests. No remote endpoint is configured by default. Connection checks do not send documents. Startup version checks use only release metadata and never alter JSON stdout.

The application-data directory is shared with the desktop. Override it with `FATROCU_DATA_DIR` or `--data-dir`. It contains SQLite records and retained originals, outside the installation. The core rejects data/backup/export destinations inside Git checkouts. Delete a document with `delete <ID> --revision <N>`; back up first if you need to retain it. Backup copies the database and source directory to a fresh destination.

Telemetry preference defaults to enabled and can be changed with `settings set --telemetry false`. **Package collection and purge are deferred until the user decides its contents and disable/delete behavior.** No telemetry package is created, uploaded or erased by this implementation.

## Exports and custom templates

Only approved records can be exported. The saved default format is used when `--format` is omitted. XLSX has Documents and Taxes sheets; exact decimal text avoids numeric precision loss, and identifiers remain strings. CSV repeats document fields per tax row and neutralizes formula-like values. JSON includes the shared record schema. Outputs never overwrite an existing file.

`template add <DEFINITION.json>` saves a named custom template extending a built-in family. Definitions must retain built-in keys; see [schema and templates](docs/schema.md).

## Updates and extraction gate

`fatrocu-cli update` shows the latest published stable GitHub Release and its notes. `update --install` selects this OS/architecture, verifies its published SHA-256 digest and publisher identity, then installs with rollback. Windows requires the signed bundled helper and a confirmed certificate fingerprint in `FATROCU_TRUSTED_PUBLISHER_SHA256`; Linux requires minisign and a confirmed `FATROCU_TRUSTED_MINISIGN_KEY`. Missing trust information causes a refusal, never an unchecked install. See [release packaging](docs/releases.md).

Extraction benchmark: **NOT RUN (dataset pending)**. Do not enable it until the user explicitly confirms readiness and the workspace agent instructions are updated. See [private benchmark format](docs/benchmark.md) and the workspace `benchmarks.md`. Unit and integration tests use synthetic data and do not satisfy the private accuracy gate. No code PR may open until both template gates pass; no PR is merged by this work.
