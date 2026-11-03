# Shared schema and custom templates

Schema version: 1. Rust types in `src/model.rs` are authoritative. Every source field has this representation:

```json
{"value": null, "source": null, "needs_review": true}
```

A readable value is a string. Dates use YYYY-MM-DD; decimal values use decimal strings, never binary floating point. Original-language legal names and source identifiers are preserved. A source region, when available, is `{ "page": 1, "x": 0.1, "y": 0.1, "width": 0.2, "height": 0.05 }`: one-based page, top-left normalized coordinates. Absent evidence has a null source. Human edits do not manufacture coordinates.

Document contains schema_version, id, revision, profile_id, template_id, kind (e_archive/z_report/utts), file_name, source_sha256, created_at, status (review/approved), fields, original taxes, merge_taxes, accepted_missing, extraction_method, model and approved_at. Source bytes remain in the managed source directory; the record does not embed them. `show` returns document, issues and the local source path.

Built-in scalar keys:

| Family | Keys |
| --- | --- |
| Both | document_number, date, seller_tax_id, seller_name, gross_total, currency |
| e-Archive | buyer_tax_id, buyer_name, direction |
| OKC | vehicle_plate (required for UTTS only) |

Kind is the document type. For OKC document_number is Z no, never a requested receipt number. Required scalar fields are those in the plan; currency is retained when printed. Each tax row has tax_type, rate, base and amount, each a Field. UBL VAT code 0015 is mapped to VAT; other printed names/codes are retained. Rows are not inferred from totals, and missing tax values remain null. Templates add optional named fields; they cannot remove the built-in fields.

To create a template, copy the matching object from `template list`, change id and name, set builtin to false, and extend fields with ASCII keys and English labels. Keep family as e_archive or z_report. The desktop Settings editor supports the same operation. Custom labels are data, not executable code.

Deterministic inputs: UBL Invoice XML; HTML data-field attributes and explicit two-column label/value tables; explicitly labeled text/PDF lines. Free-layout text and scans use the configured model. Unsupported, encrypted or malformed documents fail with an actionable error and are never replaced with invented extracted values.

No cross-document derivation, taxpayer guessing, tax calculation from absent values, automatic approval or automatic telemetry upload occurs.
