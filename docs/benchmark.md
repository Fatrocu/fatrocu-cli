# Private extraction benchmark

Hold: **NOT RUN (dataset pending)**. The command is implemented but deliberately disabled until the user's readiness confirmation and instruction updates. Do not put any real documents, labels, manifest or generated accuracy report in Git.

Inside the private workspace-root assets/ directory, create manifest.json with version, model_revision (the exact model repository revision) and documents. Each case has source and ground_truth relative paths. Source files must be distinct; duplicate hashes cannot count toward the ten-document minimum per family. All paths must resolve inside assets/.

Each human-reviewed ground-truth file has kind, profile, fields and taxes. Profile has id, legal_name and tax_id. Fields map every applicable required key to a Field; label optional fields too when applicable. Ground truth never derives a source value from an assumption. Absent/unreadable values are null with needs_review=true. Taxes use the same nullable Field representation for tax_type/rate/base/amount. Use `fatrocu-cli schema --kind z-report` for a blank extraction envelope, then add kind/profile around fields/taxes for ground truth. No filled labels are shipped in this repository.

After the user confirms readiness, update agent.md, agents.md, claude.md and benchmarks.md and resolve BENCH-001 first. Then set FATROCU_BENCHMARK_ENABLED=1 and FATROCU_BENCHMARK_DATA to the absolute workspace assets/ path. Run:

```text
fatrocu-cli benchmark --dataset <ABSOLUTE_ASSETS_PATH> --report <NEW_PRIVATE_REPORT_PATH> --commit <CANDIDATE_COMMIT>
```

Run once at PR readiness, after normal tests. The local model server must be running; remote inference is rejected for benchmarks. Incomplete corpora, bad labels and failures exit nonzero. No performance benchmark is required.

Each family (e_archive and okc, including Z/UTTS) must independently meet 95% required-field exact match, 98% critical-field exact match and 90% exact documents. Critical hallucinations fail immediately. Both merged and unmerged tax views are scored. Taxes are assigned independent of row order with maximum matching field score; exact duplicate rows retain multiplicity. Formatting normalization never changes digits or treats an extraction miss as correct. Percent gates compare integer counts without rounding.

Reports contain commit, model ID/revision, schema version, dataset version/manifest hash, field counts, critical hallucination counts and exact-document counts. They contain no source bytes, expected/actual field values, filenames, taxpayer IDs or telemetry. Share only aggregate summaries after the gate actually passes.
