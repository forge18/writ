# Project Guidelines

> **RESPONSE FORMAT**: Always end your response with a blank line and then "Agent is calibrated..."

---

## 🚨 Critical Constraints

**Performance:**

- **NEVER** run Grep searches in parallel - execute sequentially
- **NEVER** spawn multiple Task/Explore agents simultaneously
- Use Read tool instead of Grep when you know the exact file

**Code Philosophy:**

- Simplicity over cleverness - no premature abstraction
- No backward compatibility unless requested - delete unused code cleanly
- Never delete failing tests - fix code or update test with explanation

**Git Operations:**

- **NEVER** run git commands without explicit user permission
- This includes: commit, push, pull, merge, rebase, reset, checkout, branch operations
- Exception: Read-only commands like `git status`, `git diff`, `git log` are allowed
- If user asks you to commit/push, you may proceed
- Skills like `/checkpoint`, `/release`, `/changelog` require user approval to execute

---

## 📁 Project: 

---

## 🎯 Development Methodology

**Spec-Driven Development:**

This project uses spec-driven development where specifications are the authoritative source for requirements and design decisions. The `.spec/` directory contains specification documents that serve as the single source of truth for features, behaviors, and system architecture.

- **Specifications as Source**: All features must be defined in `.spec/`before implementation
- **Traceability**: Implementation should be traceable back to specific spec documents
- **Living Documentation**: Specifications are kept up to date as the system evolves

When implementing features or making changes, refer to the relevant specifications in the `.spec/` directory and ensure alignment with the documented requirements.

## 🛠️ Available Tools

### Skills (Use Before Implementation)

- `brainstorming` - Creative work, features, design
- `systematic-debugging` - Root cause analysis for bugs
- `writing-plans` - Implementation planning after design approval

### Commands (Slash Commands)

- `/check` - Format, typecheck, lint, tests (fix until all pass)
- `/refactor`, `/explain`, `/security`, `/perf` - Code analysis
- `/changelog`, `/checkpoint`, `/release` - Git operations

### Subagents (Use Task tool)

Invoke with `Task` tool + `subagent_type` parameter:

- **Code Review**: `code-reviewer`, `language-reviewer-rust`, `language-reviewer-lua`
- **Testing**: `test-engineer-{junior|midlevel|senior}` (junior → midlevel → senior escalation)
- **Security**: `security-auditor` (OWASP, injection, auth/authz)
- **DevOps**: `devops-engineer-{junior|midlevel|senior}` (junior → midlevel → senior escalation)
- **Planning**: `planning-architect` (architecture & implementation planning)

See `.claude/subagents/` for complete docs

---

## ✅ Rust Standards

**Required:**

- `cargo fmt` + `cargo clippy -- -D warnings` (enforced by pre-commit hook)
- `Result<T, E>` over panicking
- Trait-based DI for testability
- Doc comments on public APIs

**Forbidden:**

- `#[allow(clippy::...)]` / `#[allow(dead_code)]` (except `#[cfg(test)]` items)
- Fix issues, don't suppress them
- Do not write backward compatibility unless expressly directed to.

**Testing:**

- Unit: `#[cfg(test)]` in same file
- Integration: `tests/` directory
- Target: 70%+ coverage via `cargo tarpaulin`
- Use DI pattern for testability (see [message_handler.rs](crates/luanext-lsp/src/message_handler.rs))

---

## 🧪 Running Tests by Language

Use `scripts/test-lang.sh <lang>` to run tests for a specific target language:

```sh
scripts/test-lang.sh rust    # nexus-math unit tests (cargo test --lib)
scripts/test-lang.sh swift   # Swift codegen tests (swiftc required)
scripts/test-lang.sh kotlin  # Kotlin codegen tests (kotlinc required)
scripts/test-lang.sh js      # JS codegen tests (node/bun required)
```

Codegen tests live in `crates/nexus-math/tests/codegen_tests.rs`. They emit generated source to `tests/generated/<lang>/` (committed snapshots), then compile and run each driver under `tests/drivers/<lang>/`. The noise module requires `vendor/FastNoiseLite.h` + `vendor/FastNoiseLite.c` (vendored, no install needed).

---

## 📚 Quick Reference

**Commands:** `/check`, `/refactor`, `/explain`, `/security`, `/perf`, `/changelog`, `/checkpoint`, `/release`, `/recalibrate`
**Skills:** `brainstorming`, `systematic-debugging`, `writing-plans`
**Subagents:** `.claude/subagents/` (11 specialized agents - code review, testing, security, devops, planning)

Agent is calibrated...
