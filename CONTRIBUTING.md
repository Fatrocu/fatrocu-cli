# Contributing to Fatrocu CLI v3.1

> Thank you for your interest in contributing to Fatrocu CLI!

## 📋 Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Project Structure](#project-structure)
- [How to Contribute](#how-to-contribute)
- [Coding Standards](#coding-standards)
- [Pull Request Process](#pull-request-process)
- [Release Process](#release-process)

---

## Code of Conduct

- Be respectful and patient
- Be constructive in discussions
- Be welcoming to newcomers
- Assume good intent
- Be concise and patient

---

## Getting Started

### Prerequisites

- Rust 1.75+
- Python 3.10+
- Git
- VS Code or your preferred editor

### Setup

```bash
# Clone the repository
git clone https://github.com/Nec0ti/Fatrocu.git
cd Fatrocu/fatrocu-cli

# Install Rust toolchain
rustup install stable

# Install Python dependencies
pip install torch torchvision torchaudio --index-url https://download.pytorch.org/whl/cu124
pip install excelize python-dateutil python-dotenv

# Build in development mode
cargo build

# Run tests
cargo test
```

---

## Project Structure

```
fatrocu-cli/
├── Cargo.toml              # Rust package manifest
├── src/
│   └── main.rs            # CLI entry point
├── fatrocu-cli/           # Python CLI wrapper (optional)
├── docs/
│   ├── index.html         # Main documentation
│   ├── styles.css         # Styling
│   ├── api-reference.md   # API reference
│   └── faq.html           # FAQ
├── README.md
├── CONTRIBUTING.md        # This file
└── LICENSE
```

---

## How to Contribute

### Bug Reports

When reporting a bug, please include:

1. **Expected behavior** — What should happen?
2. **Actual behavior** — What actually happened?
3. **Steps to reproduce** — Clear, step-by-step instructions
4. **Environment** — OS, Rust version, model version, hardware
5. **Logs/Error messages** — Full error output

Example:

```markdown
### Bug Report

**OS:** Windows 11 x64
**Rust:** 1.79.0
**Model:** ImajeV-2B-Q8_0
**Error:** `fatrocu process` fails with "llama-cli not found"

**Steps to reproduce:**
1. Run `fatrocu models --download ImajeV-2B-Q8_0`
2. Run `fatrocu process --model ImajeV-2B-Q8_0 --image invoice.pdf`
3. Error: `Error: llama-cli not found`

**Expected:** Model should be downloaded and used
**Actual:** Error about missing llama-cli
```

### Feature Requests

When requesting a feature, please include:

- Use case / motivation
- Why it's needed
- Example of desired behavior
- Any alternatives considered

### Pull Requests

1. Fork the repository
2. Create a branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Write tests (if applicable)
5. Write documentation
6. Commit with clear messages
7. Push to your fork
8. Open a Pull Request

**PR Checklist:**

- [ ] Code follows the style guide
- [ ] Tests pass (`cargo test`)
- [ ] Documentation is updated
- [ ] No new warnings (`cargo clippy`)
- [ ] Commit messages are clear and descriptive

---

## Coding Standards

### Rust

- Use `clippy` for linting
- Follow the [Rust style guide](https://rust-lang.github.io/api-guidelines/)
- Add doc comments for public functions
- Use `Result<T, E>` instead of panicking (unless it's a recoverable panic)

### Python (fatrocu-cli wrapper)

- Follow [PEP 8](https://pep8.org/)
- Use type hints
- Add docstrings with type hints and return types
- Use `mypy` for type checking

### Documentation

- Write clear, concise documentation
- Use markdown for formatting
- Include examples where applicable
- Keep the tone friendly and approachable

---

## Pull Request Process

1. Update the README.md with details of changes if appropriate
2. Update the CHANGELOG.md
3. Ensure all tests pass
4. Update documentation
5. Ensure code is linted and passes clippy
6. Request review from maintainers
7. Address review feedback

---

## Release Process

The release process is automated:

1. Update version in `Cargo.toml`
2. Update CHANGELOG.md
3. Commit with `chore: release vX.Y.Z`
4. Push to main branch
5. CI will build and publish to PyPI

Manual releases:

1. Tag the release: `git tag v3.1.0`
2. Push the tag: `git push origin v3.1.0`
3. CI will create a GitHub release with binaries

---

## Questions?

- Open an issue on [GitHub](https://github.com/Nec0ti/Fatrocu/issues)
- Email: nec0ti@proton.me
- Twitter: [@nec0ti](https://twitter.com/nec0ti)

Thank you for contributing! 🎉
