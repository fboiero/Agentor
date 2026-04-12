# argentor-tee

Trusted Execution Environment (TEE) integration for Argentor.

> **Status: scaffolding only.** All providers are stubs. Real integration is
> planned per the roadmap below. This crate defines the traits, types, and
> verifier surface so higher layers can be written against the TEE abstraction
> today and swap in real backends later without API churn.

## What is a TEE, and why does it matter

A **Trusted Execution Environment (TEE)** is a hardware-isolated region of a
processor where code and data are protected from everything outside — the
operating system, the hypervisor, even the cloud provider. Memory is encrypted
with a key the host cannot access; execution is attested by a signature chained
to the CPU vendor's root of trust.

For Argentor, this matters because many regulated workloads cannot run in a
"the cloud provider can see my data" model:

- **Banking**: PCI-DSS, PSD2 — card data and cryptographic keys must never be
  observable by infrastructure operators.
- **Healthcare**: HIPAA, HDS — patient data handled by an AI agent must have
  provable isolation.
- **Defense / government**: classified workloads demand attestable, verifiable
  execution with a published measurement.
- **Confidential compute for AI**: protect model weights and inference inputs
  from the host.

## Provider comparison

| Feature                | AWS Nitro Enclaves       | Intel SGX                | AMD SEV-SNP              |
| ---------------------- | ------------------------ | ------------------------ | ------------------------ |
| Isolation granularity  | Full VM                  | Process / enclave        | Full VM                  |
| Memory limit           | Up to 512 GB             | 256 MB EPC (legacy), 512 GB (Scalable+) | Host memory |
| Attestation format     | COSE_Sign1 (NSM)         | ECDSA quote (DCAP)       | SNP report (VCEK-signed) |
| Access model           | EC2 parent instance only | Local + remote           | KVM guest + remote       |
| Ease of adoption       | **High** (managed)       | Medium (needs Gramine/Occlum) | Medium (needs KVM + QEMU) |
| Vendor lock-in         | AWS only                 | Intel CPUs only          | AMD EPYC only            |
| Side-channel exposure  | Low (no shared cache with host) | Moderate (shared cache attacks) | Low-moderate |
| Recommended first pick | **Yes** — easiest path   | No (complex tooling)     | No (infra-heavy)         |

## Roadmap

The providers will land in this order, driven by adoption cost vs. value:

1. **AWS Nitro Enclaves** — easiest to deploy (just an EC2 instance with
   `nitro_enclaves=enabled`), widest customer base, managed attestation root.
2. **AMD SEV-SNP** — second, because confidential VMs are increasingly offered
   by Azure, GCP, and OCI. One integration covers all three.
3. **Intel SGX** — third, because it requires binary-level tooling
   (Gramine/Occlum) and is narrower in hardware coverage.

For each provider the sequence is: capability detection → spawn/terminate →
attestation generation → attestation verification (vendor PKI) → key release
(sealed storage).

## Use cases

- **Regulated agent execution**: run an Argentor agent entirely inside an
  enclave so prompts, tool calls, and memory never leave encrypted memory.
- **Confidential tool invocation**: invoke sensitive tools (e.g. signing, KYC
  verification) only after the caller has presented a valid attestation.
- **Remote attestation for audit**: emit signed reports to compliance logs
  (argentor-compliance) proving which code was running at a given time.
- **Sealed key storage**: release a decryption key only to an enclave whose
  MRENCLAVE matches a published allowlist.

## Security caveats

TEEs are **strong**, but not a silver bullet:

- **Side-channel attacks**: SGX in particular has a history of cache-timing
  and speculative-execution leaks (Foreshadow, Plundervolt, ÆPIC). Keep
  microcode patched and apply vendor mitigations.
- **Availability**: the host can still kill your enclave. TEEs guarantee
  confidentiality and integrity, not availability.
- **Key management**: the security of the whole system hinges on the trust
  chain rooted at the vendor. Plan for root-key compromise (Intel's AESM
  rotations, AMD ARK updates).
- **Debug mode is insecure**: debug-mode enclaves disable memory encryption
  binding. Argentor enforces `debug_mode=false` by default — do not override
  in production.
- **Nonce freshness**: always bind attestation reports to a fresh verifier
  nonce. A replayed report proves nothing.
- **Measurement allowlist governance**: you are only as secure as the
  allowlist of accepted `MRENCLAVE` / `image_hash` values you maintain.

## Feature flags

```toml
argentor-tee = { version = "1", features = ["aws-nitro"] }
```

- `aws-nitro` — include the AWS Nitro Enclaves stub provider
- `intel-sgx` — include the Intel SGX stub provider
- `amd-sev` — include the AMD SEV-SNP stub provider
- `all-tee` — include all of the above

No feature is enabled by default — you opt into the specific backend your
deployment supports.

## License

AGPL-3.0-only, same as the rest of Argentor.
