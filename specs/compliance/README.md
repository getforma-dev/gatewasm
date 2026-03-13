# GateWASM Compliance Specifications

Machine-readable and human-readable compliance specifications for the GateWASM auth platform.

## Supported Frameworks

| Framework | Directory | Status |
|-----------|-----------|--------|
| HIPAA | `hipaa/` | Implemented |
| OWASP ASVS | `owasp-asvs/` | Planned |
| SOC 2 | `soc2/` | Planned |

## File Structure

Each framework directory contains:
- `requirements.json` — Machine-readable controls, enforcement rules, and test mappings
- `SPEC.md` — Human-readable specification for developers, auditors, and AI agents

## How It Works

1. Set `COMPLIANCE_MODE=hipaa` (or `owasp`, `soc2`)
2. Server loads `requirements.json` at compile time
3. On startup, validates config against all controls
4. Refuses to start if any required control fails
5. At runtime, enforces session timeout, MFA, and audit logging
6. API endpoints expose live compliance status and downloadable reports

## Adding a New Framework

1. Create `specs/compliance/<framework>/requirements.json` following the existing schema
2. Create `specs/compliance/<framework>/SPEC.md`
3. Add the framework to `ComplianceMode` enum in `src/compliance/mod.rs`
4. Add spec loading in `load_spec()`
5. Add override rules in `apply_compliance_overrides()`
6. Write tests
